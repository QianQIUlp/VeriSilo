use std::{
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, TcpListener, TcpStream, ToSocketAddrs},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::{
    domain::{NetworkProfile, ProxyScheme},
    vault::ProxyAuthentication,
};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_HTTP_RESPONSE_HEADER: usize = 16 * 1024;

#[derive(Debug, Clone)]
pub struct RelayEndpoint {
    pub host: String,
    pub port: u16,
}

#[derive(Debug)]
pub struct ProxyRelay {
    endpoint: RelayEndpoint,
    shutdown: Arc<AtomicBool>,
    accept_thread: Option<JoinHandle<()>>,
}

#[derive(Debug, Error)]
pub enum ProxyRelayError {
    #[error("本机代理中继只支持固定 HTTP 或 SOCKS5 上游。")]
    UnsupportedProxy,
    #[error("SOCKS5 用户名和密码各自不能超过 255 字节。")]
    SocksCredentialTooLong,
    #[error("无法启动或验证本机代理中继：{0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpstreamPreflight {
    pub authentication_verified: bool,
}

struct RelayConfig {
    scheme: ProxyScheme,
    host: String,
    port: u16,
    authentication: Option<ProxyAuthentication>,
}

#[derive(Debug)]
struct SocksTarget {
    encoded: Vec<u8>,
    authority: String,
}

impl ProxyRelay {
    pub fn start(
        profile: &NetworkProfile,
        authentication: Option<ProxyAuthentication>,
    ) -> Result<Self, ProxyRelayError> {
        let NetworkProfile::FixedProxy {
            scheme, host, port, ..
        } = profile
        else {
            return Err(ProxyRelayError::UnsupportedProxy);
        };
        if !matches!(scheme, ProxyScheme::Http | ProxyScheme::Socks5) {
            return Err(ProxyRelayError::UnsupportedProxy);
        }
        if matches!(scheme, ProxyScheme::Socks5)
            && authentication.as_ref().is_some_and(|authentication| {
                authentication.username().as_bytes().len() > u8::MAX as usize
                    || authentication.password().as_bytes().len() > u8::MAX as usize
            })
        {
            return Err(ProxyRelayError::SocksCredentialTooLong);
        }

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let endpoint = RelayEndpoint {
            host: Ipv4Addr::LOCALHOST.to_string(),
            port: address.port(),
        };
        let config = Arc::new(RelayConfig {
            scheme: scheme.clone(),
            host: host.clone(),
            port: *port,
            authentication,
        });
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let accept_thread = thread::spawn(move || {
            while !thread_shutdown.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let connection_config = Arc::clone(&config);
                        thread::spawn(move || {
                            let _ = handle_local_socks_connection(stream, &connection_config);
                        });
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            endpoint,
            shutdown,
            accept_thread: Some(accept_thread),
        })
    }

    pub fn endpoint(&self) -> &RelayEndpoint {
        &self.endpoint
    }

    pub fn supports(profile: &NetworkProfile) -> bool {
        matches!(
            profile,
            NetworkProfile::FixedProxy {
                scheme: ProxyScheme::Http | ProxyScheme::Socks5,
                ..
            }
        )
    }

    pub fn preflight_upstream(
        profile: &NetworkProfile,
        authentication: Option<&ProxyAuthentication>,
    ) -> Result<UpstreamPreflight, ProxyRelayError> {
        let NetworkProfile::FixedProxy {
            scheme, host, port, ..
        } = profile
        else {
            return Err(ProxyRelayError::UnsupportedProxy);
        };
        let mut upstream = connect_with_timeout(host, *port)?;
        upstream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
        upstream.set_write_timeout(Some(HANDSHAKE_TIMEOUT))?;
        match scheme {
            ProxyScheme::Socks5 => {
                negotiate_socks5_authentication(&mut upstream, authentication)?;
                Ok(UpstreamPreflight {
                    authentication_verified: authentication.is_some(),
                })
            }
            ProxyScheme::Http => Ok(UpstreamPreflight {
                // HTTP Basic/Digest authentication is request-target dependent.
                // It is verified only by the user-triggered browser exit check.
                authentication_verified: false,
            }),
            ProxyScheme::Https | ProxyScheme::Socks4 => Err(ProxyRelayError::UnsupportedProxy),
        }
    }
}

impl Drop for ProxyRelay {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        let _ = TcpStream::connect((Ipv4Addr::LOCALHOST, self.endpoint.port));
        if let Some(thread) = self.accept_thread.take() {
            let _ = thread.join();
        }
    }
}

fn handle_local_socks_connection(mut client: TcpStream, config: &RelayConfig) -> io::Result<()> {
    client.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    client.set_write_timeout(Some(HANDSHAKE_TIMEOUT))?;

    let mut greeting = [0_u8; 2];
    client.read_exact(&mut greeting)?;
    if greeting[0] != 5 || greeting[1] == 0 || greeting[1] > 16 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid local SOCKS5 greeting",
        ));
    }
    let mut methods = vec![0_u8; greeting[1] as usize];
    client.read_exact(&mut methods)?;
    if !methods.contains(&0) {
        client.write_all(&[5, 0xff])?;
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "local SOCKS5 client did not offer no-auth",
        ));
    }
    client.write_all(&[5, 0])?;

    let target = match read_socks_target(&mut client) {
        Ok(target) => target,
        Err(error) => {
            let _ = write_socks_reply(&mut client, 8);
            return Err(error);
        }
    };
    let upstream = match connect_upstream(config, &target) {
        Ok(upstream) => upstream,
        Err(error) => {
            let _ = write_socks_reply(&mut client, 1);
            return Err(error);
        }
    };
    write_socks_reply(&mut client, 0)?;
    client.set_read_timeout(None)?;
    client.set_write_timeout(None)?;
    relay_bidirectionally(client, upstream)
}

fn read_socks_target(client: &mut impl Read) -> io::Result<SocksTarget> {
    let mut request = [0_u8; 4];
    client.read_exact(&mut request)?;
    if request[0] != 5 || request[1] != 1 || request[2] != 0 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "only SOCKS5 CONNECT is supported",
        ));
    }

    let mut encoded = vec![request[3]];
    let host = match request[3] {
        1 => {
            let mut bytes = [0_u8; 4];
            client.read_exact(&mut bytes)?;
            encoded.extend_from_slice(&bytes);
            IpAddr::V4(Ipv4Addr::from(bytes)).to_string()
        }
        3 => {
            let mut length = [0_u8; 1];
            client.read_exact(&mut length)?;
            if length[0] == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "empty SOCKS5 hostname",
                ));
            }
            let mut bytes = vec![0_u8; length[0] as usize];
            client.read_exact(&mut bytes)?;
            let hostname = String::from_utf8(bytes.clone()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "non-UTF8 SOCKS5 hostname")
            })?;
            if !hostname.is_ascii() || hostname.chars().any(char::is_control) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid SOCKS5 hostname",
                ));
            }
            encoded.push(length[0]);
            encoded.extend_from_slice(&bytes);
            hostname
        }
        4 => {
            let mut bytes = [0_u8; 16];
            client.read_exact(&mut bytes)?;
            encoded.extend_from_slice(&bytes);
            IpAddr::V6(Ipv6Addr::from(bytes)).to_string()
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported SOCKS5 address type",
            ))
        }
    };
    let mut port_bytes = [0_u8; 2];
    client.read_exact(&mut port_bytes)?;
    encoded.extend_from_slice(&port_bytes);
    let port = u16::from_be_bytes(port_bytes);
    let authority = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    Ok(SocksTarget { encoded, authority })
}

fn connect_upstream(config: &RelayConfig, target: &SocksTarget) -> io::Result<TcpStream> {
    let mut upstream = connect_with_timeout(&config.host, config.port)?;
    upstream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    upstream.set_write_timeout(Some(HANDSHAKE_TIMEOUT))?;
    match config.scheme {
        ProxyScheme::Http => authenticate_http_proxy(&mut upstream, config, target)?,
        ProxyScheme::Socks5 => authenticate_socks5_proxy(&mut upstream, config, target)?,
        ProxyScheme::Https | ProxyScheme::Socks4 => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "unsupported authenticated upstream proxy",
            ))
        }
    }
    upstream.set_read_timeout(None)?;
    upstream.set_write_timeout(None)?;
    Ok(upstream)
}

fn authenticate_http_proxy(
    upstream: &mut TcpStream,
    config: &RelayConfig,
    target: &SocksTarget,
) -> io::Result<()> {
    let authorization = if let Some(authentication) = config.authentication.as_ref() {
        let plaintext = Zeroizing::new(format!(
            "{}:{}",
            authentication.username(),
            authentication.password()
        ));
        let credentials = Zeroizing::new(STANDARD.encode(plaintext.as_bytes()));
        Zeroizing::new(format!(
            "Proxy-Authorization: Basic {}\r\n",
            credentials.as_str()
        ))
    } else {
        Zeroizing::new(String::new())
    };
    let request = Zeroizing::new(format!(
        "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n{}Proxy-Connection: Keep-Alive\r\n\r\n",
        authorization.as_str(),
        authority = target.authority,
    ));
    upstream.write_all(request.as_bytes())?;

    let mut header = Vec::new();
    while header.len() < MAX_HTTP_RESPONSE_HEADER {
        let mut byte = [0_u8; 1];
        upstream.read_exact(&mut byte)?;
        header.push(byte[0]);
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    if !header.ends_with(b"\r\n\r\n") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "upstream HTTP proxy response header is invalid",
        ));
    }
    let status_line = header
        .split(|byte| *byte == b'\n')
        .next()
        .and_then(|line| std::str::from_utf8(line).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid HTTP status line"))?;
    let status = status_line
        .split_ascii_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid HTTP proxy status"))?;
    if !(200..300).contains(&status) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("upstream HTTP proxy rejected authentication with status {status}"),
        ));
    }
    Ok(())
}

fn authenticate_socks5_proxy(
    upstream: &mut TcpStream,
    config: &RelayConfig,
    target: &SocksTarget,
) -> io::Result<()> {
    negotiate_socks5_authentication(upstream, config.authentication.as_ref())?;

    let mut request = vec![5, 1, 0];
    request.extend_from_slice(&target.encoded);
    upstream.write_all(&request)?;
    consume_socks5_reply(upstream)
}

fn negotiate_socks5_authentication(
    upstream: &mut TcpStream,
    authentication: Option<&ProxyAuthentication>,
) -> io::Result<()> {
    let method = if authentication.is_some() { 2 } else { 0 };
    upstream.write_all(&[5, 1, method])?;
    let mut response = [0_u8; 2];
    upstream.read_exact(&mut response)?;
    if response != [5, method] {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            if authentication.is_some() {
                "upstream SOCKS5 proxy refused username/password authentication"
            } else {
                "upstream SOCKS5 proxy refused no-authentication mode"
            },
        ));
    }
    let Some(authentication) = authentication else {
        return Ok(());
    };
    let username = authentication.username().as_bytes();
    let password = authentication.password().as_bytes();
    if username.len() > u8::MAX as usize || password.len() > u8::MAX as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "SOCKS5 credentials exceed the protocol limit",
        ));
    }
    let mut request = Vec::with_capacity(username.len() + password.len() + 3);
    request.extend_from_slice(&[1, username.len() as u8]);
    request.extend_from_slice(username);
    request.push(password.len() as u8);
    request.extend_from_slice(password);
    upstream.write_all(&request)?;
    let mut authentication_response = [0_u8; 2];
    upstream.read_exact(&mut authentication_response)?;
    if authentication_response != [1, 0] {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "upstream SOCKS5 proxy rejected credentials",
        ));
    }
    Ok(())
}

fn consume_socks5_reply(upstream: &mut TcpStream) -> io::Result<()> {
    let mut response = [0_u8; 4];
    upstream.read_exact(&mut response)?;
    if response[0] != 5 || response[1] != 0 || response[2] != 0 {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            format!("upstream SOCKS5 CONNECT failed with code {}", response[1]),
        ));
    }
    match response[3] {
        1 => discard_exact(upstream, 4 + 2),
        3 => {
            let mut length = [0_u8; 1];
            upstream.read_exact(&mut length)?;
            discard_exact(upstream, length[0] as usize + 2)
        }
        4 => discard_exact(upstream, 16 + 2),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid upstream SOCKS5 reply address type",
        )),
    }
}

fn discard_exact(stream: &mut TcpStream, length: usize) -> io::Result<()> {
    let mut buffer = vec![0_u8; length];
    stream.read_exact(&mut buffer)
}

fn write_socks_reply(client: &mut TcpStream, code: u8) -> io::Result<()> {
    client.write_all(&[5, code, 0, 1, 0, 0, 0, 0, 0, 0])
}

fn connect_with_timeout(host: &str, port: u16) -> io::Result<TcpStream> {
    let addresses = (host.trim_matches(['[', ']']), port).to_socket_addrs()?;
    let mut last_error = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, Duration::from_secs(3)) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "proxy host resolved to no addresses",
        )
    }))
}

fn relay_bidirectionally(mut client: TcpStream, mut upstream: TcpStream) -> io::Result<()> {
    let mut client_reader = client.try_clone()?;
    let mut upstream_writer = upstream.try_clone()?;
    let upload = thread::spawn(move || {
        let result = io::copy(&mut client_reader, &mut upstream_writer);
        let _ = upstream_writer.shutdown(Shutdown::Write);
        result
    });
    let download = io::copy(&mut upstream, &mut client);
    let _ = client.shutdown(Shutdown::Write);
    let upload = upload
        .join()
        .map_err(|_| io::Error::other("proxy relay worker panicked"))?;
    upload?;
    download?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Cursor, Read, Write},
        net::{TcpListener, TcpStream},
        thread,
    };

    use super::{handle_local_socks_connection, read_socks_target, ProxyRelay, RelayConfig};
    use crate::{
        domain::{NetworkProfile, ProxyScheme},
        vault::ProxyAuthentication,
    };

    fn domain_connect_request(hostname: &str, port: u16) -> Vec<u8> {
        assert!(hostname.is_ascii());
        let length = u8::try_from(hostname.len()).expect("test hostname fits SOCKS5");
        let mut request = vec![5, 1, 0, 3, length];
        request.extend_from_slice(hostname.as_bytes());
        request.extend_from_slice(&port.to_be_bytes());
        request
    }

    #[test]
    fn domain_targets_are_parsed_without_socket_chunk_assumptions() {
        let request = domain_connect_request("example.test", 443);
        let mut cursor = Cursor::new(request);
        let target = read_socks_target(&mut cursor).expect("parse SOCKS5 domain target");
        assert_eq!(target.authority, "example.test:443");
        assert_eq!(target.encoded[0], 3);
        assert_eq!(target.encoded[1], 12);
    }

    #[test]
    fn authenticated_socks5_is_relayed_through_a_loopback_no_auth_endpoint() {
        let upstream = TcpListener::bind("127.0.0.1:0").expect("bind fake upstream");
        let upstream_address = upstream.local_addr().expect("fake upstream address");
        let fake_server = thread::spawn(move || {
            let (mut stream, _) = upstream.accept().expect("accept relay");
            let mut greeting = [0_u8; 3];
            stream.read_exact(&mut greeting).expect("read greeting");
            assert_eq!(greeting, [5, 1, 2]);
            stream.write_all(&[5, 2]).expect("select auth");
            let mut auth_header = [0_u8; 2];
            stream
                .read_exact(&mut auth_header)
                .expect("read auth header");
            let mut username = vec![0_u8; auth_header[1] as usize];
            stream.read_exact(&mut username).expect("read username");
            let mut password_length = [0_u8; 1];
            stream
                .read_exact(&mut password_length)
                .expect("read password length");
            let mut password = vec![0_u8; password_length[0] as usize];
            stream.read_exact(&mut password).expect("read password");
            assert_eq!(username, b"alice");
            assert_eq!(password, b"secret");
            stream.write_all(&[1, 0]).expect("accept auth");

            let mut request = [0_u8; 4];
            stream
                .read_exact(&mut request)
                .expect("read connect header");
            assert_eq!(request, [5, 1, 0, 3]);
            let mut hostname_length = [0_u8; 1];
            stream
                .read_exact(&mut hostname_length)
                .expect("read hostname length");
            let mut target = vec![0_u8; hostname_length[0] as usize + 2];
            stream.read_exact(&mut target).expect("read target");
            stream
                .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 1])
                .expect("accept connect");
            let mut payload = [0_u8; 4];
            stream.read_exact(&mut payload).expect("read payload");
            stream.write_all(&payload).expect("echo payload");
        });

        let local = TcpListener::bind("127.0.0.1:0").expect("bind local relay");
        let local_address = local.local_addr().expect("local relay address");
        let relay_worker = thread::spawn(move || {
            let (stream, _) = local.accept().expect("accept local SOCKS client");
            handle_local_socks_connection(
                stream,
                &RelayConfig {
                    scheme: ProxyScheme::Socks5,
                    host: "127.0.0.1".to_owned(),
                    port: upstream_address.port(),
                    authentication: Some(ProxyAuthentication::new(
                        "alice".to_owned(),
                        "secret".to_owned(),
                    )),
                },
            )
        });
        let mut client = TcpStream::connect(local_address).expect("connect local relay");
        client.write_all(&[5, 1, 0]).expect("offer no auth");
        let mut method = [0_u8; 2];
        client.read_exact(&mut method).expect("read method");
        assert_eq!(method, [5, 0]);
        client
            .write_all(&domain_connect_request("example.test", 80))
            .expect("request target");
        let mut reply = [0_u8; 10];
        client.read_exact(&mut reply).expect("read reply");
        if reply[1] != 0 {
            drop(client);
            let error = relay_worker
                .join()
                .expect("relay worker exits")
                .expect_err("failed reply must include a worker error");
            panic!("local SOCKS relay rejected a valid target: {error}");
        }
        client.write_all(b"ping").expect("send payload");
        let mut echoed = [0_u8; 4];
        client.read_exact(&mut echoed).expect("read echo");
        assert_eq!(&echoed, b"ping");
        drop(client);
        relay_worker
            .join()
            .expect("relay worker exits")
            .expect("relay completes");
        fake_server.join().expect("fake server exits");
    }

    #[test]
    fn local_relay_binds_a_random_loopback_port() {
        let upstream = TcpListener::bind("127.0.0.1:0").expect("bind unused upstream");
        let relay = ProxyRelay::start(
            &NetworkProfile::FixedProxy {
                proxy_required: true,
                scheme: ProxyScheme::Socks5,
                host: "127.0.0.1".to_owned(),
                port: upstream.local_addr().expect("upstream address").port(),
                bypass_list: Vec::new(),
                credential_reference: Some(uuid::Uuid::new_v4()),
                external_mihomo: None,
            },
            Some(ProxyAuthentication::new(
                "alice".to_owned(),
                "secret".to_owned(),
            )),
        )
        .expect("start relay");
        assert_eq!(relay.endpoint().host, "127.0.0.1");
        assert_ne!(relay.endpoint().port, 0);
        drop(relay);
    }

    #[test]
    fn socks5_credentials_are_verified_during_preflight() {
        let upstream = TcpListener::bind("127.0.0.1:0").expect("bind fake upstream");
        let upstream_address = upstream.local_addr().expect("fake upstream address");
        let fake_server = thread::spawn(move || {
            let (mut stream, _) = upstream.accept().expect("accept preflight");
            let mut greeting = [0_u8; 3];
            stream.read_exact(&mut greeting).expect("read greeting");
            assert_eq!(greeting, [5, 1, 2]);
            stream.write_all(&[5, 2]).expect("select auth");
            let mut auth_header = [0_u8; 2];
            stream
                .read_exact(&mut auth_header)
                .expect("read auth header");
            let mut username = vec![0_u8; auth_header[1] as usize];
            stream.read_exact(&mut username).expect("read username");
            let mut password_length = [0_u8; 1];
            stream
                .read_exact(&mut password_length)
                .expect("read password length");
            let mut password = vec![0_u8; password_length[0] as usize];
            stream.read_exact(&mut password).expect("read password");
            assert_eq!(username, b"alice");
            assert_eq!(password, b"secret");
            stream.write_all(&[1, 0]).expect("accept auth");
        });

        let profile = NetworkProfile::FixedProxy {
            proxy_required: true,
            scheme: ProxyScheme::Socks5,
            host: "127.0.0.1".to_owned(),
            port: upstream_address.port(),
            bypass_list: Vec::new(),
            credential_reference: Some(uuid::Uuid::new_v4()),
            external_mihomo: None,
        };
        let authentication = ProxyAuthentication::new("alice".to_owned(), "secret".to_owned());
        let result = ProxyRelay::preflight_upstream(&profile, Some(&authentication))
            .expect("preflight succeeds");
        assert!(result.authentication_verified);
        fake_server.join().expect("fake server exits");
    }
}
