use std::{
    collections::{HashMap, VecDeque},
    fmt,
    io::{self, Read, Write},
    net::{
        IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs,
    },
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    domain::{NetworkProfile, ProxyScheme},
    vault::ProxyAuthentication,
};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const HEALTH_IO_POLL: Duration = Duration::from_millis(100);
const MAX_HTTP_RESPONSE_HEADER: usize = 16 * 1024;
const MAX_CONCURRENT_CONNECTIONS: usize = 64;
const MAX_RELAY_RECEIPTS: usize = 128;
const MAX_UPSTREAM_ADDRESSES: usize = 16;
const RELAY_RECEIPT_MAX_AGE: Duration = Duration::from_secs(10 * 60);
const RELAY_SHUTDOWN_GRACE: Duration = Duration::from_millis(750);
const RELAY_SHUTDOWN_POLL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone)]
pub struct RelayEndpoint {
    pub host: String,
    pub port: u16,
}

#[derive(Debug)]
pub struct ProxyRelay {
    endpoint: RelayEndpoint,
    shutdown: Arc<AtomicBool>,
    receipts: Arc<RelayReceiptStore>,
    credentials: Arc<RuntimeCredentials>,
    config: Arc<RelayConfig>,
    connections: Arc<ActiveRelayConnections>,
    active_connections: Arc<AtomicUsize>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelayAuthenticationEvidence {
    Accepted,
    Rejected,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelayUpstreamOutcome {
    HttpConnectAccepted,
    HttpCredentialsAccepted,
    HttpAuthenticationRejected,
    Socks5ConnectAccepted,
    Socks5CredentialsAccepted,
    Socks5AuthenticationRejected,
}

impl RelayUpstreamOutcome {
    fn accepted(self) -> bool {
        matches!(
            self,
            Self::HttpConnectAccepted
                | Self::HttpCredentialsAccepted
                | Self::Socks5ConnectAccepted
                | Self::Socks5CredentialsAccepted
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct RelayBinding {
    relay_id: Uuid,
    silo_id: Uuid,
    runtime_id: Uuid,
}

#[derive(Debug, Clone)]
struct RelayConnectionReceipt {
    relay_id: Uuid,
    silo_id: Uuid,
    runtime_id: Uuid,
    connection_id: u64,
    handshake_at: Instant,
    handshake_at_utc: DateTime<Utc>,
    outcome: RelayUpstreamOutcome,
    bytes_relayed_at: Option<Instant>,
    bytes_relayed_at_utc: Option<DateTime<Utc>>,
}

#[derive(Debug)]
struct RelayReceiptStore {
    binding: RelayBinding,
    receipts: Mutex<VecDeque<RelayConnectionReceipt>>,
}

impl RelayReceiptStore {
    fn new(silo_id: Uuid, runtime_id: Uuid) -> Self {
        Self {
            binding: RelayBinding {
                relay_id: Uuid::new_v4(),
                silo_id,
                runtime_id,
            },
            receipts: Mutex::new(VecDeque::with_capacity(MAX_RELAY_RECEIPTS)),
        }
    }

    fn record_handshake(&self, connection_id: u64, outcome: RelayUpstreamOutcome) {
        self.record_handshake_at(connection_id, outcome, Instant::now(), Utc::now());
    }

    fn record_handshake_at(
        &self,
        connection_id: u64,
        outcome: RelayUpstreamOutcome,
        handshake_at: Instant,
        handshake_at_utc: DateTime<Utc>,
    ) {
        let Ok(mut receipts) = self.receipts.lock() else {
            return;
        };
        prune_stale_receipts(&mut receipts, Instant::now());
        if receipts.len() == MAX_RELAY_RECEIPTS {
            receipts.pop_front();
        }
        receipts.push_back(RelayConnectionReceipt {
            relay_id: self.binding.relay_id,
            silo_id: self.binding.silo_id,
            runtime_id: self.binding.runtime_id,
            connection_id,
            handshake_at,
            handshake_at_utc,
            outcome,
            bytes_relayed_at: None,
            bytes_relayed_at_utc: None,
        });
    }

    fn mark_bytes_relayed(&self, connection_id: u64) {
        let Ok(mut receipts) = self.receipts.lock() else {
            return;
        };
        let Some(receipt) = receipts
            .iter_mut()
            .rev()
            .find(|receipt| receipt.connection_id == connection_id)
        else {
            return;
        };
        if receipt.outcome.accepted() && receipt.bytes_relayed_at.is_none() {
            receipt.bytes_relayed_at = Some(Instant::now());
            receipt.bytes_relayed_at_utc = Some(Utc::now());
        }
    }

    fn authentication_evidence(
        &self,
        silo_id: Uuid,
        runtime_id: Uuid,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> RelayAuthenticationEvidence {
        if self.binding.silo_id != silo_id
            || self.binding.runtime_id != runtime_id
            || window_start > window_end
        {
            return RelayAuthenticationEvidence::None;
        }
        let now = Instant::now();
        let Ok(mut receipts) = self.receipts.lock() else {
            return RelayAuthenticationEvidence::None;
        };
        prune_stale_receipts(&mut receipts, now);
        let in_window =
            |timestamp: DateTime<Utc>| timestamp >= window_start && timestamp <= window_end;
        let matches_binding = |receipt: &&RelayConnectionReceipt| {
            receipt.relay_id == self.binding.relay_id
                && receipt.silo_id == silo_id
                && receipt.runtime_id == runtime_id
                && now
                    .checked_duration_since(receipt.handshake_at)
                    .is_some_and(|age| age <= RELAY_RECEIPT_MAX_AGE)
        };

        let relevant = receipts.iter().filter(matches_binding).collect::<Vec<_>>();
        if relevant.iter().any(|receipt| {
            receipt.outcome == RelayUpstreamOutcome::HttpAuthenticationRejected
                && in_window(receipt.handshake_at_utc)
        }) {
            return RelayAuthenticationEvidence::Rejected;
        }
        if relevant.iter().any(|receipt| {
            receipt.outcome == RelayUpstreamOutcome::HttpCredentialsAccepted
                && in_window(receipt.handshake_at_utc)
                && receipt.bytes_relayed_at.is_some_and(|timestamp| {
                    now.checked_duration_since(timestamp)
                        .is_some_and(|age| age <= RELAY_RECEIPT_MAX_AGE)
                })
                && receipt.bytes_relayed_at_utc.is_some_and(in_window)
        }) {
            RelayAuthenticationEvidence::Accepted
        } else {
            RelayAuthenticationEvidence::None
        }
    }
}

fn prune_stale_receipts(receipts: &mut VecDeque<RelayConnectionReceipt>, now: Instant) {
    receipts.retain(|receipt| {
        now.checked_duration_since(receipt.handshake_at)
            .is_some_and(|age| age <= RELAY_RECEIPT_MAX_AGE)
    });
}

#[derive(Debug)]
struct RelayConfig {
    scheme: ProxyScheme,
    addresses: Vec<SocketAddr>,
    credentials: Arc<RuntimeCredentials>,
}

struct RuntimeCredentials {
    authentication: Mutex<Option<ProxyAuthentication>>,
}

impl RuntimeCredentials {
    fn new(authentication: Option<ProxyAuthentication>) -> Self {
        Self {
            authentication: Mutex::new(authentication),
        }
    }

    fn snapshot(&self) -> io::Result<Option<ProxyAuthentication>> {
        self.authentication
            .lock()
            .map(|authentication| authentication.clone())
            .map_err(|_| io::Error::other("proxy relay credential state is unavailable"))
    }

    fn revoke(&self) {
        if let Ok(mut authentication) = self.authentication.lock() {
            *authentication = None;
        }
    }

    #[cfg(test)]
    fn is_present(&self) -> bool {
        self.authentication
            .lock()
            .is_ok_and(|authentication| authentication.is_some())
    }
}

impl fmt::Debug for RuntimeCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RuntimeCredentials(<redacted>)")
    }
}

#[derive(Debug, Default)]
struct ActiveRelayConnections {
    revoked: AtomicBool,
    sockets: Mutex<HashMap<u64, Vec<TcpStream>>>,
}

impl ActiveRelayConnections {
    fn track(&self, connection_id: u64, stream: &TcpStream) -> io::Result<()> {
        if self.revoked.load(Ordering::Acquire) {
            let _ = stream.shutdown(Shutdown::Both);
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "proxy relay runtime was revoked",
            ));
        }
        let tracked = stream.try_clone()?;
        let mut sockets = self
            .sockets
            .lock()
            .map_err(|_| io::Error::other("proxy relay connection state is unavailable"))?;
        if self.revoked.load(Ordering::Acquire) {
            let _ = tracked.shutdown(Shutdown::Both);
            let _ = stream.shutdown(Shutdown::Both);
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "proxy relay runtime was revoked",
            ));
        }
        sockets.entry(connection_id).or_default().push(tracked);
        Ok(())
    }

    fn remove(&self, connection_id: u64) {
        if let Ok(mut sockets) = self.sockets.lock() {
            sockets.remove(&connection_id);
        }
    }

    fn revoke(&self) {
        self.revoked.store(true, Ordering::Release);
        let sockets = self
            .sockets
            .lock()
            .map(|mut sockets| sockets.drain().flat_map(|(_, sockets)| sockets).collect())
            .unwrap_or_else(|_| Vec::new());
        for socket in sockets {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
}

struct ActiveConnectionGuard {
    active: Arc<AtomicUsize>,
    connection: Option<(Arc<ActiveRelayConnections>, u64)>,
}

impl ActiveConnectionGuard {
    fn track(
        mut self,
        connections: Arc<ActiveRelayConnections>,
        connection_id: u64,
        stream: &TcpStream,
    ) -> io::Result<Self> {
        connections.track(connection_id, stream)?;
        self.connection = Some((connections, connection_id));
        Ok(self)
    }
}

impl Drop for ActiveConnectionGuard {
    fn drop(&mut self) {
        if let Some((connections, connection_id)) = self.connection.as_ref() {
            connections.remove(*connection_id);
        }
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Debug)]
struct SocksTarget {
    encoded: Vec<u8>,
    authority: String,
}

impl ProxyRelay {
    pub fn start(
        profile: &NetworkProfile,
        silo_id: Uuid,
        runtime_id: Uuid,
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
                authentication.username().len() > u8::MAX as usize
                    || authentication.password().len() > u8::MAX as usize
            })
        {
            return Err(ProxyRelayError::SocksCredentialTooLong);
        }

        // Pin the addresses for this exact runtime before retaining its
        // credentials. Health checks and relayed traffic then use the same
        // finite endpoint set and never perform DNS while holding credentials.
        let addresses = resolve_upstream_addresses(host, *port)?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let endpoint = RelayEndpoint {
            host: Ipv4Addr::LOCALHOST.to_string(),
            port: address.port(),
        };
        let credentials = Arc::new(RuntimeCredentials::new(authentication));
        let config = Arc::new(RelayConfig {
            scheme: scheme.clone(),
            addresses,
            credentials: Arc::clone(&credentials),
        });
        let shutdown = Arc::new(AtomicBool::new(false));
        let active_connections = Arc::new(AtomicUsize::new(0));
        let connections = Arc::new(ActiveRelayConnections::default());
        let next_connection_id = Arc::new(AtomicU64::new(1));
        let receipts = Arc::new(RelayReceiptStore::new(silo_id, runtime_id));
        let thread_shutdown = Arc::clone(&shutdown);
        let thread_receipts = Arc::clone(&receipts);
        let thread_connections = Arc::clone(&connections);
        let thread_config = Arc::clone(&config);
        let thread_active_connections = Arc::clone(&active_connections);
        let accept_thread = thread::spawn(move || {
            while !thread_shutdown.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        // Winsock accepts a socket with the listener's nonblocking
                        // mode. The worker below performs a bounded blocking
                        // handshake, so normalize the accepted stream explicitly
                        // on every platform before handing it off.
                        if stream.set_nonblocking(false).is_err() {
                            let _ = stream.shutdown(Shutdown::Both);
                            continue;
                        }
                        if thread_shutdown.load(Ordering::Acquire) {
                            let _ = stream.shutdown(Shutdown::Both);
                            break;
                        }
                        let connection_id = next_connection_id.fetch_add(1, Ordering::Relaxed);
                        let Some(connection_guard) =
                            try_acquire_connection(&thread_active_connections)
                        else {
                            let _ = stream.shutdown(Shutdown::Both);
                            continue;
                        };
                        let connection_guard = match connection_guard.track(
                            Arc::clone(&thread_connections),
                            connection_id,
                            &stream,
                        ) {
                            Ok(guard) => guard,
                            Err(_) => continue,
                        };
                        let connection_config = Arc::clone(&thread_config);
                        let connection_receipts = Arc::clone(&thread_receipts);
                        let connection_connections = Arc::clone(&thread_connections);
                        let _ = thread::Builder::new()
                            .name("verisilo-proxy-relay".to_owned())
                            .spawn(move || {
                                let _connection_guard = connection_guard;
                                let _ = handle_local_socks_connection(
                                    stream,
                                    &connection_config,
                                    connection_id,
                                    connection_receipts,
                                    connection_connections,
                                );
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
            receipts,
            credentials,
            config,
            connections,
            active_connections,
            accept_thread: Some(accept_thread),
        })
    }

    pub fn endpoint(&self) -> &RelayEndpoint {
        &self.endpoint
    }

    pub fn is_healthy(&self) -> bool {
        let cancelled = AtomicBool::new(false);
        self.is_healthy_until(Instant::now() + Duration::from_millis(500), &cancelled)
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

    pub(crate) fn authentication_evidence(
        &self,
        silo_id: Uuid,
        runtime_id: Uuid,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> RelayAuthenticationEvidence {
        self.receipts
            .authentication_evidence(silo_id, runtime_id, window_start, window_end)
    }

    pub(crate) fn matches_runtime(&self, silo_id: Uuid, runtime_id: Uuid) -> bool {
        self.receipts.binding.silo_id == silo_id && self.receipts.binding.runtime_id == runtime_id
    }

    pub(crate) fn is_healthy_until(&self, deadline: Instant, cancelled: &AtomicBool) -> bool {
        if self.shutdown.load(Ordering::Acquire)
            || cancelled.load(Ordering::Acquire)
            || self
                .accept_thread
                .as_ref()
                .is_some_and(JoinHandle::is_finished)
        {
            return false;
        }
        let Ok(timeout) = remaining_io_timeout(deadline, Duration::from_millis(500), cancelled)
        else {
            return false;
        };
        TcpStream::connect_timeout(
            &SocketAddr::from((Ipv4Addr::LOCALHOST, self.endpoint.port)),
            timeout,
        )
        .is_ok()
    }

    pub(crate) fn verify_upstream_until(
        &self,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> Result<(), ProxyRelayError> {
        if self.shutdown.load(Ordering::Acquire) || cancelled.load(Ordering::Acquire) {
            return Err(revoked_relay_error());
        }
        let mut upstream = connect_addresses_until(
            &self.config.addresses,
            deadline,
            cancelled,
            Some(&self.shutdown),
        )?;
        if self.shutdown.load(Ordering::Acquire) || cancelled.load(Ordering::Acquire) {
            let _ = upstream.shutdown(Shutdown::Both);
            return Err(revoked_relay_error());
        }
        // DNS resolution and TCP connect completed before this snapshot. From
        // here every credential-bearing operation shares the caller's total
        // deadline and observes runtime revocation between bounded I/O polls.
        match self.config.scheme {
            ProxyScheme::Socks5 => {
                let authentication = self.credentials.snapshot()?;
                negotiate_socks5_authentication_until(
                    &mut upstream,
                    authentication.as_ref(),
                    deadline,
                    cancelled,
                    Some(&self.shutdown),
                )?;
                Ok(())
            }
            ProxyScheme::Http => Ok(()),
            ProxyScheme::Https | ProxyScheme::Socks4 => Err(ProxyRelayError::UnsupportedProxy),
        }
    }

    /// Revokes only the relay bound to the supplied Silo/runtime tuple. A
    /// mismatch is deliberately a no-op so one runtime can never close another
    /// relay through a stale or forged identifier.
    pub(crate) fn shutdown_for_runtime(&mut self, silo_id: Uuid, runtime_id: Uuid) -> bool {
        if !self.matches_runtime(silo_id, runtime_id) {
            return false;
        }
        self.shutdown_inner();
        true
    }

    fn shutdown_inner(&mut self) {
        if self.shutdown.swap(true, Ordering::AcqRel) {
            return;
        }
        self.credentials.revoke();
        self.connections.revoke();
        let _ = TcpStream::connect_timeout(
            &SocketAddr::from((Ipv4Addr::LOCALHOST, self.endpoint.port)),
            Duration::from_millis(100),
        );
        if let Some(thread) = self.accept_thread.take() {
            let _ = thread.join();
        }
        // Close anything registered by an accept/worker racing the first
        // revoke, then give detached workers a fixed interval to drop clones.
        self.connections.revoke();
        let deadline = Instant::now() + RELAY_SHUTDOWN_GRACE;
        while self.active_connections.load(Ordering::Acquire) != 0 && Instant::now() < deadline {
            thread::sleep(RELAY_SHUTDOWN_POLL);
        }
        self.connections.revoke();
    }

    #[cfg(test)]
    pub(crate) fn inject_authentication_receipt_for_test(
        &self,
        evidence: RelayAuthenticationEvidence,
        observed_at: DateTime<Utc>,
        bytes_relayed: bool,
    ) {
        let outcome = match evidence {
            RelayAuthenticationEvidence::Accepted => RelayUpstreamOutcome::HttpCredentialsAccepted,
            RelayAuthenticationEvidence::Rejected => {
                RelayUpstreamOutcome::HttpAuthenticationRejected
            }
            RelayAuthenticationEvidence::None => return,
        };
        let connection_id = u64::MAX;
        self.receipts
            .record_handshake_at(connection_id, outcome, Instant::now(), observed_at);
        if bytes_relayed {
            self.receipts.mark_bytes_relayed(connection_id);
            if let Ok(mut receipts) = self.receipts.receipts.lock() {
                if let Some(receipt) = receipts
                    .iter_mut()
                    .rev()
                    .find(|receipt| receipt.connection_id == connection_id)
                {
                    receipt.bytes_relayed_at_utc = Some(observed_at);
                }
            }
        }
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

fn revoked_relay_error() -> ProxyRelayError {
    ProxyRelayError::Io(io::Error::new(
        io::ErrorKind::ConnectionAborted,
        "proxy relay runtime was revoked",
    ))
}

fn try_acquire_connection(active: &Arc<AtomicUsize>) -> Option<ActiveConnectionGuard> {
    active
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            (current < MAX_CONCURRENT_CONNECTIONS).then_some(current + 1)
        })
        .ok()
        .map(|_| ActiveConnectionGuard {
            active: Arc::clone(active),
            connection: None,
        })
}

impl Drop for ProxyRelay {
    fn drop(&mut self) {
        self.shutdown_inner();
    }
}

fn handle_local_socks_connection(
    mut client: TcpStream,
    config: &RelayConfig,
    connection_id: u64,
    receipts: Arc<RelayReceiptStore>,
    connections: Arc<ActiveRelayConnections>,
) -> io::Result<()> {
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
    let upstream = match connect_upstream(config, &target, connection_id, &receipts, &connections) {
        Ok(upstream) => upstream,
        Err(error) => {
            let _ = write_socks_reply(&mut client, 1);
            return Err(error);
        }
    };
    write_socks_reply(&mut client, 0)?;
    client.set_read_timeout(None)?;
    client.set_write_timeout(None)?;
    relay_bidirectionally(client, upstream, connection_id, receipts)
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

fn connect_upstream(
    config: &RelayConfig,
    target: &SocksTarget,
    connection_id: u64,
    receipts: &RelayReceiptStore,
    connections: &ActiveRelayConnections,
) -> io::Result<TcpStream> {
    match config.scheme {
        ProxyScheme::Http => {
            connect_http_upstream(config, target, connection_id, receipts, connections)
        }
        ProxyScheme::Socks5 => {
            let mut upstream = connect_configured_upstream(config, connection_id, connections)?;
            authenticate_socks5_proxy(&mut upstream, config, target, connection_id, receipts)?;
            upstream.set_read_timeout(None)?;
            upstream.set_write_timeout(None)?;
            Ok(upstream)
        }
        ProxyScheme::Https | ProxyScheme::Socks4 => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unsupported authenticated upstream proxy",
        )),
    }
}

fn connect_configured_upstream(
    config: &RelayConfig,
    connection_id: u64,
    connections: &ActiveRelayConnections,
) -> io::Result<TcpStream> {
    let upstream = connect_addresses_with_timeout(&config.addresses)?;
    connections.track(connection_id, &upstream)?;
    upstream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    upstream.set_write_timeout(Some(HANDSHAKE_TIMEOUT))?;
    Ok(upstream)
}

#[derive(Debug, Clone, Copy)]
struct HttpProxyResponse {
    status: u16,
    offers_basic_authentication: bool,
}

fn connect_http_upstream(
    config: &RelayConfig,
    target: &SocksTarget,
    connection_id: u64,
    receipts: &RelayReceiptStore,
    connections: &ActiveRelayConnections,
) -> io::Result<TcpStream> {
    // A 2xx response to a request without credentials proves only that CONNECT
    // was accepted. It must not be relabeled as credential authentication.
    let mut upstream = connect_configured_upstream(config, connection_id, connections)?;
    let initial = send_http_connect(&mut upstream, target, None)?;
    if (200..300).contains(&initial.status) {
        receipts.record_handshake(connection_id, RelayUpstreamOutcome::HttpConnectAccepted);
        upstream.set_read_timeout(None)?;
        upstream.set_write_timeout(None)?;
        return Ok(upstream);
    }
    if initial.status != 407 {
        return Err(http_connect_error(initial.status));
    }

    let Some(authentication) = config.credentials.snapshot()? else {
        receipts.record_handshake(
            connection_id,
            RelayUpstreamOutcome::HttpAuthenticationRejected,
        );
        return Err(http_authentication_error());
    };
    if !initial.offers_basic_authentication {
        receipts.record_handshake(
            connection_id,
            RelayUpstreamOutcome::HttpAuthenticationRejected,
        );
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "upstream HTTP proxy requires an unsupported authentication method",
        ));
    }

    // Reconnect after the challenge instead of assuming a 407 response keeps
    // the transport reusable. Only challenge -> credentialed 2xx is recorded
    // as accepted Basic authentication.
    drop(upstream);
    let mut authenticated = connect_configured_upstream(config, connection_id, connections)?;
    let response = send_http_connect(&mut authenticated, target, Some(&authentication))?;
    if response.status == 407 {
        receipts.record_handshake(
            connection_id,
            RelayUpstreamOutcome::HttpAuthenticationRejected,
        );
        return Err(http_authentication_error());
    }
    if !(200..300).contains(&response.status) {
        return Err(http_connect_error(response.status));
    }
    receipts.record_handshake(connection_id, RelayUpstreamOutcome::HttpCredentialsAccepted);
    authenticated.set_read_timeout(None)?;
    authenticated.set_write_timeout(None)?;
    Ok(authenticated)
}

fn send_http_connect(
    upstream: &mut TcpStream,
    target: &SocksTarget,
    authentication: Option<&ProxyAuthentication>,
) -> io::Result<HttpProxyResponse> {
    let authorization = if let Some(authentication) = authentication {
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

    let mut header = Vec::with_capacity(1024);
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
    let header_text = std::str::from_utf8(&header)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid HTTP proxy header"))?;
    let status_line = header_text
        .lines()
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid HTTP status line"))?;
    let status = status_line
        .split_ascii_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid HTTP proxy status"))?;
    let offers_basic_authentication = header_text.lines().skip(1).any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("proxy-authenticate")
                && value
                    .trim_start()
                    .split_ascii_whitespace()
                    .next()
                    .is_some_and(|scheme| scheme.eq_ignore_ascii_case("basic"))
        })
    });
    Ok(HttpProxyResponse {
        status,
        offers_basic_authentication,
    })
}

fn http_authentication_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "upstream HTTP proxy rejected authentication",
    )
}

fn http_connect_error(status: u16) -> io::Error {
    io::Error::new(
        io::ErrorKind::ConnectionRefused,
        format!("upstream HTTP proxy rejected CONNECT with status {status}"),
    )
}

fn authenticate_socks5_proxy(
    upstream: &mut TcpStream,
    config: &RelayConfig,
    target: &SocksTarget,
    connection_id: u64,
    receipts: &RelayReceiptStore,
) -> io::Result<()> {
    let authentication = config.credentials.snapshot()?;
    if let Err(error) = negotiate_socks5_authentication(upstream, authentication.as_ref()) {
        if authentication.is_some() {
            receipts.record_handshake(
                connection_id,
                RelayUpstreamOutcome::Socks5AuthenticationRejected,
            );
        }
        return Err(error);
    }

    let mut request = vec![5, 1, 0];
    request.extend_from_slice(&target.encoded);
    upstream.write_all(&request)?;
    consume_socks5_reply(upstream)?;
    receipts.record_handshake(
        connection_id,
        if authentication.is_some() {
            RelayUpstreamOutcome::Socks5CredentialsAccepted
        } else {
            RelayUpstreamOutcome::Socks5ConnectAccepted
        },
    );
    Ok(())
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
    let mut request = Zeroizing::new(Vec::with_capacity(username.len() + password.len() + 3));
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

fn negotiate_socks5_authentication_until(
    upstream: &mut TcpStream,
    authentication: Option<&ProxyAuthentication>,
    deadline: Instant,
    cancelled: &AtomicBool,
    revoked: Option<&AtomicBool>,
) -> io::Result<()> {
    let method = if authentication.is_some() { 2 } else { 0 };
    write_all_until(upstream, &[5, 1, method], deadline, cancelled, revoked)?;
    let mut response = [0_u8; 2];
    read_exact_until(upstream, &mut response, deadline, cancelled, revoked)?;
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
    let mut request = Zeroizing::new(Vec::with_capacity(username.len() + password.len() + 3));
    request.extend_from_slice(&[1, username.len() as u8]);
    request.extend_from_slice(username);
    request.push(password.len() as u8);
    request.extend_from_slice(password);
    write_all_until(upstream, &request, deadline, cancelled, revoked)?;
    let mut authentication_response = [0_u8; 2];
    read_exact_until(
        upstream,
        &mut authentication_response,
        deadline,
        cancelled,
        revoked,
    )?;
    if authentication_response != [1, 0] {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "upstream SOCKS5 proxy rejected credentials",
        ));
    }
    Ok(())
}

fn write_all_until(
    stream: &mut TcpStream,
    mut bytes: &[u8],
    deadline: Instant,
    cancelled: &AtomicBool,
    revoked: Option<&AtomicBool>,
) -> io::Result<()> {
    while !bytes.is_empty() {
        let timeout = remaining_relay_timeout(deadline, HEALTH_IO_POLL, cancelled, revoked)?;
        stream.set_write_timeout(Some(timeout))?;
        match stream.write(bytes) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "proxy health handshake closed while writing",
                ))
            }
            Ok(written) => bytes = &bytes[written..],
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn read_exact_until(
    stream: &mut TcpStream,
    mut bytes: &mut [u8],
    deadline: Instant,
    cancelled: &AtomicBool,
    revoked: Option<&AtomicBool>,
) -> io::Result<()> {
    while !bytes.is_empty() {
        let timeout = remaining_relay_timeout(deadline, HEALTH_IO_POLL, cancelled, revoked)?;
        stream.set_read_timeout(Some(timeout))?;
        match stream.read(bytes) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "proxy health handshake closed while reading",
                ))
            }
            Ok(read) => {
                let remaining = bytes;
                bytes = &mut remaining[read..];
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
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
    let addresses = resolve_upstream_addresses(host, port)?;
    connect_addresses_with_timeout(&addresses)
}

fn resolve_upstream_addresses(host: &str, port: u16) -> io::Result<Vec<SocketAddr>> {
    let addresses = (host.trim_matches(['[', ']']), port)
        .to_socket_addrs()?
        .take(MAX_UPSTREAM_ADDRESSES)
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "proxy host resolved to no addresses",
        ));
    }
    Ok(addresses)
}

fn connect_addresses_with_timeout(addresses: &[SocketAddr]) -> io::Result<TcpStream> {
    let mut last_error = None;
    for address in addresses {
        match TcpStream::connect_timeout(address, CONNECT_TIMEOUT) {
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

fn connect_addresses_until(
    addresses: &[SocketAddr],
    deadline: Instant,
    cancelled: &AtomicBool,
    revoked: Option<&AtomicBool>,
) -> io::Result<TcpStream> {
    let mut last_error = None;
    for address in addresses {
        let timeout = remaining_relay_timeout(deadline, CONNECT_TIMEOUT, cancelled, revoked)?;
        match TcpStream::connect_timeout(address, timeout) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "proxy runtime has no pinned upstream addresses",
        )
    }))
}

fn remaining_io_timeout(
    deadline: Instant,
    maximum: Duration,
    cancelled: &AtomicBool,
) -> io::Result<Duration> {
    if cancelled.load(Ordering::Acquire) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "proxy health check was cancelled",
        ));
    }
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "proxy health check exceeded its total deadline",
        ));
    }
    Ok(remaining.min(maximum))
}

fn remaining_relay_timeout(
    deadline: Instant,
    maximum: Duration,
    cancelled: &AtomicBool,
    revoked: Option<&AtomicBool>,
) -> io::Result<Duration> {
    if revoked.is_some_and(|revoked| revoked.load(Ordering::Acquire)) {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            "proxy relay runtime was revoked",
        ));
    }
    remaining_io_timeout(deadline, maximum, cancelled)
}

fn relay_bidirectionally(
    mut client: TcpStream,
    mut upstream: TcpStream,
    connection_id: u64,
    receipts: Arc<RelayReceiptStore>,
) -> io::Result<()> {
    let mut client_reader = client.try_clone()?;
    let mut upstream_writer = upstream.try_clone()?;
    let upload_receipts = Arc::clone(&receipts);
    let upload = thread::spawn(move || {
        let result = relay_bytes(
            &mut client_reader,
            &mut upstream_writer,
            connection_id,
            &upload_receipts,
        );
        let _ = upstream_writer.shutdown(Shutdown::Write);
        result
    });
    let download = relay_bytes(&mut upstream, &mut client, connection_id, &receipts);
    let _ = client.shutdown(Shutdown::Write);
    let upload = upload
        .join()
        .map_err(|_| io::Error::other("proxy relay worker panicked"))?;
    upload?;
    download?;
    Ok(())
}

fn relay_bytes(
    reader: &mut TcpStream,
    writer: &mut TcpStream,
    connection_id: u64,
    receipts: &RelayReceiptStore,
) -> io::Result<u64> {
    let mut total = 0_u64;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let length = reader.read(&mut buffer)?;
        if length == 0 {
            return Ok(total);
        }
        writer.write_all(&buffer[..length])?;
        total = total.saturating_add(length as u64);
        receipts.mark_bytes_relayed(connection_id);
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration as ChronoDuration, Utc};
    use std::{
        io::{Cursor, Read, Write},
        net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
        sync::{
            atomic::{AtomicBool, AtomicUsize},
            mpsc, Arc,
        },
        thread,
        time::{Duration, Instant},
    };
    use uuid::Uuid;

    use super::{
        handle_local_socks_connection, read_socks_target, try_acquire_connection,
        ActiveRelayConnections, ProxyRelay, RelayAuthenticationEvidence, RelayConfig,
        RelayReceiptStore, RuntimeCredentials, MAX_CONCURRENT_CONNECTIONS,
    };
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

    fn read_http_header(stream: &mut TcpStream) -> String {
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            let mut byte = [0_u8; 1];
            stream.read_exact(&mut byte).expect("read HTTP header");
            header.push(byte[0]);
            assert!(header.len() < super::MAX_HTTP_RESPONSE_HEADER);
        }
        String::from_utf8(header).expect("HTTP header is UTF-8")
    }

    fn http_profile(port: u16) -> NetworkProfile {
        NetworkProfile::FixedProxy {
            proxy_required: true,
            scheme: ProxyScheme::Http,
            host: "127.0.0.1".to_owned(),
            port,
            bypass_list: Vec::new(),
            credential_reference: Some(Uuid::new_v4()),
            external_mihomo: None,
        }
    }

    fn socks_profile(port: u16) -> NetworkProfile {
        NetworkProfile::FixedProxy {
            proxy_required: true,
            scheme: ProxyScheme::Socks5,
            host: "127.0.0.1".to_owned(),
            port,
            bypass_list: Vec::new(),
            credential_reference: None,
            external_mihomo: None,
        }
    }

    fn connect_to_relay(relay: &ProxyRelay) -> (TcpStream, u8) {
        let mut client =
            TcpStream::connect((relay.endpoint().host.as_str(), relay.endpoint().port))
                .expect("connect local relay");
        client.write_all(&[5, 1, 0]).expect("offer no auth");
        let mut method = [0_u8; 2];
        client.read_exact(&mut method).expect("read local method");
        assert_eq!(method, [5, 0]);
        client
            .write_all(&domain_connect_request("example.test", 443))
            .expect("request target");
        let mut reply = [0_u8; 10];
        client.read_exact(&mut reply).expect("read local reply");
        (client, reply[1])
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
    fn upstream_health_deadline_caps_a_slow_socks_handshake() {
        let upstream = TcpListener::bind("127.0.0.1:0").expect("bind slow SOCKS upstream");
        let upstream_address = upstream.local_addr().expect("slow upstream address");
        let (greeting_seen_tx, greeting_seen) = mpsc::channel();
        let (release_tx, release) = mpsc::channel();
        let worker = thread::spawn(move || {
            let (mut stream, _) = upstream.accept().expect("accept health probe");
            let mut greeting = [0_u8; 3];
            stream
                .read_exact(&mut greeting)
                .expect("read health greeting");
            greeting_seen_tx.send(()).expect("report health greeting");
            stream.write_all(&[5]).expect("write partial method reply");
            let _ = release.recv_timeout(Duration::from_secs(2));
            let _ = stream.write_all(&[0]);
        });
        let relay = ProxyRelay::start(
            &socks_profile(upstream_address.port()),
            Uuid::new_v4(),
            Uuid::new_v4(),
            None,
        )
        .expect("start relay with pinned upstream");
        let cancelled = AtomicBool::new(false);
        let started_at = Instant::now();

        let result =
            relay.verify_upstream_until(started_at + Duration::from_millis(150), &cancelled);

        greeting_seen
            .recv_timeout(Duration::from_secs(1))
            .expect("health check reached SOCKS handshake");
        assert!(matches!(
            result,
            Err(super::ProxyRelayError::Io(ref error))
                if error.kind() == std::io::ErrorKind::TimedOut
        ));
        assert!(
            started_at.elapsed() < Duration::from_secs(1),
            "one total deadline must cap every partial SOCKS read"
        );
        let _ = release_tx.send(());
        worker.join().expect("slow SOCKS worker exits");
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
            let receipts = Arc::new(RelayReceiptStore::new(Uuid::new_v4(), Uuid::new_v4()));
            let connections = Arc::new(ActiveRelayConnections::default());
            handle_local_socks_connection(
                stream,
                &RelayConfig {
                    scheme: ProxyScheme::Socks5,
                    addresses: vec![upstream_address],
                    credentials: Arc::new(RuntimeCredentials::new(Some(ProxyAuthentication::new(
                        "alice".to_owned(),
                        "secret".to_owned(),
                    )))),
                },
                1,
                receipts,
                connections,
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
    fn http_basic_challenge_acceptance_and_relayed_bytes_create_bound_evidence() {
        let upstream = TcpListener::bind("127.0.0.1:0").expect("bind fake HTTP proxy");
        let upstream_address = upstream.local_addr().expect("fake proxy address");
        let fake_server = thread::spawn(move || {
            let (mut challenge, _) = upstream.accept().expect("accept unauthenticated CONNECT");
            let initial = read_http_header(&mut challenge);
            assert!(!initial.to_ascii_lowercase().contains("proxy-authorization"));
            challenge
                .write_all(
                    b"HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"test\"\r\nContent-Length: 0\r\n\r\n",
                )
                .expect("write Basic challenge");
            drop(challenge);

            let (mut authenticated, _) = upstream.accept().expect("accept credentialed CONNECT");
            let request = read_http_header(&mut authenticated);
            assert!(request.to_ascii_lowercase().contains(
                "proxy-authorization: basic ywxpY2u6c2vjcmv0"
                    .to_ascii_lowercase()
                    .as_str()
            ));
            authenticated
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .expect("accept authenticated CONNECT");
            let mut payload = [0_u8; 4];
            authenticated
                .read_exact(&mut payload)
                .expect("read payload");
            authenticated.write_all(&payload).expect("echo payload");
        });

        let silo_id = Uuid::new_v4();
        let runtime_id = Uuid::new_v4();
        let relay = ProxyRelay::start(
            &http_profile(upstream_address.port()),
            silo_id,
            runtime_id,
            Some(ProxyAuthentication::new(
                "alice".to_owned(),
                "secret".to_owned(),
            )),
        )
        .expect("start HTTP relay");
        let (mut client, reply) = connect_to_relay(&relay);
        assert_eq!(reply, 0);
        client.write_all(b"ping").expect("send tunneled payload");
        let mut echoed = [0_u8; 4];
        client
            .read_exact(&mut echoed)
            .expect("read tunneled payload");
        assert_eq!(&echoed, b"ping");

        let now = Utc::now();
        assert_eq!(
            relay.authentication_evidence(
                silo_id,
                runtime_id,
                now - ChronoDuration::seconds(5),
                now + ChronoDuration::seconds(1),
            ),
            RelayAuthenticationEvidence::Accepted
        );
        drop(client);
        drop(relay);
        fake_server.join().expect("fake server exits");
    }

    #[test]
    fn http_407_after_credentials_creates_rejected_evidence() {
        let upstream = TcpListener::bind("127.0.0.1:0").expect("bind fake HTTP proxy");
        let upstream_address = upstream.local_addr().expect("fake proxy address");
        let fake_server = thread::spawn(move || {
            let (mut challenge, _) = upstream.accept().expect("accept initial CONNECT");
            let _ = read_http_header(&mut challenge);
            challenge
                .write_all(
                    b"HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"test\"\r\nContent-Length: 0\r\n\r\n",
                )
                .expect("write challenge");
            drop(challenge);
            let (mut rejected, _) = upstream.accept().expect("accept credentialed CONNECT");
            let request = read_http_header(&mut rejected);
            assert!(request.to_ascii_lowercase().contains("proxy-authorization"));
            rejected
                .write_all(
                    b"HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"test\"\r\nContent-Length: 0\r\n\r\n",
                )
                .expect("reject credentials");
        });

        let silo_id = Uuid::new_v4();
        let runtime_id = Uuid::new_v4();
        let relay = ProxyRelay::start(
            &http_profile(upstream_address.port()),
            silo_id,
            runtime_id,
            Some(ProxyAuthentication::new(
                "alice".to_owned(),
                "wrong".to_owned(),
            )),
        )
        .expect("start HTTP relay");
        let (client, reply) = connect_to_relay(&relay);
        assert_ne!(reply, 0);
        let now = Utc::now();
        assert_eq!(
            relay.authentication_evidence(
                silo_id,
                runtime_id,
                now - ChronoDuration::seconds(5),
                now + ChronoDuration::seconds(1),
            ),
            RelayAuthenticationEvidence::Rejected
        );
        drop(client);
        drop(relay);
        fake_server.join().expect("fake server exits");
    }

    #[test]
    fn unauthenticated_http_proxy_never_proves_configured_credentials() {
        let upstream = TcpListener::bind("127.0.0.1:0").expect("bind fake HTTP proxy");
        let upstream_address = upstream.local_addr().expect("fake proxy address");
        let fake_server = thread::spawn(move || {
            let (mut stream, _) = upstream.accept().expect("accept CONNECT");
            let request = read_http_header(&mut stream);
            assert!(!request.to_ascii_lowercase().contains("proxy-authorization"));
            stream
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .expect("accept CONNECT without auth");
            let mut payload = [0_u8; 4];
            stream.read_exact(&mut payload).expect("read payload");
            stream.write_all(&payload).expect("echo payload");
        });

        let silo_id = Uuid::new_v4();
        let runtime_id = Uuid::new_v4();
        let relay = ProxyRelay::start(
            &http_profile(upstream_address.port()),
            silo_id,
            runtime_id,
            Some(ProxyAuthentication::new(
                "alice".to_owned(),
                "secret".to_owned(),
            )),
        )
        .expect("start HTTP relay");
        let (mut client, reply) = connect_to_relay(&relay);
        assert_eq!(reply, 0);
        client.write_all(b"ping").expect("send payload");
        let mut echoed = [0_u8; 4];
        client.read_exact(&mut echoed).expect("read payload");
        let now = Utc::now();
        assert_eq!(
            relay.authentication_evidence(
                silo_id,
                runtime_id,
                now - ChronoDuration::seconds(5),
                now + ChronoDuration::seconds(1),
            ),
            RelayAuthenticationEvidence::None
        );
        drop(client);
        drop(relay);
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
            Uuid::new_v4(),
            Uuid::new_v4(),
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
    fn exact_runtime_shutdown_refuses_new_connections_and_bounds_existing_connections() {
        let upstream = TcpListener::bind("127.0.0.1:0").expect("bind fake SOCKS upstream");
        let upstream_address = upstream.local_addr().expect("fake upstream address");
        let server = thread::spawn(move || {
            let (mut stream, _) = upstream.accept().expect("accept relay connection");
            let mut greeting = [0_u8; 3];
            stream
                .read_exact(&mut greeting)
                .expect("read SOCKS greeting");
            assert_eq!(greeting, [5, 1, 0]);
            stream.write_all(&[5, 0]).expect("accept no-auth");
            let mut request = [0_u8; 4];
            stream
                .read_exact(&mut request)
                .expect("read CONNECT header");
            assert_eq!(request, [5, 1, 0, 3]);
            let mut hostname_length = [0_u8; 1];
            stream
                .read_exact(&mut hostname_length)
                .expect("read hostname length");
            let mut target = vec![0_u8; hostname_length[0] as usize + 2];
            stream.read_exact(&mut target).expect("read target");
            stream
                .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 1])
                .expect("accept CONNECT");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("bound upstream wait");
            let mut byte = [0_u8; 1];
            assert!(matches!(stream.read(&mut byte), Ok(0) | Err(_)));
        });

        let silo_id = Uuid::new_v4();
        let runtime_id = Uuid::new_v4();
        let mut relay = ProxyRelay::start(
            &socks_profile(upstream_address.port()),
            silo_id,
            runtime_id,
            None,
        )
        .expect("start relay");
        let old_endpoint = SocketAddr::from((Ipv4Addr::LOCALHOST, relay.endpoint().port));
        let (mut client, reply) = connect_to_relay(&relay);
        assert_eq!(reply, 0);

        let started = Instant::now();
        assert!(relay.shutdown_for_runtime(silo_id, runtime_id));
        assert!(started.elapsed() < Duration::from_secs(2));
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .expect("bound client read");
        let mut byte = [0_u8; 1];
        assert!(matches!(client.read(&mut byte), Ok(0) | Err(_)));
        assert!(TcpStream::connect_timeout(&old_endpoint, Duration::from_millis(200)).is_err());
        server.join().expect("fake upstream exits");
    }

    #[test]
    fn wrong_runtime_shutdown_cannot_affect_another_relay() {
        let upstream_a = TcpListener::bind("127.0.0.1:0").expect("bind upstream A");
        let upstream_b = TcpListener::bind("127.0.0.1:0").expect("bind upstream B");
        let silo_a = Uuid::new_v4();
        let runtime_a = Uuid::new_v4();
        let silo_b = Uuid::new_v4();
        let runtime_b = Uuid::new_v4();
        let mut relay_a = ProxyRelay::start(
            &http_profile(upstream_a.local_addr().expect("upstream A address").port()),
            silo_a,
            runtime_a,
            Some(ProxyAuthentication::new(
                "alice".to_owned(),
                "secret-a".to_owned(),
            )),
        )
        .expect("start relay A");
        let relay_b = ProxyRelay::start(
            &http_profile(upstream_b.local_addr().expect("upstream B address").port()),
            silo_b,
            runtime_b,
            Some(ProxyAuthentication::new(
                "bob".to_owned(),
                "secret-b".to_owned(),
            )),
        )
        .expect("start relay B");
        let endpoint_a = SocketAddr::from((Ipv4Addr::LOCALHOST, relay_a.endpoint().port));

        assert!(!relay_a.shutdown_for_runtime(silo_b, runtime_b));
        assert!(relay_a.is_healthy());
        assert!(relay_b.is_healthy());
        assert!(relay_a.credentials.is_present());
        assert!(relay_a.shutdown_for_runtime(silo_a, runtime_a));
        assert!(!relay_a.credentials.is_present());
        assert!(TcpStream::connect_timeout(&endpoint_a, Duration::from_millis(200)).is_err());
        assert!(
            relay_b.is_healthy(),
            "relay B must remain independently usable"
        );
    }

    #[test]
    fn local_relay_bounds_concurrent_clients() {
        let active = Arc::new(AtomicUsize::new(0));
        let guards = (0..MAX_CONCURRENT_CONNECTIONS)
            .map(|_| try_acquire_connection(&active).expect("connection slot"))
            .collect::<Vec<_>>();
        assert!(try_acquire_connection(&active).is_none());
        drop(guards);
        assert!(try_acquire_connection(&active).is_some());
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
