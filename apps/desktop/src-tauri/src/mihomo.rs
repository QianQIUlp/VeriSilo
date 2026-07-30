use std::{
    collections::HashMap,
    io::{self, Read, Write},
    net::{IpAddr, SocketAddr, TcpStream},
    time::Duration,
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use url::Url;
use zeroize::{Zeroize, Zeroizing};

use crate::{domain::ExternalMihomoBinding, vault::MihomoControllerAuthentication};

const CONTROLLER_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MihomoControllerInput {
    pub controller_url: String,
    #[serde(default)]
    pub secret: String,
}

impl Drop for MihomoControllerInput {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MihomoSnapshot {
    pub checked_at: String,
    pub groups: Vec<MihomoSelectorGroup>,
    pub providers: Vec<MihomoProvider>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MihomoProvider {
    pub name: String,
    pub vehicle_type: Option<String>,
    pub updated_at: Option<String>,
    pub node_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MihomoSelectorGroup {
    pub name: String,
    pub selected: Option<String>,
    pub nodes: Vec<MihomoNode>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MihomoNode {
    pub name: String,
    pub proxy_type: Option<String>,
    pub delay_ms: Option<u64>,
    pub alive: Option<bool>,
}

#[derive(Debug, Error)]
pub enum MihomoError {
    #[error("Mihomo Controller 只允许显式本机 HTTP 地址，例如 http://127.0.0.1:9090/。")]
    UnsafeController,
    #[error("Mihomo Controller Secret 过长或包含不支持的字符。")]
    InvalidSecret,
    #[error("无法连接本机 Mihomo Controller：{0}")]
    Io(#[from] io::Error),
    #[error("本机 Mihomo Controller 返回 HTTP {0}；请检查地址和 Secret。")]
    HttpStatus(u16),
    #[error("本机 Mihomo Controller 返回了无法识别的响应。")]
    InvalidResponse,
    #[error("没有找到 Mihomo 选择组“{0}”。")]
    SelectorNotFound(String),
    #[error("Mihomo 选择组“{group}”中不存在节点“{node}”。")]
    NodeNotFound { group: String, node: String },
    #[error("Mihomo 报告节点“{0}”当前不可用。")]
    NodeUnavailable(String),
    #[error("Mihomo 回读结果显示选择组“{group}”没有保持在节点“{node}”。")]
    SelectionNotApplied { group: String, node: String },
}

pub fn inspect_controller(input: &MihomoControllerInput) -> Result<MihomoSnapshot, MihomoError> {
    validate_secret(&input.secret)?;
    let body = controller_request(
        &input.controller_url,
        "GET",
        "/proxies",
        &input.secret,
        None,
    )?;
    let mut snapshot = parse_snapshot(&body)?;
    snapshot.providers = controller_request(
        &input.controller_url,
        "GET",
        "/providers/proxies",
        &input.secret,
        None,
    )
    .ok()
    .and_then(|body| parse_providers(&body).ok())
    .unwrap_or_default();
    Ok(snapshot)
}

pub fn apply_binding(
    binding: &ExternalMihomoBinding,
    authentication: Option<&MihomoControllerAuthentication>,
) -> Result<(), MihomoError> {
    let secret = authentication.map_or("", MihomoControllerAuthentication::secret);
    validate_secret(secret)?;

    let before = controller_request(&binding.controller_url, "GET", "/proxies", secret, None)?;
    let before = parse_snapshot(&before)?;
    let group = before
        .groups
        .iter()
        .find(|group| group.name == binding.selector_group)
        .ok_or_else(|| MihomoError::SelectorNotFound(binding.selector_group.clone()))?;
    let Some(node) = group
        .nodes
        .iter()
        .find(|node| node.name == binding.node_name)
    else {
        return Err(MihomoError::NodeNotFound {
            group: binding.selector_group.clone(),
            node: binding.node_name.clone(),
        });
    };
    if node.alive == Some(false) {
        return Err(MihomoError::NodeUnavailable(binding.node_name.clone()));
    }

    let path = format!("/proxies/{}", encode_path_segment(&binding.selector_group));
    let body = serde_json::to_vec(&serde_json::json!({ "name": binding.node_name }))
        .map_err(|_| MihomoError::InvalidResponse)?;
    controller_request(&binding.controller_url, "PUT", &path, secret, Some(&body))?;

    let after = controller_request(&binding.controller_url, "GET", "/proxies", secret, None)?;
    let after = parse_snapshot(&after)?;
    let selected = after
        .groups
        .iter()
        .find(|group| group.name == binding.selector_group)
        .and_then(|group| group.selected.as_deref());
    if selected != Some(binding.node_name.as_str()) {
        return Err(MihomoError::SelectionNotApplied {
            group: binding.selector_group.clone(),
            node: binding.node_name.clone(),
        });
    }
    if after
        .groups
        .iter()
        .find(|group| group.name == binding.selector_group)
        .and_then(|group| {
            group
                .nodes
                .iter()
                .find(|node| node.name == binding.node_name)
        })
        .is_some_and(|node| node.alive == Some(false))
    {
        return Err(MihomoError::NodeUnavailable(binding.node_name.clone()));
    }
    Ok(())
}

fn validate_secret(secret: &str) -> Result<(), MihomoError> {
    if secret.len() > 1_024 || secret.chars().any(char::is_control) {
        return Err(MihomoError::InvalidSecret);
    }
    Ok(())
}

fn parse_snapshot(body: &[u8]) -> Result<MihomoSnapshot, MihomoError> {
    let root: Value = serde_json::from_slice(body).map_err(|_| MihomoError::InvalidResponse)?;
    let proxies = root
        .get("proxies")
        .and_then(Value::as_object)
        .ok_or(MihomoError::InvalidResponse)?;
    let mut groups = Vec::new();
    for (group_name, group_value) in proxies {
        if bounded_controller_text(group_name, 128).is_none() || groups.len() >= 256 {
            continue;
        }
        let Some(node_names) = group_value.get("all").and_then(Value::as_array) else {
            continue;
        };
        let mut nodes = node_names
            .iter()
            .filter_map(Value::as_str)
            .filter(|node_name| bounded_controller_text(node_name, 256).is_some())
            .take(2_048)
            .map(|node_name| {
                let node = proxies.get(node_name);
                MihomoNode {
                    name: node_name.to_owned(),
                    proxy_type: node
                        .and_then(|value| value.get("type"))
                        .and_then(Value::as_str)
                        .and_then(|value| bounded_controller_text(value, 64))
                        .map(str::to_owned),
                    delay_ms: node.and_then(latest_delay),
                    alive: node
                        .and_then(|value| value.get("alive"))
                        .and_then(Value::as_bool),
                }
            })
            .collect::<Vec<_>>();
        nodes.sort_by(|left, right| left.name.cmp(&right.name));
        groups.push(MihomoSelectorGroup {
            name: group_name.clone(),
            selected: group_value
                .get("now")
                .and_then(Value::as_str)
                .and_then(|value| bounded_controller_text(value, 256))
                .map(str::to_owned),
            nodes,
        });
    }
    groups.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(MihomoSnapshot {
        checked_at: Utc::now().to_rfc3339(),
        groups,
        providers: Vec::new(),
    })
}

fn parse_providers(body: &[u8]) -> Result<Vec<MihomoProvider>, MihomoError> {
    let root: Value = serde_json::from_slice(body).map_err(|_| MihomoError::InvalidResponse)?;
    let providers = root
        .get("providers")
        .and_then(Value::as_object)
        .ok_or(MihomoError::InvalidResponse)?;
    let mut result = providers
        .iter()
        .filter_map(|(provider_name, provider)| {
            let name = provider
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(provider_name);
            Some(MihomoProvider {
                name: bounded_controller_text(name, 256)?.to_owned(),
                vehicle_type: provider
                    .get("vehicleType")
                    .and_then(Value::as_str)
                    .and_then(|value| bounded_controller_text(value, 64))
                    .map(str::to_owned),
                updated_at: provider
                    .get("updatedAt")
                    .and_then(Value::as_str)
                    .and_then(|value| bounded_controller_text(value, 128))
                    .map(str::to_owned),
                node_count: provider
                    .get("proxies")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len),
            })
        })
        .take(100)
        .collect::<Vec<_>>();
    result.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(result)
}

fn bounded_controller_text(value: &str, maximum: usize) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty() && value.chars().count() <= maximum && !value.chars().any(char::is_control))
        .then_some(value)
}

fn latest_delay(value: &Value) -> Option<u64> {
    value
        .get("history")
        .and_then(Value::as_array)
        .and_then(|history| history.last())
        .and_then(|entry| entry.get("delay"))
        .and_then(Value::as_u64)
        .filter(|delay| *delay > 0)
}

fn controller_request(
    controller_url: &str,
    method: &str,
    path: &str,
    secret: &str,
    body: Option<&[u8]>,
) -> Result<Vec<u8>, MihomoError> {
    let endpoint = controller_endpoint(controller_url)?;
    let mut stream = TcpStream::connect_timeout(&endpoint, CONTROLLER_TIMEOUT)?;
    stream.set_read_timeout(Some(CONTROLLER_TIMEOUT))?;
    stream.set_write_timeout(Some(CONTROLLER_TIMEOUT))?;
    let authorization = Zeroizing::new(if secret.is_empty() {
        String::new()
    } else {
        format!("Authorization: Bearer {secret}\r\n")
    });
    let body = body.unwrap_or_default();
    let content_headers = if body.is_empty() {
        String::new()
    } else {
        format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        )
    };
    let host_header = if endpoint.is_ipv6() {
        format!("[{}]:{}", endpoint.ip(), endpoint.port())
    } else {
        format!("{}:{}", endpoint.ip(), endpoint.port())
    };
    let request = Zeroizing::new(format!(
        "{method} {path} HTTP/1.1\r\nHost: {host_header}\r\nAccept: application/json\r\n{}{content_headers}Connection: close\r\n\r\n",
        authorization.as_str(),
    ));
    stream.write_all(request.as_bytes())?;
    stream.write_all(body)?;

    let response = read_http_response(&mut stream)?;
    parse_http_response(&response)
}

fn read_http_response(stream: &mut TcpStream) -> Result<Vec<u8>, MihomoError> {
    let mut response = Vec::new();
    let mut buffer = [0_u8; 8 * 1_024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                response.extend_from_slice(&buffer[..read]);
                if response.len() > MAX_RESPONSE_BYTES {
                    return Err(MihomoError::InvalidResponse);
                }
                if http_response_is_complete(&response) {
                    break;
                }
            }
            Err(error)
                if error.kind() == io::ErrorKind::ConnectionReset
                    && http_response_is_complete(&response) =>
            {
                break;
            }
            Err(error) => return Err(MihomoError::Io(error)),
        }
    }
    if response.is_empty() {
        return Err(MihomoError::InvalidResponse);
    }
    Ok(response)
}

fn http_response_is_complete(response: &[u8]) -> bool {
    let Some(header_end) = response.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let Ok(header) = std::str::from_utf8(&response[..header_end]) else {
        return false;
    };
    let headers = header
        .split("\r\n")
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect::<HashMap<_, _>>();
    let body = &response[header_end + 4..];
    if let Some(length) = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
    {
        return body.len() >= length;
    }
    headers
        .get("transfer-encoding")
        .is_some_and(|value| value.eq_ignore_ascii_case("chunked") && decode_chunked(body).is_ok())
}

fn controller_endpoint(controller_url: &str) -> Result<SocketAddr, MihomoError> {
    let url = Url::parse(controller_url).map_err(|_| MihomoError::UnsafeController)?;
    let host = url
        .host_str()
        .ok_or(MihomoError::UnsafeController)?
        .trim_matches(['[', ']']);
    let ip = host
        .parse::<IpAddr>()
        .map_err(|_| MihomoError::UnsafeController)?;
    if url.scheme() != "http"
        || !ip.is_loopback()
        || url.port().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(MihomoError::UnsafeController);
    }
    Ok(SocketAddr::new(
        ip,
        url.port().ok_or(MihomoError::UnsafeController)?,
    ))
}

fn parse_http_response(response: &[u8]) -> Result<Vec<u8>, MihomoError> {
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or(MihomoError::InvalidResponse)?;
    let header =
        std::str::from_utf8(&response[..header_end]).map_err(|_| MihomoError::InvalidResponse)?;
    let mut lines = header.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split_ascii_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or(MihomoError::InvalidResponse)?;
    if !(200..300).contains(&status) {
        return Err(MihomoError::HttpStatus(status));
    }
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect::<HashMap<_, _>>();
    let body = &response[header_end + 4..];
    if headers
        .get("transfer-encoding")
        .is_some_and(|value| value.eq_ignore_ascii_case("chunked"))
    {
        decode_chunked(body)
    } else if let Some(length) = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
    {
        body.get(..length)
            .map(<[u8]>::to_vec)
            .ok_or(MihomoError::InvalidResponse)
    } else {
        Ok(body.to_vec())
    }
}

fn decode_chunked(mut body: &[u8]) -> Result<Vec<u8>, MihomoError> {
    let mut decoded = Vec::new();
    loop {
        let line_end = body
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or(MihomoError::InvalidResponse)?;
        let size_text = std::str::from_utf8(&body[..line_end])
            .map_err(|_| MihomoError::InvalidResponse)?
            .split(';')
            .next()
            .ok_or(MihomoError::InvalidResponse)?;
        let size = usize::from_str_radix(size_text.trim(), 16)
            .map_err(|_| MihomoError::InvalidResponse)?;
        body = &body[line_end + 2..];
        if size == 0 {
            return Ok(decoded);
        }
        let chunk = body.get(..size).ok_or(MihomoError::InvalidResponse)?;
        if decoded.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(MihomoError::InvalidResponse);
        }
        decoded.extend_from_slice(chunk);
        body = body.get(size + 2..).ok_or(MihomoError::InvalidResponse)?;
    }
}

fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(*byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use super::{apply_binding, inspect_controller, MihomoControllerInput};
    use crate::domain::ExternalMihomoBinding;

    const RESPONSE_BEFORE: &str = r#"{"proxies":{"GLOBAL":{"type":"Selector","now":"old","all":["old","Tokyo 01"]},"old":{"type":"Socks5","alive":true,"history":[{"delay":120}]},"Tokyo 01":{"type":"Socks5","alive":true,"history":[{"delay":42}]}}}"#;
    const RESPONSE_AFTER: &str = r#"{"proxies":{"GLOBAL":{"type":"Selector","now":"Tokyo 01","all":["old","Tokyo 01"]},"old":{"type":"Socks5"},"Tokyo 01":{"type":"Socks5","alive":true,"history":[{"delay":42}]}}}"#;
    const PROVIDER_RESPONSE: &str = r#"{"providers":{"airport":{"name":"My subscription","vehicleType":"HTTP","updatedAt":"2026-07-26T12:00:00Z","proxies":[{},{}]}}}"#;

    fn write_json(stream: &mut std::net::TcpStream, body: &str) {
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .expect("write fake controller response");
    }

    fn read_request(stream: &mut std::net::TcpStream) -> String {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        let expected_length = loop {
            let read = stream.read(&mut buffer).expect("read controller request");
            assert!(read > 0, "controller request closed before headers");
            request.extend_from_slice(&buffer[..read]);
            let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let header = std::str::from_utf8(&request[..header_end])
                .expect("controller request headers are UTF-8");
            let content_length = header
                .split("\r\n")
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .map(|(_, value)| {
                    value
                        .trim()
                        .parse::<usize>()
                        .expect("valid request content length")
                })
                .unwrap_or(0);
            break header_end + 4 + content_length;
        };
        while request.len() < expected_length {
            let read = stream.read(&mut buffer).expect("read controller body");
            assert!(read > 0, "controller request closed before body");
            request.extend_from_slice(&buffer[..read]);
        }
        String::from_utf8(request).expect("controller request is UTF-8")
    }

    #[test]
    fn controller_snapshot_and_binding_are_checked_on_loopback() {
        let listener =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("bind fake controller");
        let address = listener.local_addr().expect("fake controller address");
        let server = thread::spawn(move || {
            for index in 0..5 {
                let (mut stream, _) = listener.accept().expect("accept controller request");
                let request_text = read_request(&mut stream);
                assert!(request_text.contains("Authorization: Bearer controller-secret"));
                if request_text.starts_with("GET /providers/proxies HTTP/1.1") {
                    write_json(&mut stream, PROVIDER_RESPONSE);
                } else if request_text.starts_with("PUT /proxies/GLOBAL HTTP/1.1") {
                    assert!(request_text.starts_with("PUT /proxies/GLOBAL HTTP/1.1"));
                    assert!(request_text.contains("Content-Length:"));
                    write_json(&mut stream, "{}");
                } else if index == 4 {
                    write_json(&mut stream, RESPONSE_AFTER);
                } else {
                    write_json(&mut stream, RESPONSE_BEFORE);
                }
            }
        });

        let controller_url = format!("http://127.0.0.1:{}/", address.port());
        let snapshot = inspect_controller(&MihomoControllerInput {
            controller_url: controller_url.clone(),
            secret: "controller-secret".to_owned(),
        })
        .expect("inspect controller");
        assert_eq!(snapshot.groups[0].name, "GLOBAL");
        assert_eq!(snapshot.groups[0].nodes[0].delay_ms, Some(42));
        assert_eq!(snapshot.providers[0].name, "My subscription");
        assert_eq!(snapshot.providers[0].node_count, 2);

        let authentication =
            crate::vault::MihomoControllerAuthentication::new("controller-secret".to_owned());
        apply_binding(
            &ExternalMihomoBinding {
                controller_url,
                selector_group: "GLOBAL".to_owned(),
                node_name: "Tokyo 01".to_owned(),
                controller_secret_reference: None,
            },
            Some(&authentication),
        )
        .expect("apply binding");
        server.join().expect("fake controller exits");
    }

    #[test]
    fn remote_controllers_are_rejected() {
        assert!(inspect_controller(&MihomoControllerInput {
            controller_url: "http://192.0.2.10:9090/".to_owned(),
            secret: String::new(),
        })
        .is_err());
    }
}
