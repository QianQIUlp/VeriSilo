use std::{
    env, fs,
    io::{self, Read, Write},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::domain::app_data_root;

const PROTOCOL_VERSION: u32 = 1;
const MAX_MESSAGE_BYTES: usize = 256 * 1024;
const HOST_ALLOWLIST_FILE: &str = "native-host-allowlist.json";

#[derive(Debug, Error)]
pub enum NativeHostError {
    #[error("Native host caller origin is missing or unauthorized.")]
    UnauthorizedOrigin,
    #[error("Native host message is invalid.")]
    InvalidMessage,
    #[error("Native host message exceeds the maximum size.")]
    MessageTooLarge,
    #[error("Native host I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("Native host serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum NativeRequest {
    Handshake {
        protocol_version: u32,
        request_id: Uuid,
    },
    GetRuntimeStatus {
        protocol_version: u32,
        request_id: Uuid,
    },
    OpenDesktop {
        protocol_version: u32,
        request_id: Uuid,
    },
}

impl NativeRequest {
    fn is_current_protocol(&self) -> bool {
        match self {
            Self::Handshake {
                protocol_version, ..
            }
            | Self::GetRuntimeStatus {
                protocol_version, ..
            }
            | Self::OpenDesktop {
                protocol_version, ..
            } => *protocol_version == PROTOCOL_VERSION,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum NativeResponse {
    HandshakeAck {
        protocol_version: u32,
        request_id: Uuid,
        product: &'static str,
    },
    Error {
        request_id: Option<Uuid>,
        code: &'static str,
        message: &'static str,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostAllowlist {
    allowed_extension_ids: Vec<String>,
}

pub fn run_native_host() -> Result<(), NativeHostError> {
    let origin = env::args()
        .nth(1)
        .ok_or(NativeHostError::UnauthorizedOrigin)?;
    if !is_allowed_origin(&origin) {
        return Err(NativeHostError::UnauthorizedOrigin);
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    loop {
        let payload = match read_frame(&mut reader) {
            Ok(Some(payload)) => payload,
            Ok(None) => return Ok(()),
            Err(error) => {
                let _ = write_frame(
                    &mut writer,
                    &NativeResponse::Error {
                        request_id: None,
                        code: "invalid_message",
                        message: "Native host rejected an invalid message.",
                    },
                );
                return Err(error);
            }
        };

        let raw: Value = match serde_json::from_slice(&payload) {
            Ok(value) if !contains_sensitive_key(&value) => value,
            _ => {
                write_frame(
                    &mut writer,
                    &NativeResponse::Error {
                        request_id: None,
                        code: "invalid_message",
                        message:
                            "Sensitive browser state is not accepted by VeriSilo Native Messaging.",
                    },
                )?;
                continue;
            }
        };

        let request: NativeRequest = match serde_json::from_value::<NativeRequest>(raw) {
            Ok(request) if request.is_current_protocol() => request,
            _ => {
                write_frame(
                    &mut writer,
                    &NativeResponse::Error {
                        request_id: None,
                        code: "invalid_message",
                        message: "Native host rejected an unsupported message.",
                    },
                )?;
                continue;
            }
        };

        let response = match request {
            NativeRequest::Handshake { request_id, .. } => NativeResponse::HandshakeAck {
                protocol_version: PROTOCOL_VERSION,
                request_id,
                product: "VeriSilo",
            },
            NativeRequest::GetRuntimeStatus { request_id, .. } => NativeResponse::Error {
                request_id: Some(request_id),
                code: "unavailable",
                message: "Live desktop runtime status is unavailable through Native Messaging.",
            },
            NativeRequest::OpenDesktop { request_id, .. } => NativeResponse::Error {
                request_id: Some(request_id),
                code: "unavailable",
                message: "Open the VeriSilo desktop application directly.",
            },
        };
        write_frame(&mut writer, &response)?;
    }
}

fn is_allowed_origin(origin: &str) -> bool {
    let Some(extension_id) = extension_id_from_origin(origin) else {
        return false;
    };
    load_allowed_extension_ids()
        .iter()
        .any(|allowed_id| allowed_id == &extension_id)
}

fn extension_id_from_origin(origin: &str) -> Option<String> {
    let extension_id = origin
        .strip_prefix("chrome-extension://")?
        .strip_suffix('/')?;
    if extension_id.len() == 32
        && extension_id
            .chars()
            .all(|character| matches!(character, 'a'..='p'))
    {
        Some(extension_id.to_owned())
    } else {
        None
    }
}

fn load_allowed_extension_ids() -> Vec<String> {
    if cfg!(debug_assertions) {
        if let Ok(ids) = env::var("VERISILO_DEV_EXTENSION_IDS") {
            return ids
                .split(',')
                .map(str::trim)
                .filter(|id| {
                    id.len() == 32 && id.chars().all(|character| matches!(character, 'a'..='p'))
                })
                .map(str::to_owned)
                .collect();
        }
    }

    let Ok(root) = app_data_root() else {
        return Vec::new();
    };
    let path = root.join(HOST_ALLOWLIST_FILE);
    let Ok(raw) = fs::read(path) else {
        return Vec::new();
    };
    serde_json::from_slice::<HostAllowlist>(&raw)
        .map(|allowlist| {
            allowlist
                .allowed_extension_ids
                .into_iter()
                .filter(|id| {
                    id.len() == 32 && id.chars().all(|character| matches!(character, 'a'..='p'))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn read_frame(reader: &mut impl Read) -> Result<Option<Vec<u8>>, NativeHostError> {
    let mut length_bytes = [0_u8; 4];
    match reader.read_exact(&mut length_bytes) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(NativeHostError::Io(error)),
    }
    let length = u32::from_ne_bytes(length_bytes) as usize;
    if length > MAX_MESSAGE_BYTES {
        return Err(NativeHostError::MessageTooLarge);
    }
    let mut payload = vec![0_u8; length];
    reader.read_exact(&mut payload)?;
    Ok(Some(payload))
}

fn write_frame(writer: &mut impl Write, response: &NativeResponse) -> Result<(), NativeHostError> {
    let payload = serde_json::to_vec(response)?;
    let length = u32::try_from(payload.len()).map_err(|_| NativeHostError::MessageTooLarge)?;
    writer.write_all(&length.to_ne_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

fn contains_sensitive_key(value: &Value) -> bool {
    const FORBIDDEN_KEYWORDS: [&str; 6] = [
        "cookie",
        "authorization",
        "password",
        "credential",
        "localstorage",
        "indexeddb",
    ];
    match value {
        Value::Array(values) => values.iter().any(contains_sensitive_key),
        Value::Object(values) => values.iter().any(|(key, nested)| {
            let normalized = key.to_ascii_lowercase();
            FORBIDDEN_KEYWORDS
                .iter()
                .any(|keyword| normalized.contains(keyword))
                || contains_sensitive_key(nested)
        }),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{
        contains_sensitive_key, extension_id_from_origin, read_frame, NativeHostError,
        MAX_MESSAGE_BYTES,
    };
    use serde_json::json;

    #[test]
    fn parses_only_valid_extension_origins() {
        assert_eq!(
            extension_id_from_origin("chrome-extension://abcdefghijklmnopabcdefghijklmnop/"),
            Some("abcdefghijklmnopabcdefghijklmnop".to_owned())
        );
        assert_eq!(extension_id_from_origin("https://example.test/"), None);
    }

    #[test]
    fn rejects_cookie_payloads() {
        assert!(contains_sensitive_key(
            &json!({ "nested": { "cookieValue": "x" } })
        ));
    }

    #[test]
    fn rejects_oversized_native_messages_before_allocation() {
        let frame = ((MAX_MESSAGE_BYTES as u32) + 1).to_ne_bytes().to_vec();
        let error = read_frame(&mut Cursor::new(frame)).expect_err("reject oversized frame");
        assert!(matches!(error, NativeHostError::MessageTooLarge));
    }
}
