//! Blocking HTTPS transport for the desktop control plane.
//!
//! Ordinary hostname, validity and chain verification and the configured leaf
//! certificate or SPKI pin are enforced together by rustls before the TLS
//! handshake completes. Redirects and ambient system proxies are disabled so
//! the authenticated origin cannot silently change underneath the pin.

use std::{
    io::Read,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use reqwest::{
    blocking::{Client, Response},
    header::{HeaderValue, ACCEPT, AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE},
    redirect::Policy,
    tls::TlsInfo,
    StatusCode,
};
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::CryptoProvider,
    pki_types::{CertificateDer, ServerName, UnixTime},
    CertificateError, ClientConfig, DigitallySignedStruct, DistinguishedName, Error as TlsError,
    SignatureScheme,
};
use rustls_platform_verifier::Verifier;
use sha2::{Digest, Sha256};
use x509_parser::parse_x509_certificate;
use zeroize::Zeroizing;

use crate::{
    RemoteBackendError, RemoteTransport, TlsPin, TlsPinKind, TransportRequest, TransportResponse,
    MAX_MESSAGE_BYTES, PROTOCOL_VERSION,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Production desktop transport for a user-operated, pinned HTTPS endpoint.
///
/// The pairing response supplies an application credential. Later exchanges
/// send it as a sensitive bearer header, giving mutual application-layer
/// authentication on top of server-authenticated TLS. Client-certificate mTLS
/// can be layered in later without weakening this pin check.
#[derive(Clone)]
pub struct PinnedHttpsTransport {
    tls_provider: Arc<CryptoProvider>,
    ordinary_tls_verifier: Arc<dyn ServerCertVerifier>,
}

impl PinnedHttpsTransport {
    pub fn new() -> Result<Self, RemoteBackendError> {
        let tls_provider = default_tls_provider();
        let ordinary_tls_verifier = Verifier::new(tls_provider.clone()).map_err(|error| {
            RemoteBackendError::Transport(format!(
                "Could not initialize the pinned HTTPS client: {error}"
            ))
        })?;
        Ok(Self {
            tls_provider,
            ordinary_tls_verifier: Arc::new(ordinary_tls_verifier),
        })
    }

    fn client_for_pin(
        &self,
        expected_pin: TlsPin,
    ) -> Result<(Client, Arc<AtomicBool>), RemoteBackendError> {
        let pin_mismatch = Arc::new(AtomicBool::new(false));
        let verifier = PinningServerCertVerifier {
            ordinary: self.ordinary_tls_verifier.clone(),
            expected_pin,
            pin_mismatch: pin_mismatch.clone(),
        };
        let mut tls = ClientConfig::builder_with_provider(self.tls_provider.clone())
            .with_protocol_versions(rustls::DEFAULT_VERSIONS)
            .map_err(|error| {
                RemoteBackendError::Transport(format!(
                    "Could not initialize the pinned HTTPS client: {error}"
                ))
            })?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth();
        tls.alpn_protocols = vec![b"http/1.1".to_vec()];
        tls.enable_early_data = false;

        let client = Client::builder()
            .tls_backend_preconfigured(tls)
            .https_only(true)
            .http1_only()
            .redirect(Policy::none())
            .no_proxy()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .pool_max_idle_per_host(0)
            .tls_info(true)
            .user_agent(concat!("VeriSilo/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| {
                RemoteBackendError::Transport(format!(
                    "Could not initialize the pinned HTTPS client: {error}"
                ))
            })?;
        Ok((client, pin_mismatch))
    }

    fn exchange_inner(
        &self,
        request: TransportRequest<'_>,
    ) -> Result<TransportResponse, RemoteBackendError> {
        request.endpoint.validate()?;
        if request.payload.len() > MAX_MESSAGE_BYTES {
            return Err(RemoteBackendError::LimitExceeded(format!(
                "request is {} bytes; maximum is {MAX_MESSAGE_BYTES}",
                request.payload.len()
            )));
        }

        let (client, pin_mismatch) = self.client_for_pin(request.endpoint.pin.clone())?;
        let mut builder = client
            .post(&request.endpoint.origin)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .header(CACHE_CONTROL, "no-store")
            .header("x-verisilo-protocol", PROTOCOL_VERSION.to_string())
            .body(request.payload.to_vec());
        if let Some(credential) = request.credential {
            validate_bearer_credential(credential)?;
            let authorization = Zeroizing::new(format!("Bearer {credential}"));
            let mut value = HeaderValue::from_str(authorization.as_str()).map_err(|_| {
                RemoteBackendError::InvalidRequest(
                    "Remote credential is not a valid bearer token.".to_owned(),
                )
            })?;
            value.set_sensitive(true);
            builder = builder.header(AUTHORIZATION, value);
        }

        let response = builder
            .send()
            .map_err(|error| map_transport_error(error, &pin_mismatch))?;
        validate_http_response(request.endpoint.pin.clone(), response)
    }
}

fn default_tls_provider() -> Arc<CryptoProvider> {
    CryptoProvider::get_default()
        .cloned()
        .unwrap_or_else(|| Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
}

#[derive(Debug)]
struct PinningServerCertVerifier {
    ordinary: Arc<dyn ServerCertVerifier>,
    expected_pin: TlsPin,
    pin_mismatch: Arc<AtomicBool>,
}

impl ServerCertVerifier for PinningServerCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        let verified = self.ordinary.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        )?;
        let observed_pin = peer_pin(&self.expected_pin.kind, end_entity.as_ref())
            .map_err(|_| TlsError::InvalidCertificate(CertificateError::BadEncoding))?;
        if observed_pin.sha256 != self.expected_pin.sha256 {
            self.pin_mismatch.store(true, Ordering::Release);
            return Err(TlsError::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            ));
        }
        Ok(verified)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        self.ordinary.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        self.ordinary.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.ordinary.supported_verify_schemes()
    }

    fn requires_raw_public_keys(&self) -> bool {
        self.ordinary.requires_raw_public_keys()
    }

    fn root_hint_subjects(&self) -> Option<&[DistinguishedName]> {
        self.ordinary.root_hint_subjects()
    }
}

impl RemoteTransport for PinnedHttpsTransport {
    fn exchange(
        &mut self,
        request: TransportRequest<'_>,
    ) -> Result<TransportResponse, RemoteBackendError> {
        self.exchange_inner(request)
    }
}

fn validate_http_response(
    expected_pin: TlsPin,
    mut response: Response,
) -> Result<TransportResponse, RemoteBackendError> {
    let certificate = response
        .extensions()
        .get::<TlsInfo>()
        .and_then(TlsInfo::peer_certificate)
        .ok_or_else(|| {
            RemoteBackendError::Transport(
                "TLS completed without exposing the authenticated leaf certificate.".to_owned(),
            )
        })?;
    let observed_pin = peer_pin(&expected_pin.kind, certificate)?;
    if observed_pin.sha256 != expected_pin.sha256 {
        return Err(RemoteBackendError::PinMismatch);
    }
    if response.status() != StatusCode::OK {
        return Err(RemoteBackendError::Transport(format!(
            "Pinned Remote Agent returned HTTP {}; only 200 is accepted.",
            response.status().as_u16()
        )));
    }
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if content_type
        .split(';')
        .next()
        .is_none_or(|value| !value.trim().eq_ignore_ascii_case("application/json"))
    {
        return Err(RemoteBackendError::Transport(
            "Pinned Remote Agent did not return application/json.".to_owned(),
        ));
    }

    let mut payload = Vec::new();
    response
        .by_ref()
        .take((MAX_MESSAGE_BYTES + 1) as u64)
        .read_to_end(&mut payload)
        .map_err(|error| {
            RemoteBackendError::Transport(format!("Could not read remote response: {error}"))
        })?;
    if payload.len() > MAX_MESSAGE_BYTES {
        return Err(RemoteBackendError::LimitExceeded(format!(
            "response exceeds {MAX_MESSAGE_BYTES} bytes"
        )));
    }

    Ok(TransportResponse {
        tls_validated: true,
        peer_pin: observed_pin,
        payload,
    })
}

fn peer_pin(kind: &TlsPinKind, certificate_der: &[u8]) -> Result<TlsPin, RemoteBackendError> {
    let sha256 = match kind {
        TlsPinKind::CertificateSha256 => sha256_hex(certificate_der),
        TlsPinKind::SpkiSha256 => {
            let (remaining, certificate) =
                parse_x509_certificate(certificate_der).map_err(|_| {
                    RemoteBackendError::Transport(
                        "The authenticated peer certificate is not valid X.509 DER.".to_owned(),
                    )
                })?;
            if !remaining.is_empty() {
                return Err(RemoteBackendError::Transport(
                    "The authenticated peer certificate contains trailing DER data.".to_owned(),
                ));
            }
            sha256_hex(certificate.public_key().raw)
        }
    };
    Ok(TlsPin {
        kind: kind.clone(),
        sha256,
    })
}

fn sha256_hex(value: &[u8]) -> String {
    let digest = Sha256::digest(value);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

fn validate_bearer_credential(credential: &str) -> Result<(), RemoteBackendError> {
    if !(32..=512).contains(&credential.len())
        || !credential
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(RemoteBackendError::InvalidRequest(
            "Remote credential must be 32-512 base64url characters.".to_owned(),
        ));
    }
    Ok(())
}

fn map_transport_error(error: reqwest::Error, pin_mismatch: &AtomicBool) -> RemoteBackendError {
    if pin_mismatch.load(Ordering::Acquire) {
        return RemoteBackendError::PinMismatch;
    }
    let category = if error.is_timeout() {
        "timed out"
    } else if error.is_connect() {
        "connection or TLS validation failed"
    } else if error.is_redirect() {
        "redirect was refused"
    } else {
        "request failed"
    };
    RemoteBackendError::Transport(format!("Remote Agent {category}: {error}"))
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Cursor, Read as _, Write as _},
        net::{TcpListener, TcpStream},
        sync::mpsc::{self, Receiver},
        thread::{self, JoinHandle},
    };

    use rustls::{
        client::WebPkiServerVerifier, RootCertStore, ServerConfig, ServerConnection, StreamOwned,
    };

    use super::*;
    use crate::{EndpointOwnership, RemoteEndpoint};

    const TEST_CA_PEM: &[u8] = include_bytes!("../tests/fixtures/transport-test-ca.pem");
    const TEST_LEAF_PEM: &[u8] = include_bytes!("../tests/fixtures/transport-test-leaf.pem");
    const TEST_LEAF_KEY_PEM: &[u8] =
        include_bytes!("../tests/fixtures/transport-test-leaf-key.pem");

    struct LocalTlsServer {
        origin: String,
        observed_request: Receiver<Vec<u8>>,
        worker: JoinHandle<()>,
    }

    impl LocalTlsServer {
        fn finish(self) -> Vec<u8> {
            let observed = self.observed_request.recv().unwrap();
            self.worker.join().unwrap();
            observed
        }
    }

    fn spawn_local_tls_server() -> LocalTlsServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let certificates = rustls_pemfile::certs(&mut Cursor::new(TEST_LEAF_PEM))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let private_key = rustls_pemfile::private_key(&mut Cursor::new(TEST_LEAF_KEY_PEM))
            .unwrap()
            .unwrap();
        let mut config = ServerConfig::builder_with_provider(default_tls_provider())
            .with_protocol_versions(rustls::DEFAULT_VERSIONS)
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(certificates, private_key)
            .unwrap();
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let config = std::sync::Arc::new(config);
        let (sender, observed_request) = mpsc::channel();
        let worker = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            configure_test_stream(&stream);
            let connection = ServerConnection::new(config).unwrap();
            let mut stream = StreamOwned::new(connection, stream);
            let mut observed = Vec::new();
            let mut buffer = [0_u8; 4 * 1024];
            loop {
                match stream.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => {
                        observed.extend_from_slice(&buffer[..read]);
                        if request_is_complete(&observed) {
                            stream
                                .write_all(
                                    b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}",
                                )
                                .unwrap();
                            stream.flush().unwrap();
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            sender.send(observed).unwrap();
        });
        LocalTlsServer {
            origin: format!("https://127.0.0.1:{}/", address.port()),
            observed_request,
            worker,
        }
    }

    fn configure_test_stream(stream: &TcpStream) {
        let timeout = Some(Duration::from_secs(5));
        stream.set_read_timeout(timeout).unwrap();
        stream.set_write_timeout(timeout).unwrap();
    }

    fn request_is_complete(bytes: &[u8]) -> bool {
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut request = httparse::Request::new(&mut headers);
        let Ok(httparse::Status::Complete(header_bytes)) = request.parse(bytes) else {
            return false;
        };
        let Some(content_length) = request
            .headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case("content-length"))
            .and_then(|header| std::str::from_utf8(header.value).ok())
            .and_then(|value| value.parse::<usize>().ok())
        else {
            return false;
        };
        bytes.len() >= header_bytes + content_length
    }

    fn test_transport() -> PinnedHttpsTransport {
        let tls_provider = default_tls_provider();
        let root = rustls_pemfile::certs(&mut Cursor::new(TEST_CA_PEM))
            .next()
            .unwrap()
            .unwrap();
        let mut roots = RootCertStore::empty();
        roots.add(root).unwrap();
        let ordinary_tls_verifier =
            WebPkiServerVerifier::builder_with_provider(Arc::new(roots), tls_provider.clone())
                .build()
                .unwrap();
        PinnedHttpsTransport {
            tls_provider,
            ordinary_tls_verifier,
        }
    }

    fn endpoint(origin: String, pin: TlsPin) -> RemoteEndpoint {
        RemoteEndpoint {
            ownership: EndpointOwnership::UserSelfHosted,
            origin,
            pin,
        }
    }

    fn test_leaf_der() -> Vec<u8> {
        rustls_pemfile::certs(&mut Cursor::new(TEST_LEAF_PEM))
            .next()
            .unwrap()
            .unwrap()
            .to_vec()
    }

    #[test]
    fn wrong_certificate_pin_sends_no_pairing_token_or_http_bytes() {
        let server = spawn_local_tls_server();
        let endpoint = endpoint(
            server.origin.clone(),
            TlsPin {
                kind: TlsPinKind::CertificateSha256,
                sha256: "a".repeat(64),
            },
        );
        let payload = br#"{"pairingToken":"PAIRING_SECRET_IN_BODY"}"#;

        let error = test_transport()
            .exchange_inner(TransportRequest {
                endpoint: &endpoint,
                credential: None,
                payload,
            })
            .unwrap_err();
        let observed = server.finish();

        assert!(matches!(error, RemoteBackendError::PinMismatch));
        assert!(
            observed.is_empty(),
            "server observed HTTP bytes: {observed:?}"
        );
        assert!(!observed
            .windows(b"PAIRING_SECRET_IN_BODY".len())
            .any(|window| window == b"PAIRING_SECRET_IN_BODY"));
    }

    #[test]
    fn wrong_spki_pin_sends_no_authorization_or_body_bytes() {
        let server = spawn_local_tls_server();
        let endpoint = endpoint(
            server.origin.clone(),
            TlsPin {
                kind: TlsPinKind::SpkiSha256,
                sha256: "a".repeat(64),
            },
        );
        let credential = "B".repeat(32);
        let payload = br#"{"operation":"SECRET_OPERATION_BODY"}"#;

        let error = test_transport()
            .exchange_inner(TransportRequest {
                endpoint: &endpoint,
                credential: Some(&credential),
                payload,
            })
            .unwrap_err();
        let observed = server.finish();

        assert!(matches!(error, RemoteBackendError::PinMismatch));
        assert!(
            observed.is_empty(),
            "server observed HTTP bytes: {observed:?}"
        );
        assert!(!observed
            .windows(b"authorization".len())
            .any(|window| window.eq_ignore_ascii_case(b"authorization")));
        assert!(!observed
            .windows(credential.len())
            .any(|window| window == credential.as_bytes()));
        assert!(!observed
            .windows(b"SECRET_OPERATION_BODY".len())
            .any(|window| window == b"SECRET_OPERATION_BODY"));
    }

    #[test]
    fn correct_certificate_and_spki_pins_succeed() {
        let certificate = test_leaf_der();
        for kind in [TlsPinKind::CertificateSha256, TlsPinKind::SpkiSha256] {
            let server = spawn_local_tls_server();
            let expected_pin = peer_pin(&kind, &certificate).unwrap();
            let endpoint = endpoint(server.origin.clone(), expected_pin.clone());

            let response = test_transport()
                .exchange_inner(TransportRequest {
                    endpoint: &endpoint,
                    credential: None,
                    payload: br#"{"legitimate":true}"#,
                })
                .unwrap();
            let observed = server.finish();

            assert!(response.tls_validated);
            assert_eq!(response.peer_pin, expected_pin);
            assert_eq!(response.payload, b"{}");
            assert!(observed.starts_with(b"POST / HTTP/1.1\r\n"));
            assert!(observed.ends_with(br#"{"legitimate":true}"#));
        }
    }

    #[test]
    fn certificate_pin_is_lowercase_sha256() {
        let pin = peer_pin(&TlsPinKind::CertificateSha256, b"certificate").unwrap();
        assert_eq!(pin.kind, TlsPinKind::CertificateSha256);
        assert_eq!(
            pin.sha256,
            "03d66dd08835c1ca3f128cceacd1f31ac94163096b20f445ae84285bc0832d72"
        );
    }

    #[test]
    fn bearer_credentials_are_strict_base64url() {
        assert!(validate_bearer_credential(&"a".repeat(32)).is_ok());
        assert!(validate_bearer_credential("short").is_err());
        assert!(validate_bearer_credential(&format!("{}:", "a".repeat(31))).is_err());
        assert!(validate_bearer_credential(&"a".repeat(513)).is_err());
    }

    #[test]
    fn malformed_spki_input_fails_closed() {
        assert!(peer_pin(&TlsPinKind::SpkiSha256, b"not a certificate").is_err());
    }
}
