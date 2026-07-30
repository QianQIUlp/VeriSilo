//! Bounded TLS-only HTTP/1.1 listener for the self-hosted Remote Agent.
//!
//! This module does not provision certificates and never opens a plaintext
//! application listener. The operator supplies a normal PEM certificate chain
//! and one unencrypted private key through a strict local configuration file.
//! Every accepted TCP connection is wrapped in rustls before HTTP bytes are
//! parsed, serves exactly one request, and is then closed.

use std::{
    collections::HashSet,
    fs::{self, File},
    io::{BufReader, Cursor, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Component, Path, PathBuf},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use rustls::{ServerConfig, ServerConnection, StreamOwned};
use serde::Deserialize;
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

use verisilo_remote_backend::{
    agent::NodeDisclosure, CapabilityAvailability, RemoteCapability, RemoteOperation,
    MAX_MESSAGE_BYTES, PROTOCOL_VERSION,
};

const MAX_CONFIGURATION_BYTES: u64 = 64 * 1024;
const MAX_CERTIFICATE_CHAIN_BYTES: u64 = 512 * 1024;
const MAX_PRIVATE_KEY_BYTES: u64 = 64 * 1024;
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_HEADERS: usize = 64;
const MAX_PATH_BYTES: usize = 4_096;
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(15);
const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(30);
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Local-only provider selection. `Unavailable` is the honest deployable mode
/// when the operator has not installed a real fixed provider artifact.
#[derive(Clone, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LocalProviderConfiguration {
    Stdio {
        executable_path: PathBuf,
        executable_sha256: String,
        capabilities: Vec<RemoteCapability>,
    },
    Unavailable {
        capabilities: Vec<RemoteCapability>,
    },
}

impl LocalProviderConfiguration {
    pub fn capabilities(&self) -> &[RemoteCapability] {
        match self {
            Self::Stdio { capabilities, .. } | Self::Unavailable { capabilities } => capabilities,
        }
    }
}

/// Strict deployment configuration read only from an absolute local file.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteAgentServerConfiguration {
    pub listen_address: SocketAddr,
    pub tls_certificate_chain_path: PathBuf,
    pub tls_private_key_path: PathBuf,
    pub auth_state_path: PathBuf,
    pub agent_state_path: PathBuf,
    pub credential_lifetime_seconds: u64,
    pub node: NodeDisclosure,
    pub provider: LocalProviderConfiguration,
}

impl RemoteAgentServerConfiguration {
    pub fn validate(&self) -> Result<(), ServerError> {
        if self.listen_address.port() == 0
            || self.node.node_id.is_nil()
            || !(60..=24 * 60 * 60).contains(&self.credential_lifetime_seconds)
            || self.node.validate().is_err()
        {
            return Err(ServerError::InvalidConfiguration);
        }

        let mut paths = vec![
            &self.tls_certificate_chain_path,
            &self.tls_private_key_path,
            &self.auth_state_path,
            &self.agent_state_path,
        ];
        if let LocalProviderConfiguration::Stdio {
            executable_path, ..
        } = &self.provider
        {
            paths.push(executable_path);
        }
        if paths.iter().any(|path| !valid_absolute_path(path))
            || paths.iter().enumerate().any(|(index, path)| {
                paths[index + 1..]
                    .iter()
                    .any(|other| path.as_path() == other.as_path())
            })
            || !canonical_parent(&self.auth_state_path)
            || !canonical_parent(&self.agent_state_path)
            || !optional_private_state_file(&self.auth_state_path)
            || !optional_private_state_file(&self.agent_state_path)
            || !optional_private_state_file(&self.agent_state_path.with_extension("bak"))
        {
            return Err(ServerError::InvalidConfiguration);
        }

        validate_capabilities(self.provider.capabilities())?;
        match &self.provider {
            LocalProviderConfiguration::Unavailable { capabilities }
                if capabilities.iter().any(|capability| {
                    matches!(&capability.availability, CapabilityAvailability::Available)
                }) =>
            {
                return Err(ServerError::InvalidConfiguration)
            }
            LocalProviderConfiguration::Stdio {
                executable_sha256, ..
            } if !valid_sha256(executable_sha256) => return Err(ServerError::InvalidConfiguration),
            _ => {}
        }
        Ok(())
    }

    pub fn credential_lifetime_ms(&self) -> Result<u64, ServerError> {
        self.credential_lifetime_seconds
            .checked_mul(1_000)
            .ok_or(ServerError::InvalidConfiguration)
    }
}

/// Loads strict JSON without accepting symlinks, unknown fields, relative
/// paths, trailing JSON values, or a group/world-writable config file.
pub fn load_configuration(path: &Path) -> Result<RemoteAgentServerConfiguration, ServerError> {
    if !valid_absolute_path(path) {
        return Err(ServerError::InvalidConfigurationPath);
    }
    let canonical = fs::canonicalize(path).map_err(|_| ServerError::InvalidConfigurationPath)?;
    if canonical != path {
        return Err(ServerError::InvalidConfigurationPath);
    }
    reject_symlink(path)?;
    require_safe_config_permissions(path)?;
    let raw = read_bounded_file(path, MAX_CONFIGURATION_BYTES)?;
    let mut deserializer = serde_json::Deserializer::from_slice(&raw);
    let configuration = RemoteAgentServerConfiguration::deserialize(&mut deserializer)
        .map_err(|_| ServerError::InvalidConfiguration)?;
    deserializer
        .end()
        .map_err(|_| ServerError::InvalidConfiguration)?;
    configuration.validate()?;
    Ok(configuration)
}

/// Loads and validates one normal rustls server configuration. No certificate
/// generation, ACME client, plaintext challenge listener, or fallback TLS mode
/// exists here.
pub fn load_tls_configuration(
    configuration: &RemoteAgentServerConfiguration,
) -> Result<Arc<ServerConfig>, ServerError> {
    configuration.validate()?;
    let certificate_bytes = strict_deployment_file(
        &configuration.tls_certificate_chain_path,
        MAX_CERTIFICATE_CHAIN_BYTES,
        false,
    )?;
    let private_key_bytes = Zeroizing::new(strict_deployment_file(
        &configuration.tls_private_key_path,
        MAX_PRIVATE_KEY_BYTES,
        true,
    )?);
    let certificate_labels = validate_pem_structure(&certificate_bytes, &["CERTIFICATE"])?;
    let private_key_labels = validate_pem_structure(
        &private_key_bytes,
        &["PRIVATE KEY", "RSA PRIVATE KEY", "EC PRIVATE KEY"],
    )?;
    if certificate_labels.is_empty() || private_key_labels.len() != 1 {
        return Err(ServerError::InvalidTlsMaterial);
    }

    let certificates = rustls_pemfile::certs(&mut Cursor::new(&certificate_bytes))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ServerError::InvalidTlsMaterial)?;
    if certificates.len() != certificate_labels.len() {
        return Err(ServerError::InvalidTlsMaterial);
    }
    let private_key = rustls_pemfile::private_key(&mut Cursor::new(private_key_bytes.as_slice()))
        .map_err(|_| ServerError::InvalidTlsMaterial)?
        .ok_or(ServerError::InvalidTlsMaterial)?;
    let mut tls = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certificates, private_key)
        .map_err(|_| ServerError::InvalidTlsMaterial)?;
    tls.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(tls))
}

/// Application boundary used by the listener. Implementations receive only a
/// validated bearer token and bounded JSON bytes.
pub trait JsonRequestHandler {
    fn handle_json(&mut self, bearer: Option<&str>, body: &[u8]) -> ApplicationResponse;
    fn maintenance_tick(&mut self);
}

/// Bounded response body. Pairing credentials are zeroized after the TLS write.
pub struct ApplicationResponse {
    status_code: u16,
    body: Vec<u8>,
}

impl ApplicationResponse {
    pub fn new(status_code: u16, body: Vec<u8>) -> Self {
        Self { status_code, body }
    }

    fn valid(&self) -> bool {
        matches!(self.status_code, 200 | 400 | 401 | 403 | 500 | 503)
            && self.body.len() <= MAX_MESSAGE_BYTES
    }
}

impl Drop for ApplicationResponse {
    fn drop(&mut self) {
        self.body.zeroize();
    }
}

/// Runs the TLS listener. Accept is polled with a finite interval so TTL
/// maintenance runs when there is no traffic; a slow client is bounded by the
/// 15-second connection timeout and cannot starve maintenance indefinitely.
///
/// Requests and provider calls are intentionally serialized to preserve the
/// single-writer ordering of auth and Agent stores. A deployment exposed to an
/// untrusted WAN still needs an outer connection/rate-limiting layer; this V0.9
/// listener is not a general-purpose high-concurrency edge proxy.
pub fn serve_https(
    configuration: &RemoteAgentServerConfiguration,
    tls: Arc<ServerConfig>,
    handler: &mut impl JsonRequestHandler,
) -> Result<(), ServerError> {
    configuration.validate()?;
    let listener = TcpListener::bind(configuration.listen_address)
        .map_err(|_| ServerError::ListenerUnavailable)?;
    listener
        .set_nonblocking(true)
        .map_err(|_| ServerError::ListenerUnavailable)?;
    let mut next_maintenance = Instant::now();
    loop {
        let now = Instant::now();
        if now >= next_maintenance {
            handler.maintenance_tick();
            // Schedule from completion so repeated failures or a slow fixed
            // provider are retried at most once per interval, never in a busy
            // catch-up loop.
            next_maintenance = Instant::now() + MAINTENANCE_INTERVAL;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if configure_stream(&stream).is_err() {
                    continue;
                }
                let connection = match ServerConnection::new(tls.clone()) {
                    Ok(connection) => connection,
                    Err(_) => return Err(ServerError::TlsUnavailable),
                };
                let mut stream = StreamOwned::new(connection, stream);
                // Error details are deliberately not logged: they can contain
                // parser context adjacent to bearer/token bytes.
                let _ = serve_one_connection(&mut stream, handler);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                let until_maintenance = next_maintenance.saturating_duration_since(Instant::now());
                thread::sleep(ACCEPT_POLL_INTERVAL.min(until_maintenance));
            }
            Err(_) => thread::sleep(ACCEPT_POLL_INTERVAL),
        }
    }
}

fn serve_one_connection(
    stream: &mut StreamOwned<ServerConnection, TcpStream>,
    handler: &mut impl JsonRequestHandler,
) -> Result<(), ServerError> {
    match read_http_request(stream) {
        Ok(request) => {
            let response = handler.handle_json(request.bearer.as_deref(), &request.body);
            drop(request);
            if response.valid() {
                write_http_response(stream, response.status_code, &response.body)?;
            } else {
                write_fixed_error(stream, 500, "internal_error")?;
            }
        }
        Err(rejection) => {
            write_fixed_error(stream, rejection.status_code(), rejection.public_code())?;
        }
    }
    stream.flush().map_err(|_| ServerError::ConnectionFailed)
}

fn configure_stream(stream: &TcpStream) -> Result<(), ServerError> {
    stream
        .set_read_timeout(Some(CONNECTION_TIMEOUT))
        .and_then(|_| stream.set_write_timeout(Some(CONNECTION_TIMEOUT)))
        .and_then(|_| stream.set_nodelay(true))
        .map_err(|_| ServerError::ConnectionFailed)
}

/// Parsed form used by parser tests and the live TLS connection. `Drop`
/// zeroizes bearer and body bytes.
pub struct ParsedHttpRequest {
    bearer: Option<String>,
    body: Vec<u8>,
}

impl ParsedHttpRequest {
    pub fn bearer(&self) -> Option<&str> {
        self.bearer.as_deref()
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }
}

impl Drop for ParsedHttpRequest {
    fn drop(&mut self) {
        if let Some(bearer) = &mut self.bearer {
            bearer.zeroize();
        }
        self.body.zeroize();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpRejection {
    BadRequest,
    MethodNotAllowed,
    PayloadTooLarge,
    HeadersTooLarge,
    RequestTimeout,
}

impl HttpRejection {
    pub fn status_code(self) -> u16 {
        match self {
            Self::BadRequest => 400,
            Self::MethodNotAllowed => 405,
            Self::PayloadTooLarge => 413,
            Self::HeadersTooLarge => 431,
            Self::RequestTimeout => 408,
        }
    }

    fn public_code(self) -> &'static str {
        match self {
            Self::BadRequest => "bad_request",
            Self::MethodNotAllowed => "method_not_allowed",
            Self::PayloadTooLarge => "payload_too_large",
            Self::HeadersTooLarge => "headers_too_large",
            Self::RequestTimeout => "request_timeout",
        }
    }
}

/// Parses one complete HTTP request. The input must contain exactly the body
/// declared by Content-Length; chunked framing and pipelining are rejected.
pub fn parse_http_request_bytes(bytes: &[u8]) -> Result<ParsedHttpRequest, HttpRejection> {
    let header_end = find_header_end(bytes).ok_or(HttpRejection::BadRequest)?;
    if header_end > MAX_HEADER_BYTES {
        return Err(HttpRejection::HeadersTooLarge);
    }
    let head = parse_http_head(&bytes[..header_end])?;
    let expected = header_end
        .checked_add(head.content_length)
        .ok_or(HttpRejection::PayloadTooLarge)?;
    if expected != bytes.len() {
        return Err(HttpRejection::BadRequest);
    }
    Ok(ParsedHttpRequest {
        bearer: head.bearer,
        body: bytes[header_end..].to_vec(),
    })
}

fn read_http_request(
    reader: &mut StreamOwned<ServerConnection, TcpStream>,
) -> Result<ParsedHttpRequest, HttpRejection> {
    let deadline = Instant::now() + CONNECTION_TIMEOUT;
    let mut raw = Vec::with_capacity(4 * 1024);
    let mut buffer = [0_u8; 4 * 1024];
    let header_end = loop {
        let read = read_before_deadline(reader, &mut buffer, deadline)?;
        if read == 0 {
            raw.zeroize();
            return Err(HttpRejection::BadRequest);
        }
        raw.extend_from_slice(&buffer[..read]);
        if let Some(end) = find_header_end(&raw) {
            if end > MAX_HEADER_BYTES {
                raw.zeroize();
                return Err(HttpRejection::HeadersTooLarge);
            }
            break end;
        }
        if raw.len() > MAX_HEADER_BYTES {
            raw.zeroize();
            return Err(HttpRejection::HeadersTooLarge);
        }
    };

    let head = match parse_http_head(&raw[..header_end]) {
        Ok(head) => head,
        Err(error) => {
            raw.zeroize();
            return Err(error);
        }
    };
    let expected = header_end
        .checked_add(head.content_length)
        .ok_or(HttpRejection::PayloadTooLarge)?;
    if raw.len() > expected {
        raw.zeroize();
        return Err(HttpRejection::BadRequest);
    }
    while raw.len() < expected {
        let remaining = expected - raw.len();
        let read_limit = remaining.min(buffer.len());
        let read = read_before_deadline(reader, &mut buffer[..read_limit], deadline)?;
        if read == 0 {
            raw.zeroize();
            return Err(HttpRejection::BadRequest);
        }
        raw.extend_from_slice(&buffer[..read]);
    }
    let parsed = parse_http_request_bytes(&raw);
    raw.zeroize();
    parsed
}

fn read_before_deadline(
    reader: &mut StreamOwned<ServerConnection, TcpStream>,
    buffer: &mut [u8],
    deadline: Instant,
) -> Result<usize, HttpRejection> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(HttpRejection::RequestTimeout);
    }
    reader
        .sock
        .set_read_timeout(Some(remaining))
        .map_err(|_| HttpRejection::BadRequest)?;
    reader.read(buffer).map_err(|error| match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
            HttpRejection::RequestTimeout
        }
        _ => HttpRejection::BadRequest,
    })
}

struct ParsedHttpHead {
    content_length: usize,
    bearer: Option<String>,
}

fn parse_http_head(bytes: &[u8]) -> Result<ParsedHttpHead, HttpRejection> {
    let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
    let mut request = httparse::Request::new(&mut headers);
    let parsed = request
        .parse(bytes)
        .map_err(|_| HttpRejection::BadRequest)?;
    let httparse::Status::Complete(consumed) = parsed else {
        return Err(HttpRejection::BadRequest);
    };
    if consumed != bytes.len()
        || request.method != Some("POST")
        || request.path != Some("/")
        || request.version != Some(1)
    {
        return if request.method != Some("POST") {
            Err(HttpRejection::MethodNotAllowed)
        } else {
            Err(HttpRejection::BadRequest)
        };
    }

    let mut seen = HashSet::new();
    let mut host = None;
    let mut content_length = None;
    let mut content_type = None;
    let mut protocol = None;
    let mut bearer = None;
    for header in request.headers.iter() {
        let name = header.name.to_ascii_lowercase();
        if !seen.insert(name.clone()) {
            return Err(HttpRejection::BadRequest);
        }
        let value = std::str::from_utf8(header.value).map_err(|_| HttpRejection::BadRequest)?;
        if value.trim() != value || value.chars().any(char::is_control) {
            return Err(HttpRejection::BadRequest);
        }
        match name.as_str() {
            "host" => host = Some(value),
            "content-length" => content_length = Some(parse_content_length(value)?),
            "content-type" => content_type = Some(value),
            "x-verisilo-protocol" => protocol = Some(value),
            "authorization" => bearer = Some(parse_bearer(value)?),
            "transfer-encoding" | "upgrade" | "expect" | "proxy-authorization" | "cookie" => {
                return Err(HttpRejection::BadRequest)
            }
            "connection"
                if value
                    .split(',')
                    .any(|token| token.trim().eq_ignore_ascii_case("upgrade")) =>
            {
                return Err(HttpRejection::BadRequest)
            }
            _ => {}
        }
    }

    if host.is_none_or(|value| value.is_empty() || value.len() > 255)
        || content_type.is_none_or(|value| !value.eq_ignore_ascii_case("application/json"))
        || protocol != Some("1")
    {
        return Err(HttpRejection::BadRequest);
    }
    Ok(ParsedHttpHead {
        content_length: content_length.ok_or(HttpRejection::BadRequest)?,
        bearer,
    })
}

fn parse_content_length(value: &str) -> Result<usize, HttpRejection> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(HttpRejection::BadRequest);
    }
    let length = value
        .parse::<usize>()
        .map_err(|_| HttpRejection::PayloadTooLarge)?;
    if length > MAX_MESSAGE_BYTES {
        return Err(HttpRejection::PayloadTooLarge);
    }
    Ok(length)
}

fn parse_bearer(value: &str) -> Result<String, HttpRejection> {
    let token = value
        .strip_prefix("Bearer ")
        .ok_or(HttpRejection::BadRequest)?;
    if !(32..=512).contains(&token.len())
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(HttpRejection::BadRequest);
    }
    Ok(token.to_owned())
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

fn write_fixed_error(
    writer: &mut impl Write,
    status_code: u16,
    code: &'static str,
) -> Result<(), ServerError> {
    let body = format!("{{\"error\":\"{code}\"}}");
    write_http_response(writer, status_code, body.as_bytes())
}

fn write_http_response(
    writer: &mut impl Write,
    status_code: u16,
    body: &[u8],
) -> Result<(), ServerError> {
    if body.len() > MAX_MESSAGE_BYTES {
        return Err(ServerError::ConnectionFailed);
    }
    let reason = match status_code {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => return Err(ServerError::ConnectionFailed),
    };
    let head = format!(
        "HTTP/1.1 {status_code} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nX-VeriSilo-Protocol: {}\r\nConnection: close\r\n\r\n",
        body.len(),
        PROTOCOL_VERSION
    );
    writer
        .write_all(head.as_bytes())
        .and_then(|_| writer.write_all(body))
        .map_err(|_| ServerError::ConnectionFailed)
}

fn validate_capabilities(capabilities: &[RemoteCapability]) -> Result<(), ServerError> {
    if capabilities.len() != RemoteOperation::ALL.len()
        || RemoteOperation::ALL.iter().any(|operation| {
            capabilities
                .iter()
                .filter(|item| item.operation == *operation)
                .count()
                != 1
        })
        || capabilities
            .iter()
            .any(|capability| match &capability.availability {
                CapabilityAvailability::Available => false,
                CapabilityAvailability::Unavailable { reason } => {
                    reason.is_empty()
                        || reason.len() > 512
                        || reason.trim() != reason
                        || reason.chars().any(char::is_control)
                }
            })
    {
        return Err(ServerError::InvalidConfiguration);
    }
    if capability_is_available(capabilities, RemoteOperation::Create)
        && !capability_is_available(capabilities, RemoteOperation::Destroy)
    {
        return Err(ServerError::InvalidConfiguration);
    }
    Ok(())
}

fn capability_is_available(capabilities: &[RemoteCapability], operation: RemoteOperation) -> bool {
    capabilities.iter().any(|capability| {
        capability.operation == operation
            && matches!(&capability.availability, CapabilityAvailability::Available)
    })
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && !value.bytes().all(|byte| byte == b'0')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_absolute_path(path: &Path) -> bool {
    path.is_absolute()
        && path.as_os_str().len() <= MAX_PATH_BYTES
        && path.file_name().is_some()
        && path
            .components()
            .all(|component| !matches!(component, Component::CurDir | Component::ParentDir))
}

fn canonical_parent(path: &Path) -> bool {
    path.parent()
        .and_then(|parent| fs::canonicalize(parent).ok().map(|value| value == parent))
        .unwrap_or(false)
}

fn optional_private_state_file(path: &Path) -> bool {
    if !path.exists() {
        return true;
    }
    match fs::canonicalize(path) {
        Ok(canonical) if canonical == path => {}
        _ => return false,
    }
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o777 == 0o600
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn strict_deployment_file(
    path: &Path,
    maximum_bytes: u64,
    private: bool,
) -> Result<Vec<u8>, ServerError> {
    if !valid_absolute_path(path) {
        return Err(ServerError::InvalidConfiguration);
    }
    let canonical = fs::canonicalize(path).map_err(|_| ServerError::InvalidTlsMaterial)?;
    if canonical != path {
        return Err(ServerError::InvalidTlsMaterial);
    }
    reject_symlink(path)?;
    if private {
        require_private_permissions(path)?;
    }
    read_bounded_file(path, maximum_bytes)
}

fn read_bounded_file(path: &Path, maximum_bytes: u64) -> Result<Vec<u8>, ServerError> {
    let metadata = fs::metadata(path).map_err(|_| ServerError::FileUnavailable)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > maximum_bytes {
        return Err(ServerError::FileUnavailable);
    }
    let file = File::open(path).map_err(|_| ServerError::FileUnavailable)?;
    let mut raw = Vec::with_capacity(metadata.len() as usize);
    BufReader::new(file)
        .take(maximum_bytes + 1)
        .read_to_end(&mut raw)
        .map_err(|_| ServerError::FileUnavailable)?;
    if raw.len() as u64 > maximum_bytes {
        return Err(ServerError::FileUnavailable);
    }
    Ok(raw)
}

fn reject_symlink(path: &Path) -> Result<(), ServerError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ServerError::FileUnavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ServerError::FileUnavailable);
    }
    Ok(())
}

#[cfg(unix)]
fn require_private_permissions(path: &Path) -> Result<(), ServerError> {
    use std::os::unix::fs::PermissionsExt;

    let mode = fs::metadata(path)
        .map_err(|_| ServerError::FileUnavailable)?
        .permissions()
        .mode()
        & 0o777;
    if mode != 0o600 {
        return Err(ServerError::UnsafePermissions);
    }
    Ok(())
}

#[cfg(not(unix))]
fn require_private_permissions(_path: &Path) -> Result<(), ServerError> {
    Err(ServerError::UnsupportedPlatform)
}

#[cfg(unix)]
fn require_safe_config_permissions(path: &Path) -> Result<(), ServerError> {
    use std::os::unix::fs::PermissionsExt;

    let mode = fs::metadata(path)
        .map_err(|_| ServerError::FileUnavailable)?
        .permissions()
        .mode()
        & 0o777;
    if mode & 0o022 != 0 {
        return Err(ServerError::UnsafePermissions);
    }
    Ok(())
}

#[cfg(not(unix))]
fn require_safe_config_permissions(_path: &Path) -> Result<(), ServerError> {
    Err(ServerError::UnsupportedPlatform)
}

fn validate_pem_structure(
    bytes: &[u8],
    allowed_labels: &[&str],
) -> Result<Vec<String>, ServerError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ServerError::InvalidTlsMaterial)?;
    if !text.is_ascii() {
        return Err(ServerError::InvalidTlsMaterial);
    }
    let mut active: Option<String> = None;
    let mut labels = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        match &active {
            None if line.trim().is_empty() => {}
            None => {
                let label = line
                    .strip_prefix("-----BEGIN ")
                    .and_then(|value| value.strip_suffix("-----"))
                    .filter(|value| allowed_labels.contains(value))
                    .ok_or(ServerError::InvalidTlsMaterial)?;
                active = Some(label.to_owned());
            }
            Some(label) if line == format!("-----END {label}-----") => {
                labels.push(label.clone());
                active = None;
            }
            Some(_) => {
                if line.is_empty()
                    || line.len() > 128
                    || !line.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=')
                    })
                {
                    return Err(ServerError::InvalidTlsMaterial);
                }
            }
        }
    }
    if active.is_some() {
        return Err(ServerError::InvalidTlsMaterial);
    }
    Ok(labels)
}

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("Remote Agent configuration path is invalid")]
    InvalidConfigurationPath,
    #[error("Remote Agent configuration is invalid")]
    InvalidConfiguration,
    #[error("required local file is unavailable or outside its size bound")]
    FileUnavailable,
    #[error("local configuration or key permissions are unsafe")]
    UnsafePermissions,
    #[error("this secure listener configuration is unsupported on the current platform")]
    UnsupportedPlatform,
    #[error("TLS certificate chain or private key is invalid")]
    InvalidTlsMaterial,
    #[error("TLS listener address is unavailable")]
    ListenerUnavailable,
    #[error("TLS server state is unavailable")]
    TlsUnavailable,
    #[error("TLS connection failed")]
    ConnectionFailed,
}
