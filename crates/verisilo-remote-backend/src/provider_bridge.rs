//! Fixed executable bridge between the network-facing Agent and an
//! administrator-installed environment provider.
//!
//! The remote caller never supplies an executable, path, shell command or raw
//! argument list. The operator pins one absolute provider executable and its
//! SHA-256 in the Agent configuration. Every call rechecks the digest, launches
//! that file directly with one fixed protocol argument, sends a strict typed
//! request on stdin, and accepts at most one bounded strict JSON response.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use verisilo_remote_backend::{
    agent::{
        AgentError, AgentProvider, EnvironmentRecord, InputEvent, KeyCustody, LifecycleReceipt,
        ProviderDeletionReceipt, ProvisionReceipt, ResourceDeletionItem, ScreenChannel,
        ScreenTransport, VolumeAttestation,
    },
    GuestEvidence, RemoteCapability, RemoteOperation, MAX_MESSAGE_BYTES,
};

const PROVIDER_PROTOCOL_VERSION: u16 = 1;
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_PROVIDER_STDERR_BYTES: u64 = 8 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderBridgeConfiguration {
    pub executable_path: PathBuf,
    pub executable_sha256: String,
    pub capabilities: Vec<RemoteCapability>,
}

#[derive(Debug, Serialize)]
#[serde(
    tag = "operation",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum ProviderRequest<'a> {
    Create {
        protocol_version: u16,
        record: &'a EnvironmentRecord,
    },
    Lifecycle {
        protocol_version: u16,
        lifecycle_operation: RemoteOperation,
        record: &'a EnvironmentRecord,
        log_limit: Option<u16>,
    },
    Destroy {
        protocol_version: u16,
        record: &'a EnvironmentRecord,
    },
    OpenScreen {
        protocol_version: u16,
        record: &'a EnvironmentRecord,
        authorization_id: Uuid,
        expires_at_unix_ms: u64,
    },
    SendInput {
        protocol_version: u16,
        record: &'a EnvironmentRecord,
        authorization_id: Uuid,
        events: &'a [InputEvent],
    },
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum ProviderResponse {
    Provisioned {
        volume: VolumeAttestation,
        evidence: GuestEvidence,
    },
    Lifecycle {
        evidence: Option<GuestEvidence>,
        logs: Vec<String>,
    },
    Deleted {
        receipt_id: Uuid,
        remote_environment_id: Uuid,
        volume_id: Uuid,
        resource_deletions: Vec<ResourceDeletionItem>,
    },
    Screen {
        channel_id: Uuid,
        remote_environment_id: Uuid,
        authorization_id: Uuid,
        expires_at_unix_ms: u64,
        transport: ScreenTransport,
    },
    InputAccepted {
        accepted: bool,
    },
    Rejected {
        code: String,
        message: String,
    },
}

pub struct StdioAgentProvider {
    executable_path: PathBuf,
    executable_sha256: String,
    capabilities: Vec<RemoteCapability>,
}

impl StdioAgentProvider {
    pub fn new(configuration: ProviderBridgeConfiguration) -> Result<Self, AgentError> {
        let executable_path = secure_provider_path(&configuration.executable_path)?;
        validate_digest(&configuration.executable_sha256)?;
        if configuration.capabilities.len() != RemoteOperation::ALL.len()
            || RemoteOperation::ALL.iter().any(|operation| {
                configuration
                    .capabilities
                    .iter()
                    .filter(|capability| capability.operation == *operation)
                    .count()
                    != 1
            })
        {
            return Err(AgentError::Provider(
                "Provider configuration must describe all nine operations exactly once.".to_owned(),
            ));
        }
        let provider = Self {
            executable_path,
            executable_sha256: configuration.executable_sha256,
            capabilities: configuration.capabilities,
        };
        provider.verify_executable()?;
        Ok(provider)
    }

    fn verify_executable(&self) -> Result<(), AgentError> {
        let current = secure_provider_path(&self.executable_path)?;
        if current != self.executable_path {
            return Err(AgentError::Provider(
                "Provider executable canonical path changed.".to_owned(),
            ));
        }
        let digest = sha256_file(&current)?;
        if !constant_time_eq(digest.as_bytes(), self.executable_sha256.as_bytes()) {
            return Err(AgentError::Provider(
                "Provider executable SHA-256 does not match the operator pin.".to_owned(),
            ));
        }
        Ok(())
    }

    fn exchange(&self, request: &ProviderRequest<'_>) -> Result<ProviderResponse, AgentError> {
        self.verify_executable()?;
        let payload = serde_json::to_vec(request).map_err(AgentError::Json)?;
        if payload.len() > MAX_MESSAGE_BYTES {
            return Err(AgentError::LimitExceeded(
                "Provider request exceeds 64 KiB.".to_owned(),
            ));
        }
        let mut child = Command::new(&self.executable_path)
            .arg("--verisilo-provider-v1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| AgentError::Provider(format!("Could not start provider: {error}")))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| AgentError::Provider("Provider stdin was not available.".to_owned()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AgentError::Provider("Provider stdout was not available.".to_owned()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AgentError::Provider("Provider stderr was not available.".to_owned()))?;
        let stdout_reader =
            thread::spawn(move || bounded_read(stdout, MAX_MESSAGE_BYTES as u64 + 1));
        let stderr_reader =
            thread::spawn(move || bounded_read(stderr, MAX_PROVIDER_STDERR_BYTES + 1));
        if let Err(error) = std::io::Write::write_all(&mut stdin, &payload) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AgentError::Provider(format!(
                "Could not write provider request: {error}"
            )));
        }
        drop(stdin);

        let deadline = Instant::now() + PROVIDER_TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(20));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(AgentError::Provider(
                        "Provider exceeded the fixed 30-second timeout and was stopped.".to_owned(),
                    ));
                }
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(AgentError::Provider(format!(
                        "Could not observe provider process: {error}"
                    )));
                }
            }
        };
        let stdout = stdout_reader
            .join()
            .map_err(|_| AgentError::Provider("Provider stdout reader panicked.".to_owned()))?
            .map_err(|error| {
                AgentError::Provider(format!("Could not read provider stdout: {error}"))
            })?;
        let stderr = stderr_reader
            .join()
            .map_err(|_| AgentError::Provider("Provider stderr reader panicked.".to_owned()))?
            .map_err(|error| {
                AgentError::Provider(format!("Could not read provider stderr: {error}"))
            })?;
        if stdout.len() > MAX_MESSAGE_BYTES || stderr.len() > MAX_PROVIDER_STDERR_BYTES as usize {
            return Err(AgentError::LimitExceeded(
                "Provider output exceeded its negotiated bound.".to_owned(),
            ));
        }
        if !status.success() {
            return Err(AgentError::Provider(format!(
                "Provider exited unsuccessfully: {}",
                sanitized_stderr(&stderr)
            )));
        }
        let response: ProviderResponse = serde_json::from_slice(&stdout).map_err(|error| {
            AgentError::Provider(format!("Provider returned invalid strict JSON: {error}"))
        })?;
        if let ProviderResponse::Rejected { code, message } = &response {
            if !valid_label(code, 80) || !valid_message(message, 1_024) {
                return Err(AgentError::Provider(
                    "Provider rejection was malformed.".to_owned(),
                ));
            }
            return Err(AgentError::Provider(format!(
                "Provider rejected operation ({code}): {message}"
            )));
        }
        Ok(response)
    }
}

impl AgentProvider for StdioAgentProvider {
    fn capabilities(&self) -> Vec<RemoteCapability> {
        self.capabilities.clone()
    }

    fn create(&mut self, record: &EnvironmentRecord) -> Result<ProvisionReceipt, AgentError> {
        match self.exchange(&ProviderRequest::Create {
            protocol_version: PROVIDER_PROTOCOL_VERSION,
            record,
        })? {
            ProviderResponse::Provisioned { volume, evidence } => {
                if !volume.encrypted || volume.key_custody != KeyCustody::UserControlled {
                    return Err(AgentError::Provider(
                        "Provider did not attest a user-controlled encrypted volume.".to_owned(),
                    ));
                }
                Ok(ProvisionReceipt { volume, evidence })
            }
            _ => Err(response_mismatch("provisioned")),
        }
    }

    fn lifecycle(
        &mut self,
        operation: RemoteOperation,
        record: &EnvironmentRecord,
        log_limit: Option<u16>,
    ) -> Result<LifecycleReceipt, AgentError> {
        match self.exchange(&ProviderRequest::Lifecycle {
            protocol_version: PROVIDER_PROTOCOL_VERSION,
            lifecycle_operation: operation,
            record,
            log_limit,
        })? {
            ProviderResponse::Lifecycle { evidence, logs } => {
                Ok(LifecycleReceipt { evidence, logs })
            }
            _ => Err(response_mismatch("lifecycle")),
        }
    }

    fn destroy(
        &mut self,
        record: &EnvironmentRecord,
    ) -> Result<ProviderDeletionReceipt, AgentError> {
        match self.exchange(&ProviderRequest::Destroy {
            protocol_version: PROVIDER_PROTOCOL_VERSION,
            record,
        })? {
            ProviderResponse::Deleted {
                receipt_id,
                remote_environment_id,
                volume_id,
                resource_deletions,
            } => Ok(ProviderDeletionReceipt {
                receipt_id,
                remote_environment_id,
                volume_id,
                resource_deletions,
            }),
            _ => Err(response_mismatch("deleted")),
        }
    }

    fn open_screen(
        &mut self,
        record: &EnvironmentRecord,
        authorization_id: Uuid,
        expires_at_unix_ms: u64,
    ) -> Result<ScreenChannel, AgentError> {
        match self.exchange(&ProviderRequest::OpenScreen {
            protocol_version: PROVIDER_PROTOCOL_VERSION,
            record,
            authorization_id,
            expires_at_unix_ms,
        })? {
            ProviderResponse::Screen {
                channel_id,
                remote_environment_id,
                authorization_id,
                expires_at_unix_ms,
                transport,
            } => Ok(ScreenChannel {
                channel_id,
                remote_environment_id,
                authorization_id,
                expires_at_unix_ms,
                transport,
            }),
            _ => Err(response_mismatch("screen")),
        }
    }

    fn send_input(
        &mut self,
        record: &EnvironmentRecord,
        authorization_id: Uuid,
        events: &[InputEvent],
    ) -> Result<(), AgentError> {
        match self.exchange(&ProviderRequest::SendInput {
            protocol_version: PROVIDER_PROTOCOL_VERSION,
            record,
            authorization_id,
            events,
        })? {
            ProviderResponse::InputAccepted { accepted: true } => Ok(()),
            ProviderResponse::InputAccepted { accepted: false } => Err(AgentError::Provider(
                "Provider did not affirm input acceptance.".to_owned(),
            )),
            _ => Err(response_mismatch("input_accepted")),
        }
    }
}

fn secure_provider_path(path: &Path) -> Result<PathBuf, AgentError> {
    if !path.is_absolute() {
        return Err(AgentError::Provider(
            "Provider executable path must be absolute.".to_owned(),
        ));
    }
    let canonical = fs::canonicalize(path).map_err(|error| {
        AgentError::Provider(format!("Provider executable is unavailable: {error}"))
    })?;
    let metadata = fs::metadata(&canonical).map_err(|error| {
        AgentError::Provider(format!("Provider metadata is unavailable: {error}"))
    })?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 2 * 1024 * 1024 * 1024 {
        return Err(AgentError::Provider(
            "Provider must be a non-empty regular file no larger than 2 GiB.".to_owned(),
        ));
    }
    Ok(canonical)
}

fn validate_digest(value: &str) -> Result<(), AgentError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || value.bytes().all(|byte| byte == b'0')
    {
        return Err(AgentError::Provider(
            "Provider SHA-256 pin must be non-zero lowercase hexadecimal.".to_owned(),
        ));
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, AgentError> {
    let mut file = fs::File::open(path).map_err(|error| {
        AgentError::Provider(format!("Could not open provider executable: {error}"))
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            AgentError::Provider(format!("Could not hash provider executable: {error}"))
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    Ok(encoded)
}

fn bounded_read(reader: impl Read, maximum: u64) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    reader.take(maximum).read_to_end(&mut output)?;
    Ok(output)
}

fn sanitized_stderr(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let cleaned = text
        .chars()
        .filter(|character| !character.is_control() || matches!(character, ' ' | '\n' | '\t'))
        .take(1_024)
        .collect::<String>();
    if cleaned.trim().is_empty() {
        "no bounded diagnostic".to_owned()
    } else {
        cleaned.trim().to_owned()
    }
}

fn valid_label(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_message(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= maximum
        && !value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn response_mismatch(expected: &str) -> AgentError {
    AgentError::Provider(format!(
        "Provider response type did not match expected {expected}."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use verisilo_remote_backend::{CapabilityAvailability, RemoteCapability};

    fn capabilities() -> Vec<RemoteCapability> {
        RemoteOperation::ALL
            .into_iter()
            .map(|operation| RemoteCapability {
                operation,
                availability: CapabilityAvailability::Available,
            })
            .collect()
    }

    #[test]
    fn configuration_rejects_relative_zero_digest_and_incomplete_capabilities() {
        let relative = ProviderBridgeConfiguration {
            executable_path: PathBuf::from("provider"),
            executable_sha256: "a".repeat(64),
            capabilities: capabilities(),
        };
        assert!(matches!(
            StdioAgentProvider::new(relative),
            Err(AgentError::Provider(_))
        ));
        assert!(validate_digest(&"0".repeat(64)).is_err());
        assert!(validate_digest(&"a".repeat(64)).is_ok());
        let mut incomplete = capabilities();
        incomplete.pop();
        assert_eq!(incomplete.len(), 8);
    }

    #[test]
    fn provider_request_has_no_executable_shell_path_or_argument_escape() {
        let record = EnvironmentRecord {
            silo_id: Uuid::new_v4(),
            binding_id: Uuid::new_v4(),
            remote_environment_id: Uuid::new_v4(),
            node_id: Uuid::new_v4(),
            state: verisilo_remote_backend::agent::EnvironmentState::Created,
            network: verisilo_remote_backend::RemoteNetworkPolicy::Direct,
            volume: VolumeAttestation {
                encrypted: true,
                key_custody: KeyCustody::UserControlled,
                volume_id: Uuid::new_v4(),
                key_id: Uuid::new_v4(),
            },
            created_at_unix_ms: 1,
            expires_at_unix_ms: 2,
            last_activity_at_unix_ms: 1,
            deletion_proof_id: None,
        };
        let value = serde_json::to_value(ProviderRequest::Create {
            protocol_version: PROVIDER_PROTOCOL_VERSION,
            record: &record,
        })
        .unwrap();
        let object = value.as_object().unwrap();
        for forbidden in ["command", "shell", "path", "args", "executable"] {
            assert!(!object.contains_key(forbidden));
        }
    }

    #[test]
    fn provider_response_rejects_unknown_fields() {
        let raw = br#"{"type":"input_accepted","accepted":true,"command":"bad"}"#;
        assert!(serde_json::from_slice::<ProviderResponse>(raw).is_err());

        let unknown_kind = br#"{"type":"deleted","receiptId":"90f0fd93-10d3-40de-946d-93021d42d5ce","remoteEnvironmentId":"7fdb87b6-28e6-4d2d-8e3f-65e4ef580a94","volumeId":"f2183b51-d65f-455d-964a-bc824511da9d","resourceDeletions":[{"kind":"unknown","resourceId":"7fdb87b6-28e6-4d2d-8e3f-65e4ef580a94","status":"deleted"}]}"#;
        assert!(serde_json::from_slice::<ProviderResponse>(unknown_kind).is_err());

        let unknown_status = br#"{"type":"deleted","receiptId":"90f0fd93-10d3-40de-946d-93021d42d5ce","remoteEnvironmentId":"7fdb87b6-28e6-4d2d-8e3f-65e4ef580a94","volumeId":"f2183b51-d65f-455d-964a-bc824511da9d","resourceDeletions":[{"kind":"snapshot","status":"unknown"}]}"#;
        assert!(serde_json::from_slice::<ProviderResponse>(unknown_status).is_err());
    }
}
