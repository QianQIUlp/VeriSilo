#![cfg(unix)]

#[allow(dead_code)]
#[path = "../src/https_server.rs"]
mod https_server;

use https_server::{parse_http_request_bytes, HttpRejection, RemoteAgentServerConfiguration};
use uuid::Uuid;
use verisilo_remote_backend::CapabilityAvailability;

fn request(headers: &str, body: &[u8]) -> Vec<u8> {
    let mut raw = format!(
        "POST / HTTP/1.1\r\nHost: agent.example\r\nContent-Type: application/json\r\nX-VeriSilo-Protocol: 1\r\nContent-Length: {}\r\n{headers}\r\n",
        body.len()
    )
    .into_bytes();
    raw.extend_from_slice(body);
    raw
}

fn rejects(raw: &[u8], expected: HttpRejection) {
    match parse_http_request_bytes(raw) {
        Err(actual) => assert_eq!(actual, expected),
        Ok(_) => panic!("request unexpectedly passed strict HTTP parsing"),
    }
}

#[test]
fn accepts_one_strict_json_post_with_optional_bearer() {
    let token = "a".repeat(64);
    let raw = request(&format!("Authorization: Bearer {token}\r\n"), b"{}");
    let parsed = parse_http_request_bytes(&raw).unwrap();
    assert_eq!(parsed.bearer(), Some(token.as_str()));
    assert_eq!(parsed.body(), b"{}");

    let pairing = parse_http_request_bytes(&request("", b"{}")).unwrap();
    assert_eq!(pairing.bearer(), None);
}

#[test]
fn rejects_chunking_upgrade_expect_and_sensitive_ambient_headers() {
    for header in [
        "Transfer-Encoding: chunked\r\n",
        "Upgrade: websocket\r\n",
        "Connection: keep-alive, Upgrade\r\n",
        "Expect: 100-continue\r\n",
        "Cookie: secret=value\r\n",
        "Proxy-Authorization: Basic abc\r\n",
    ] {
        rejects(&request(header, b"{}"), HttpRejection::BadRequest);
    }
}

#[test]
fn rejects_duplicate_headers_case_insensitively() {
    let duplicate_length = b"POST / HTTP/1.1\r\nHost: agent.example\r\nContent-Type: application/json\r\nX-VeriSilo-Protocol: 1\r\nContent-Length: 2\r\ncontent-length: 2\r\n\r\n{}";
    rejects(duplicate_length, HttpRejection::BadRequest);

    let token = "a".repeat(64);
    let duplicate_auth = request(
        &format!("Authorization: Bearer {token}\r\nauthorization: Bearer {token}\r\n"),
        b"{}",
    );
    rejects(&duplicate_auth, HttpRejection::BadRequest);
}

#[test]
fn rejects_wrong_method_path_version_media_type_and_protocol() {
    let valid = request("", b"{}");
    let get = String::from_utf8(valid.clone())
        .unwrap()
        .replacen("POST /", "GET /", 1);
    rejects(get.as_bytes(), HttpRejection::MethodNotAllowed);

    let path = String::from_utf8(valid.clone())
        .unwrap()
        .replace("POST / HTTP", "POST /health HTTP");
    rejects(path.as_bytes(), HttpRejection::BadRequest);
    let version = String::from_utf8(valid.clone())
        .unwrap()
        .replace("HTTP/1.1", "HTTP/1.0");
    rejects(version.as_bytes(), HttpRejection::BadRequest);
    let media = String::from_utf8(valid.clone())
        .unwrap()
        .replace("application/json", "application/json; charset=utf-8");
    rejects(media.as_bytes(), HttpRejection::BadRequest);
    let protocol = String::from_utf8(valid)
        .unwrap()
        .replace("X-VeriSilo-Protocol: 1", "X-VeriSilo-Protocol: 2");
    rejects(protocol.as_bytes(), HttpRejection::BadRequest);
}

#[test]
fn enforces_exact_content_length_and_64_kib_body_limit() {
    let trailing = request("", b"{}extra");
    let mut declared_short = String::from_utf8(trailing).unwrap();
    declared_short = declared_short.replacen("Content-Length: 7", "Content-Length: 2", 1);
    rejects(declared_short.as_bytes(), HttpRejection::BadRequest);

    let oversized = b"POST / HTTP/1.1\r\nHost: agent.example\r\nContent-Type: application/json\r\nX-VeriSilo-Protocol: 1\r\nContent-Length: 65537\r\n\r\n";
    rejects(oversized, HttpRejection::PayloadTooLarge);

    let maximum = vec![b'a'; 64 * 1024];
    let parsed = parse_http_request_bytes(&request("", &maximum)).unwrap();
    assert_eq!(parsed.body().len(), 64 * 1024);
}

#[test]
fn enforces_header_limit_and_strict_bearer_syntax() {
    let huge = format!("X-Fill: {}\r\n", "a".repeat(17 * 1024));
    rejects(&request(&huge, b"{}"), HttpRejection::HeadersTooLarge);

    for authorization in [
        "Authorization: bearer aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n",
        "Authorization: Bearer short\r\n",
        "Authorization: Bearer aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:\r\n",
    ] {
        rejects(
            &request(authorization, b"{}"),
            HttpRejection::BadRequest,
        );
    }
}

#[test]
fn checked_in_unavailable_configuration_is_strict_and_valid() {
    let raw = include_str!("../verisilo-remote-agent.example.json");
    let mut configuration: RemoteAgentServerConfiguration = serde_json::from_str(raw).unwrap();
    assert_eq!(configuration.provider.capabilities().len(), 9);
    assert!(configuration
        .provider
        .capabilities()
        .iter()
        .all(|capability| matches!(
            &capability.availability,
            CapabilityAvailability::Unavailable { .. }
        )));

    // Deployment-file existence and PEM parsing are serve-time checks. Replace
    // the example host paths with one canonical temporary parent so this test
    // exercises every other strict configuration invariant without requiring
    // operator certificates on the CI worker.
    let root =
        std::env::temp_dir().join(format!("verisilo-agent-example-config-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    configuration.tls_certificate_chain_path = root.join("fullchain.pem");
    configuration.tls_private_key_path = root.join("private-key.pem");
    configuration.auth_state_path = root.join("auth-state.json");
    configuration.agent_state_path = root.join("agent-state.json");
    configuration.validate().unwrap();
    std::fs::remove_dir(root).unwrap();
}
