//! V0.8 local-environment providers.
//!
//! Every provider implements the same lifecycle surface, but advertises its
//! real capabilities.  `Unavailable` is a first-class result: callers must not
//! turn it into a successful no-op.  External programs are launched directly
//! with argument arrays; no provider invokes a command shell or accepts an
//! arbitrary guest command.

use std::{
    ffi::OsString,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration as StdDuration, Instant},
};

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[cfg(target_os = "windows")]
use crate::domain::{trusted_windows_system_tool, WindowsSystemTool};
#[cfg(target_os = "windows")]
use std::io::{Seek, SeekFrom};
#[cfg(target_os = "windows")]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(target_os = "windows")]
use windows_sys::Win32::Security::Cryptography::{
    BCryptCloseAlgorithmProvider, BCryptCreateHash, BCryptDestroyHash, BCryptFinishHash,
    BCryptHashData, BCryptOpenAlgorithmProvider, BCRYPT_ALG_HANDLE, BCRYPT_HASH_HANDLE,
    BCRYPT_SHA256_ALGORITHM,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

pub const ENVIRONMENT_CONTRACT_VERSION: u32 = 1;
pub const WSL_GUEST_AGENT_PATH: &str = "/opt/verisilo/bin/verisilo-guest-agent";
pub const WSL_GUEST_AGENT_VERSION: &str = "0.8.0";
const MAX_PROVIDER_STDOUT_BYTES: usize = 64 * 1024;
const MAX_PROVIDER_STDERR_BYTES: usize = 16 * 1024;
const MAX_BINDING_BYTES: usize = 8 * 1024;
const PROVIDER_TIMEOUT: StdDuration = StdDuration::from_secs(30);
const HYPERV_CREATE_TIMEOUT: StdDuration = StdDuration::from_secs(5 * 60);
const SANDBOX_LAUNCH_CONFIRMATION: StdDuration = StdDuration::from_secs(2);
const WSL_GUEST_PROFILE_ROOT: &str = "/var/lib/verisilo/silos";

#[derive(Debug, Clone, Copy)]
enum ProviderSystemTool {
    PowerShell,
    Wsl,
    WindowsSandbox,
}

type ProviderSystemToolResolver =
    fn(ProviderSystemTool) -> Result<PathBuf, EnvironmentBackendError>;

fn provider_system_tool(tool: ProviderSystemTool) -> Result<PathBuf, EnvironmentBackendError> {
    #[cfg(target_os = "windows")]
    {
        let tool = match tool {
            ProviderSystemTool::PowerShell => WindowsSystemTool::PowerShell,
            ProviderSystemTool::Wsl => WindowsSystemTool::Wsl,
            ProviderSystemTool::WindowsSandbox => WindowsSystemTool::WindowsSandbox,
        };
        trusted_windows_system_tool(tool).map_err(|error| {
            EnvironmentBackendError::InvalidRequest(format!(
                "Trusted Windows provider executable is unavailable: {error}"
            ))
        })
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(PathBuf::from(match tool {
            ProviderSystemTool::PowerShell => "powershell.exe",
            ProviderSystemTool::Wsl => "wsl.exe",
            ProviderSystemTool::WindowsSandbox => "WindowsSandbox.exe",
        }))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum EnvironmentBackendId {
    WslChromium,
    WindowsSandbox,
    HyperV,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum EnvironmentOperation {
    Create,
    Start,
    Stop,
    Pause,
    Snapshot,
    Destroy,
    ConfigureNetwork,
    Health,
    Logs,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "availability", rename_all = "snake_case")]
pub enum OperationAvailability {
    Available,
    Unavailable { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentCapability {
    pub operation: EnvironmentOperation,
    pub availability: OperationAvailability,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrerequisiteState {
    Configured,
    GuestObserved,
    Verified,
    Missing,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentPrerequisite {
    pub id: String,
    pub state: PrerequisiteState,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentBackendStatus {
    pub contract_version: u32,
    pub backend: EnvironmentBackendId,
    pub capabilities: Vec<EnvironmentCapability>,
    pub prerequisites: Vec<EnvironmentPrerequisite>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentActionState {
    Configured,
    Started,
    Stopped,
    Destroyed,
    Healthy,
    LogsExported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentActionReceipt {
    pub backend: EnvironmentBackendId,
    pub operation: EnvironmentOperation,
    pub environment_id: Uuid,
    pub state: EnvironmentActionState,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_path: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GuestEvidenceSource {
    GuestAgent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GuestEvidenceState {
    NotRequested,
    Configured,
    Verified,
    Failed,
    Unavailable,
}

/// Network evidence is accepted only from the fixed guest agent protocol.
/// There is deliberately no `Host` source variant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GuestNetworkEvidence {
    pub schema_version: u32,
    pub evidence_id: Uuid,
    pub environment_id: Uuid,
    pub source: GuestEvidenceSource,
    pub runtime_id: Uuid,
    pub profile_path: String,
    pub proxy_port: Option<u16>,
    pub agent_sha256: String,
    pub proxy: GuestEvidenceState,
    pub exit: GuestEvidenceState,
    pub proxy_dns: GuestEvidenceState,
    pub guest_resolver: GuestEvidenceState,
    pub observed_at: String,
    pub valid_until: String,
}

impl GuestNetworkEvidence {
    fn is_fresh(&self) -> bool {
        chrono::DateTime::parse_from_rfc3339(&self.observed_at)
            .ok()
            .zip(chrono::DateTime::parse_from_rfc3339(&self.valid_until).ok())
            .map(|(observed, valid_until)| {
                (
                    observed.with_timezone(&Utc),
                    valid_until.with_timezone(&Utc),
                )
            })
            .is_some_and(|(observed, valid_until)| {
                let now = Utc::now();
                observed <= now + Duration::seconds(30)
                    && observed >= now - Duration::minutes(2)
                    && valid_until >= now
                    && valid_until >= observed
                    && valid_until <= observed + Duration::minutes(2)
            })
    }

    fn validates_required_proxy(
        &self,
        environment_id: Uuid,
        runtime_id: Uuid,
        profile_path: &str,
        proxy_port: u16,
        agent_sha256: &str,
    ) -> bool {
        self.schema_version == ENVIRONMENT_CONTRACT_VERSION
            && self.evidence_id != Uuid::nil()
            && self.environment_id == environment_id
            && self.source == GuestEvidenceSource::GuestAgent
            && self.runtime_id == runtime_id
            && self.runtime_id != Uuid::nil()
            && self.profile_path == profile_path
            && self.proxy_port == Some(proxy_port)
            && self.agent_sha256 == agent_sha256
            && self.proxy == GuestEvidenceState::Verified
            && self.exit == GuestEvidenceState::Verified
            && self.proxy_dns == GuestEvidenceState::Verified
            && self.guest_resolver == GuestEvidenceState::Unavailable
            && self.is_fresh()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "mode",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum EnvironmentNetworkProfile {
    Direct,
    FixedProxy {
        proxy_required: bool,
        scheme: ProxyScheme,
        host: String,
        port: u16,
    },
}

impl EnvironmentNetworkProfile {
    pub fn proxy_required(&self) -> bool {
        matches!(
            self,
            Self::FixedProxy {
                proxy_required: true,
                ..
            }
        )
    }

    fn is_fixed_proxy(&self) -> bool {
        matches!(self, Self::FixedProxy { .. })
    }

    fn validate(&self) -> Result<(), EnvironmentBackendError> {
        if let Self::FixedProxy {
            host,
            port,
            scheme: _,
            proxy_required: _,
        } = self
        {
            if host.trim().is_empty()
                || host.len() > 253
                || !host.chars().all(|character| {
                    character.is_ascii_alphanumeric() || ".:-[]".contains(character)
                })
            {
                return Err(EnvironmentBackendError::InvalidRequest(
                    "Proxy host contains unsupported characters.".to_owned(),
                ));
            }
            if *port == 0 {
                return Err(EnvironmentBackendError::InvalidRequest(
                    "Proxy port must be between 1 and 65535.".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProxyScheme {
    Http,
    Https,
    Socks5,
}

#[derive(Debug, Clone)]
pub struct CreateEnvironmentRequest {
    pub environment_id: Uuid,
    pub network: EnvironmentNetworkProfile,
}

#[derive(Debug, Clone)]
pub struct EnvironmentRequest {
    pub environment_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct DestroyEnvironmentRequest {
    pub environment_id: Uuid,
    pub confirm_destroy: bool,
}

#[derive(Debug, Clone)]
pub struct ConfigureNetworkRequest {
    pub environment_id: Uuid,
    pub runtime_id: Uuid,
    pub network: EnvironmentNetworkProfile,
}

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub stdin: Option<Vec<u8>>,
    pub completion: CommandCompletion,
    pub timeout: StdDuration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCompletion {
    WaitForExit,
    ConfirmSpawned,
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub trait ProcessRunner {
    fn run(&mut self, spec: &CommandSpec) -> Result<CommandOutput, EnvironmentBackendError>;
}

#[derive(Debug, Default)]
pub struct SystemProcessRunner;

impl ProcessRunner for SystemProcessRunner {
    fn run(&mut self, spec: &CommandSpec) -> Result<CommandOutput, EnvironmentBackendError> {
        let mut command = Command::new(&spec.program);
        command.args(&spec.args);
        #[cfg(target_os = "windows")]
        crate::domain::hide_windows_console(&mut command);
        if spec.stdin.is_some() {
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }
        if spec.completion == CommandCompletion::WaitForExit {
            command.stdout(Stdio::piped()).stderr(Stdio::piped());
        } else {
            command.stdout(Stdio::null()).stderr(Stdio::null());
        }
        let mut child = command.spawn().map_err(EnvironmentBackendError::Io)?;
        if let Some(input) = &spec.stdin {
            let Some(mut stdin) = child.stdin.take() else {
                let _ = child.kill();
                let _ = child.wait();
                return Err(EnvironmentBackendError::Protocol(
                    "Fixed provider process did not expose stdin.".to_owned(),
                ));
            };
            if let Err(error) = stdin.write_all(input) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(EnvironmentBackendError::Io(error));
            }
        }
        if spec.completion == CommandCompletion::ConfirmSpawned {
            if spec.stdin.is_some() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(EnvironmentBackendError::InvalidRequest(
                    "Detached fixed-provider launches cannot accept stdin.".to_owned(),
                ));
            }
            let deadline = Instant::now() + SANDBOX_LAUNCH_CONFIRMATION;
            loop {
                if let Some(status) = child.try_wait().map_err(EnvironmentBackendError::Io)? {
                    return Ok(CommandOutput {
                        success: status.success(),
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                    });
                }
                if Instant::now() >= deadline {
                    return Ok(CommandOutput {
                        success: true,
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                    });
                }
                thread::sleep(StdDuration::from_millis(50));
            }
        }
        let stdout = child.stdout.take().ok_or_else(|| {
            EnvironmentBackendError::Protocol(
                "Fixed provider process did not expose stdout.".to_owned(),
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            EnvironmentBackendError::Protocol(
                "Fixed provider process did not expose stderr.".to_owned(),
            )
        })?;
        let stdout_reader =
            thread::spawn(move || read_bounded_output(stdout, MAX_PROVIDER_STDOUT_BYTES));
        let stderr_reader =
            thread::spawn(move || read_bounded_output(stderr, MAX_PROVIDER_STDERR_BYTES));
        let deadline = Instant::now() + spec.timeout;
        let status = loop {
            if let Some(status) = child.try_wait().map_err(EnvironmentBackendError::Io)? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(EnvironmentBackendError::Process(format!(
                    "Fixed provider process exceeded its {}-second timeout.",
                    spec.timeout.as_secs()
                )));
            }
            thread::sleep(StdDuration::from_millis(50));
        };
        let stdout = stdout_reader.join().map_err(|_| {
            EnvironmentBackendError::Protocol(
                "Provider stdout reader terminated unexpectedly.".to_owned(),
            )
        })??;
        let stderr = stderr_reader.join().map_err(|_| {
            EnvironmentBackendError::Protocol(
                "Provider stderr reader terminated unexpectedly.".to_owned(),
            )
        })??;
        Ok(CommandOutput {
            success: status.success(),
            stdout,
            stderr,
        })
    }
}

fn read_bounded_output(
    mut reader: impl Read,
    limit: usize,
) -> Result<Vec<u8>, EnvironmentBackendError> {
    let mut retained = Vec::with_capacity(limit.min(8 * 1024));
    let mut buffer = [0_u8; 8 * 1024];
    let mut overflow = false;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(EnvironmentBackendError::Io)?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(retained.len());
        retained.extend_from_slice(&buffer[..read.min(remaining)]);
        overflow |= read > remaining;
    }
    if overflow {
        return Err(EnvironmentBackendError::Protocol(format!(
            "Fixed provider output exceeded the {limit}-byte limit."
        )));
    }
    Ok(retained)
}

#[derive(Debug, Error)]
pub enum EnvironmentBackendError {
    #[error("{operation:?} is unavailable for {backend:?}: {reason}")]
    Unavailable {
        backend: EnvironmentBackendId,
        operation: EnvironmentOperation,
        reason: String,
    },
    #[error("Invalid environment request: {0}")]
    InvalidRequest(String),
    #[error("Provider protocol error: {0}")]
    Protocol(String),
    #[error("Provider process failed: {0}")]
    Process(String),
    #[error("Environment filesystem error: {0}")]
    Io(#[source] std::io::Error),
    #[error("Environment JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub trait EnvironmentBackend {
    fn backend_id(&self) -> EnvironmentBackendId;
    fn status(&self) -> EnvironmentBackendStatus;
    fn create(
        &mut self,
        request: CreateEnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError>;
    fn start(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError>;
    fn stop(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError>;
    fn pause(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError>;
    fn snapshot(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError>;
    fn destroy(
        &mut self,
        request: DestroyEnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError>;
    fn configure_network(
        &mut self,
        request: ConfigureNetworkRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError>;
    fn health(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError>;
    fn logs(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError>;
}

fn unavailable(
    backend: EnvironmentBackendId,
    operation: EnvironmentOperation,
    reason: impl Into<String>,
) -> EnvironmentBackendError {
    EnvironmentBackendError::Unavailable {
        backend,
        operation,
        reason: reason.into(),
    }
}

fn available(operation: EnvironmentOperation) -> EnvironmentCapability {
    EnvironmentCapability {
        operation,
        availability: OperationAvailability::Available,
    }
}

fn unsupported(operation: EnvironmentOperation, reason: &str) -> EnvironmentCapability {
    EnvironmentCapability {
        operation,
        availability: OperationAvailability::Unavailable {
            reason: reason.to_owned(),
        },
    }
}

fn receipt(
    backend: EnvironmentBackendId,
    operation: EnvironmentOperation,
    environment_id: Uuid,
    state: EnvironmentActionState,
    message: impl Into<String>,
) -> EnvironmentActionReceipt {
    EnvironmentActionReceipt {
        backend,
        operation,
        environment_id,
        state,
        message: message.into(),
        artifact_path: None,
    }
}

fn process_failure(output: &CommandOutput) -> EnvironmentBackendError {
    let stderr = String::from_utf8_lossy(&output.stderr);
    EnvironmentBackendError::Process(if stderr.trim().is_empty() {
        "The fixed provider process returned a non-zero status.".to_owned()
    } else {
        stderr.trim().chars().take(1_024).collect()
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EnvironmentBinding {
    schema_version: u32,
    environment_id: Uuid,
    backend: EnvironmentBackendId,
    provider_key: String,
}

impl EnvironmentBinding {
    fn new(environment_id: Uuid, backend: EnvironmentBackendId, provider_key: String) -> Self {
        Self {
            schema_version: ENVIRONMENT_CONTRACT_VERSION,
            environment_id,
            backend,
            provider_key,
        }
    }
}

fn environment_directory(state_root: &Path, environment_id: Uuid) -> PathBuf {
    state_root.join(environment_id.to_string())
}

fn ensure_state_directory(path: &Path) -> Result<(), EnvironmentBackendError> {
    fs::create_dir_all(path).map_err(EnvironmentBackendError::Io)?;
    reject_existing_reparse_components(path, "Environment state path")?;
    let metadata = fs::symlink_metadata(path).map_err(EnvironmentBackendError::Io)?;
    if metadata_is_reparse_point(&metadata) || !metadata.is_dir() {
        return Err(EnvironmentBackendError::InvalidRequest(
            "Environment state directory must be a real directory, not a link or reparse point."
                .to_owned(),
        ));
    }
    Ok(())
}

pub(crate) fn metadata_is_reparse_point(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(target_os = "windows"))]
    false
}

fn reject_existing_reparse_components(
    path: &Path,
    label: &str,
) -> Result<(), EnvironmentBackendError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        // A Windows verbatim path starts with a non-openable prefix such as
        // `\\?\C:`. The prefix only becomes a filesystem path after the root
        // component is appended (`\\?\C:\`). Tauri returns resource paths in
        // this form, so querying the bare prefix makes desktop startup fail
        // with ERROR_INVALID_FUNCTION before any provider is used.
        #[cfg(target_os = "windows")]
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata_is_reparse_point(&metadata) => {
                return Err(EnvironmentBackendError::InvalidRequest(format!(
                    "{label} must not contain a symbolic link or reparse-point component."
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(EnvironmentBackendError::Io(error)),
        }
    }
    Ok(())
}

fn read_bounded_regular_file(
    path: &Path,
    limit: usize,
) -> Result<Vec<u8>, EnvironmentBackendError> {
    let metadata = fs::symlink_metadata(path).map_err(EnvironmentBackendError::Io)?;
    if metadata_is_reparse_point(&metadata) || !metadata.is_file() || metadata.len() > limit as u64
    {
        return Err(EnvironmentBackendError::InvalidRequest(format!(
            "Environment state file must be a regular non-reparse file no larger than {limit} bytes."
        )));
    }
    fs::read(path).map_err(EnvironmentBackendError::Io)
}

fn binding_path(state_root: &Path, environment_id: Uuid) -> PathBuf {
    environment_directory(state_root, environment_id).join("binding.json")
}

fn ensure_binding(
    state_root: &Path,
    expected: &EnvironmentBinding,
) -> Result<PathBuf, EnvironmentBackendError> {
    let directory = environment_directory(state_root, expected.environment_id);
    ensure_state_directory(&directory)?;
    let path = directory.join("binding.json");
    let bytes = serde_json::to_vec_pretty(expected)?;
    match write_new_file(&path, &bytes) {
        Ok(()) => Ok(path),
        Err(EnvironmentBackendError::Io(error))
            if error.kind() == std::io::ErrorKind::AlreadyExists =>
        {
            require_binding(state_root, expected)?;
            Ok(path)
        }
        Err(error) => Err(error),
    }
}

fn require_binding(
    state_root: &Path,
    expected: &EnvironmentBinding,
) -> Result<(), EnvironmentBackendError> {
    let bytes = read_bounded_regular_file(
        &binding_path(state_root, expected.environment_id),
        MAX_BINDING_BYTES,
    )?;
    let actual: EnvironmentBinding = serde_json::from_slice(&bytes)?;
    if actual != *expected || actual.schema_version != ENVIRONMENT_CONTRACT_VERSION {
        return Err(EnvironmentBackendError::Protocol(
            "Persistent Silo binding does not match this backend, provider, and UUID.".to_owned(),
        ));
    }
    Ok(())
}

fn write_idempotent_file(path: &Path, bytes: &[u8]) -> Result<(), EnvironmentBackendError> {
    match write_new_file(path, bytes) {
        Ok(()) => Ok(()),
        Err(EnvironmentBackendError::Io(error))
            if error.kind() == std::io::ErrorKind::AlreadyExists =>
        {
            if read_bounded_regular_file(path, bytes.len().max(1))? == bytes {
                Ok(())
            } else {
                Err(EnvironmentBackendError::Protocol(
                    "Existing environment artifact differs from the deterministic configuration."
                        .to_owned(),
                ))
            }
        }
        Err(error) => Err(error),
    }
}

fn remove_regular_file_if_exists(path: &Path) -> Result<(), EnvironmentBackendError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata_is_reparse_point(&metadata) || !metadata.is_file() => {
            Err(EnvironmentBackendError::Protocol(
                "Refusing to remove an environment artifact that is not a regular file.".to_owned(),
            ))
        }
        Ok(_) => fs::remove_file(path).map_err(EnvironmentBackendError::Io),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(EnvironmentBackendError::Io(error)),
    }
}

fn remove_empty_directory_if_exists(path: &Path) -> Result<(), EnvironmentBackendError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata_is_reparse_point(&metadata) || !metadata.is_dir() => {
            Err(EnvironmentBackendError::Protocol(
                "Refusing to remove an environment state path that is not a real directory."
                    .to_owned(),
            ))
        }
        Ok(_) => fs::remove_dir(path).map_err(EnvironmentBackendError::Io),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(EnvironmentBackendError::Io(error)),
    }
}

fn require_only_directory_entries(
    path: &Path,
    allowed: &[&str],
) -> Result<(), EnvironmentBackendError> {
    for entry in fs::read_dir(path).map_err(EnvironmentBackendError::Io)? {
        let entry = entry.map_err(EnvironmentBackendError::Io)?;
        let name = entry.file_name().into_string().map_err(|_| {
            EnvironmentBackendError::Protocol(
                "Environment state contains a non-Unicode artifact name.".to_owned(),
            )
        })?;
        if !allowed.contains(&name.as_str()) {
            return Err(EnvironmentBackendError::Protocol(format!(
                "Unexpected environment artifact remains: {name}."
            )));
        }
    }
    Ok(())
}

/// Returns every durable local provider namespace that still owns state for a
/// Silo. Any real directory, file, or link at the UUID-derived path counts as
/// an artifact so callers fail closed on partial creates and interrupted
/// cleanup instead of orphaning a guest by deleting its Vault record.
pub fn local_environment_artifacts(
    environment_root: &Path,
    environment_id: Uuid,
) -> Result<Vec<EnvironmentBackendId>, EnvironmentBackendError> {
    let mut artifacts = Vec::new();
    for (directory, backend) in [
        ("wsl", EnvironmentBackendId::WslChromium),
        ("sandbox", EnvironmentBackendId::WindowsSandbox),
        ("hyperv", EnvironmentBackendId::HyperV),
    ] {
        let path = environment_root
            .join(directory)
            .join(environment_id.to_string());
        match fs::symlink_metadata(path) {
            Ok(_) => artifacts.push(backend),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(EnvironmentBackendError::Io(error)),
        }
    }
    Ok(artifacts)
}

/// Reads a UUID-derived provider binding without trusting a caller-selected
/// provider handle. This is used only to clean up environment state created by
/// older Vault schemas that did not persist a Silo run location.
pub fn local_environment_binding_provider(
    environment_root: &Path,
    environment_id: Uuid,
    backend: EnvironmentBackendId,
) -> Result<Option<String>, EnvironmentBackendError> {
    require_absolute_clean_path(environment_root, "Local environment binding root")?;
    let provider_directory = match backend {
        EnvironmentBackendId::WslChromium => "wsl",
        EnvironmentBackendId::WindowsSandbox => "sandbox",
        EnvironmentBackendId::HyperV => "hyperv",
    };
    let state_root = environment_root.join(provider_directory);
    let environment_path = environment_directory(&state_root, environment_id);
    match fs::symlink_metadata(&environment_path) {
        Ok(metadata) if !metadata_is_reparse_point(&metadata) && metadata.is_dir() => {}
        Ok(_) => {
            return Err(EnvironmentBackendError::Protocol(
                "The legacy environment owner path is not a real directory.".to_owned(),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(EnvironmentBackendError::Io(error)),
    }

    let bytes = read_bounded_regular_file(
        &binding_path(&state_root, environment_id),
        MAX_BINDING_BYTES,
    )?;
    let actual: EnvironmentBinding = serde_json::from_slice(&bytes)?;
    if actual.schema_version != ENVIRONMENT_CONTRACT_VERSION
        || actual.environment_id != environment_id
        || actual.backend != backend
    {
        return Err(EnvironmentBackendError::Protocol(
            "The legacy environment binding does not match its UUID and provider namespace."
                .to_owned(),
        ));
    }
    match backend {
        EnvironmentBackendId::WslChromium => validate_distribution_name(&actual.provider_key)?,
        EnvironmentBackendId::WindowsSandbox
            if actual.provider_key != "windows-sandbox-v0.8-ephemeral" =>
        {
            return Err(EnvironmentBackendError::Protocol(
                "The legacy Windows Sandbox binding uses an unknown provider identity.".to_owned(),
            ));
        }
        EnvironmentBackendId::HyperV
            if actual.provider_key != format!("VeriSilo-{environment_id}") =>
        {
            return Err(EnvironmentBackendError::Protocol(
                "The legacy Hyper-V binding uses an unknown provider identity.".to_owned(),
            ));
        }
        _ => {}
    }
    require_binding(
        &state_root,
        &EnvironmentBinding::new(environment_id, backend, actual.provider_key.clone()),
    )?;
    Ok(Some(actual.provider_key))
}

fn local_environment_provider_directories() -> [(&'static str, EnvironmentBackendId); 3] {
    [
        ("wsl", EnvironmentBackendId::WslChromium),
        ("sandbox", EnvironmentBackendId::WindowsSandbox),
        ("hyperv", EnvironmentBackendId::HyperV),
    ]
}

/// Inventories every durable local provider namespace without following links.
/// Missing roots and real empty provider directories are clean. Every child of
/// a provider directory counts as an artifact, regardless of its name or type,
/// so partial writes and legacy/unknown layouts cannot be orphaned by restore.
pub fn local_environment_artifact_inventory(
    environment_root: &Path,
) -> Result<Vec<EnvironmentBackendId>, EnvironmentBackendError> {
    require_absolute_clean_path(environment_root, "Local environment inventory root")?;
    let root_metadata = match fs::symlink_metadata(environment_root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(EnvironmentBackendError::Io(error)),
    };
    if metadata_is_reparse_point(&root_metadata) || !root_metadata.is_dir() {
        return Err(EnvironmentBackendError::InvalidRequest(
            "Local environment inventory root must be a real directory, not a file, link, or reparse point."
                .to_owned(),
        ));
    }

    let providers = local_environment_provider_directories();
    for entry in fs::read_dir(environment_root).map_err(EnvironmentBackendError::Io)? {
        let entry = entry.map_err(EnvironmentBackendError::Io)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(EnvironmentBackendError::InvalidRequest(
                "Local environment inventory contains an unexpected top-level entry; Vault restore is blocked."
                    .to_owned(),
            ));
        };
        if !providers.iter().any(|(directory, _)| *directory == name) {
            return Err(EnvironmentBackendError::InvalidRequest(
                "Local environment inventory contains an unexpected top-level entry; Vault restore is blocked."
                    .to_owned(),
            ));
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(EnvironmentBackendError::Io)?;
        if metadata_is_reparse_point(&metadata) || !metadata.is_dir() {
            return Err(EnvironmentBackendError::InvalidRequest(
                "Local environment provider namespaces must be real directories, not files, links, or reparse points."
                    .to_owned(),
            ));
        }
    }

    let mut artifacts = Vec::new();
    for (directory, backend) in providers {
        let provider_root = environment_root.join(directory);
        let metadata = match fs::symlink_metadata(&provider_root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(EnvironmentBackendError::Io(error)),
        };
        if metadata_is_reparse_point(&metadata) || !metadata.is_dir() {
            return Err(EnvironmentBackendError::InvalidRequest(
                "Local environment provider namespaces must be real directories, not files, links, or reparse points."
                    .to_owned(),
            ));
        }

        let mut has_artifacts = false;
        for entry in fs::read_dir(&provider_root).map_err(EnvironmentBackendError::Io)? {
            let entry = entry.map_err(EnvironmentBackendError::Io)?;
            fs::symlink_metadata(entry.path()).map_err(EnvironmentBackendError::Io)?;
            has_artifacts = true;
        }
        if has_artifacts {
            artifacts.push(backend);
        }
    }
    Ok(artifacts)
}

/// Fails closed before Vault restore whenever any local provider artifact or
/// unexpected namespace remains on disk.
pub fn ensure_no_local_environment_artifacts_for_restore(
    environment_root: &Path,
) -> Result<(), EnvironmentBackendError> {
    let artifacts = local_environment_artifact_inventory(environment_root)?;
    if artifacts.is_empty() {
        return Ok(());
    }
    Err(EnvironmentBackendError::InvalidRequest(format!(
        "Destroy or detach every local environment before restoring the Vault; durable artifacts remain for {artifacts:?}."
    )))
}

fn invalidate_binding(
    state_root: &Path,
    expected: &EnvironmentBinding,
) -> Result<(), EnvironmentBackendError> {
    let directory = environment_directory(state_root, expected.environment_id);
    ensure_state_directory(&directory)?;
    let path = binding_path(state_root, expected.environment_id);
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            require_binding(state_root, expected)?;
            remove_regular_file_if_exists(&path)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(EnvironmentBackendError::Io(error)),
    }
}

// ---------------------------------------------------------------------------
// WSL Chromium

#[derive(Debug, Clone)]
pub struct WslChromiumPrerequisites {
    pub supported_platform: bool,
    pub wsl_available: bool,
    pub discovered_distributions: Vec<String>,
    pub guest_agent_distributions: Vec<String>,
    pub gui_distributions: Vec<String>,
    pub expected_agent_sha256: String,
}

pub struct WslChromiumBackend<R: ProcessRunner> {
    distribution: String,
    prerequisites: WslChromiumPrerequisites,
    state_root: PathBuf,
    runner: R,
}

struct WslAgentEnvelope<'a> {
    schema_version: u32,
    environment_id: Uuid,
    expected_environment_id: Uuid,
    source: GuestEvidenceSource,
    agent_version: &'a str,
    agent_sha256: &'a str,
    observed_at: &'a str,
}

impl<R: ProcessRunner> WslChromiumBackend<R> {
    pub fn new(
        distribution: String,
        prerequisites: WslChromiumPrerequisites,
        state_root: PathBuf,
        runner: R,
    ) -> Result<Self, EnvironmentBackendError> {
        validate_distribution_name(&distribution)?;
        require_absolute_clean_path(&state_root, "WSL state root")?;
        if !prerequisites
            .discovered_distributions
            .iter()
            .any(|candidate| candidate == &distribution)
        {
            return Err(EnvironmentBackendError::InvalidRequest(
                "WSL Chromium may target only a distribution returned by discovery.".to_owned(),
            ));
        }
        Ok(Self {
            distribution,
            prerequisites,
            state_root,
            runner,
        })
    }

    fn binding(&self, environment_id: Uuid) -> EnvironmentBinding {
        EnvironmentBinding::new(
            environment_id,
            EnvironmentBackendId::WslChromium,
            self.distribution.clone(),
        )
    }

    fn guest_profile_path(environment_id: Uuid) -> String {
        format!("{WSL_GUEST_PROFILE_ROOT}/{environment_id}/chromium-profile")
    }

    fn validate_guest_network_profile(
        network: &EnvironmentNetworkProfile,
    ) -> Result<(), EnvironmentBackendError> {
        if let EnvironmentNetworkProfile::FixedProxy {
            scheme, host, port, ..
        } = network
        {
            if *scheme != ProxyScheme::Socks5 || host != "127.0.0.1" || *port == 0 {
                return Err(EnvironmentBackendError::InvalidRequest(
                    "WSL guest evidence accepts only a credential-free loopback SOCKS5 endpoint; HTTP(S), hostnames, and non-loopback proxy hosts are unavailable."
                        .to_owned(),
                ));
            }
        }
        Ok(())
    }

    fn expected_agent_identity_is_valid(&self) -> bool {
        self.prerequisites.expected_agent_sha256.len() == 64
            && self
                .prerequisites
                .expected_agent_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }

    pub fn command_spec(
        &self,
        subcommand: &str,
        environment_id: Uuid,
        stdin: Option<Vec<u8>>,
    ) -> Result<CommandSpec, EnvironmentBackendError> {
        const ALLOWED: &[&str] = &[
            "identity",
            "configure-network",
            "start",
            "stop",
            "detach",
            "health",
            "logs",
        ];
        if !ALLOWED.contains(&subcommand) {
            return Err(EnvironmentBackendError::InvalidRequest(
                "WSL guest-agent subcommand is not in the fixed allowlist.".to_owned(),
            ));
        }
        if !self
            .prerequisites
            .discovered_distributions
            .iter()
            .any(|candidate| candidate == &self.distribution)
        {
            return Err(EnvironmentBackendError::InvalidRequest(
                "The selected WSL distribution is no longer in the discovery snapshot.".to_owned(),
            ));
        }
        Ok(CommandSpec {
            program: provider_system_tool(ProviderSystemTool::Wsl)?,
            args: [
                "-d".into(),
                self.distribution.clone().into(),
                "--user".into(),
                "root".into(),
                "--exec".into(),
                WSL_GUEST_AGENT_PATH.into(),
                subcommand.into(),
                "--silo-id".into(),
                environment_id.to_string().into(),
            ]
            .into(),
            stdin,
            completion: CommandCompletion::WaitForExit,
            timeout: PROVIDER_TIMEOUT,
        })
    }

    fn agent_ready(&self) -> Result<(), EnvironmentBackendError> {
        if !self.prerequisites.supported_platform {
            return Err(unavailable(
                EnvironmentBackendId::WslChromium,
                EnvironmentOperation::Start,
                "WSL is available only on Windows hosts.",
            ));
        }
        if !self.prerequisites.wsl_available {
            return Err(unavailable(
                EnvironmentBackendId::WslChromium,
                EnvironmentOperation::Start,
                "wsl.exe is unavailable or WSL is not enabled.",
            ));
        }
        if !self
            .prerequisites
            .guest_agent_distributions
            .contains(&self.distribution)
            || !self.expected_agent_identity_is_valid()
        {
            return Err(unavailable(
                EnvironmentBackendId::WslChromium,
                EnvironmentOperation::Start,
                format!("Guest agent is missing at {WSL_GUEST_AGENT_PATH}."),
            ));
        }
        Ok(())
    }

    fn run_agent(
        &mut self,
        operation: EnvironmentOperation,
        subcommand: &str,
        environment_id: Uuid,
        stdin: Option<Vec<u8>>,
    ) -> Result<CommandOutput, EnvironmentBackendError> {
        self.agent_ready().map_err(|error| match error {
            EnvironmentBackendError::Unavailable { reason, .. } => {
                unavailable(EnvironmentBackendId::WslChromium, operation, reason)
            }
            other => other,
        })?;
        let spec = self.command_spec(subcommand, environment_id, stdin)?;
        let output = self.runner.run(&spec)?;
        if !output.success {
            return Err(process_failure(&output));
        }
        Ok(output)
    }

    fn validate_agent_envelope(
        &self,
        envelope: WslAgentEnvelope<'_>,
    ) -> Result<(), EnvironmentBackendError> {
        let source_is_guest_agent = envelope.source == GuestEvidenceSource::GuestAgent;
        let fresh = chrono::DateTime::parse_from_rfc3339(envelope.observed_at)
            .ok()
            .map(|observed| observed.with_timezone(&Utc))
            .is_some_and(|observed| {
                let now = Utc::now();
                observed <= now + Duration::seconds(30) && observed >= now - Duration::minutes(2)
            });
        if envelope.schema_version != ENVIRONMENT_CONTRACT_VERSION
            || envelope.environment_id != envelope.expected_environment_id
            || !source_is_guest_agent
            || envelope.agent_version != WSL_GUEST_AGENT_VERSION
            || envelope.agent_sha256 != self.prerequisites.expected_agent_sha256
            || !fresh
        {
            return Err(EnvironmentBackendError::Protocol(
                "WSL guest-agent response failed its UUID, version, hash, source, or freshness binding."
                    .to_owned(),
            ));
        }
        Ok(())
    }

    fn validate_action_response(
        &self,
        bytes: &[u8],
        environment_id: Uuid,
        action: WslAgentAction,
        state: WslAgentState,
    ) -> Result<WslActionResponse, EnvironmentBackendError> {
        let response: WslActionResponse = serde_json::from_slice(bytes)?;
        self.validate_agent_envelope(WslAgentEnvelope {
            schema_version: response.schema_version,
            environment_id: response.environment_id,
            expected_environment_id: environment_id,
            source: response.source,
            agent_version: &response.agent_version,
            agent_sha256: &response.agent_sha256,
            observed_at: &response.observed_at,
        })?;
        if response.action != action || response.state != state {
            return Err(EnvironmentBackendError::Protocol(
                "WSL guest-agent action receipt did not match the fixed operation.".to_owned(),
            ));
        }
        Ok(response)
    }
}

impl<R: ProcessRunner> EnvironmentBackend for WslChromiumBackend<R> {
    fn backend_id(&self) -> EnvironmentBackendId {
        EnvironmentBackendId::WslChromium
    }

    fn status(&self) -> EnvironmentBackendStatus {
        let agent_ready = self
            .prerequisites
            .guest_agent_distributions
            .contains(&self.distribution);
        let gui_ready = self
            .prerequisites
            .gui_distributions
            .contains(&self.distribution);
        let operational = self.prerequisites.supported_platform
            && self.prerequisites.wsl_available
            && agent_ready
            && self.expected_agent_identity_is_valid();
        let availability = |operation| {
            if operational {
                available(operation)
            } else {
                unsupported(
                    operation,
                    "WSL, the discovered distribution, and the fixed guest agent are required.",
                )
            }
        };
        EnvironmentBackendStatus {
            contract_version: ENVIRONMENT_CONTRACT_VERSION,
            backend: self.backend_id(),
            capabilities: vec![
                unsupported(
                    EnvironmentOperation::Create,
                    "V0.8 does not import or create WSL distributions; select a discovered distribution.",
                ),
                if operational && gui_ready {
                    available(EnvironmentOperation::Start)
                } else {
                    unsupported(
                        EnvironmentOperation::Start,
                        "WSL, the exact fixed guest agent, and WSLg Chromium are required.",
                    )
                },
                availability(EnvironmentOperation::Stop),
                unsupported(
                    EnvironmentOperation::Pause,
                    "Pausing a shared WSL distribution would affect unrelated workloads.",
                ),
                unsupported(
                    EnvironmentOperation::Snapshot,
                    "V0.8 does not snapshot or export user WSL distributions.",
                ),
                availability(EnvironmentOperation::Destroy),
                availability(EnvironmentOperation::ConfigureNetwork),
                availability(EnvironmentOperation::Health),
                availability(EnvironmentOperation::Logs),
            ],
            prerequisites: vec![
                EnvironmentPrerequisite {
                    id: "windows-host".to_owned(),
                    state: if self.prerequisites.supported_platform {
                        PrerequisiteState::Verified
                    } else {
                        PrerequisiteState::Unavailable
                    },
                    detail: "WSL Chromium requires a Windows host.".to_owned(),
                },
                EnvironmentPrerequisite {
                    id: "wsl".to_owned(),
                    state: if self.prerequisites.wsl_available {
                        PrerequisiteState::Verified
                    } else {
                        PrerequisiteState::Missing
                    },
                    detail: "wsl.exe --status must succeed.".to_owned(),
                },
                EnvironmentPrerequisite {
                    id: "discovered-distribution".to_owned(),
                    state: PrerequisiteState::Configured,
                    detail: format!("Selected discovered distribution: {}.", self.distribution),
                },
                EnvironmentPrerequisite {
                    id: "guest-agent".to_owned(),
                    state: if agent_ready && self.expected_agent_identity_is_valid() {
                        PrerequisiteState::GuestObserved
                    } else {
                        PrerequisiteState::Missing
                    },
                    detail: format!(
                        "Required fixed path {WSL_GUEST_AGENT_PATH}, root owner, exact 0755 mode, version {WSL_GUEST_AGENT_VERSION}, build-embedded SHA-256, and the non-login verisilo-browser account."
                    ),
                },
                EnvironmentPrerequisite {
                    id: "guest-network-evidence".to_owned(),
                    state: PrerequisiteState::Unknown,
                    detail: "No network state is promoted by capability discovery: configure-network must obtain a fresh runtime-bound guest observation, and required mode additionally verifies proxy DNS plus exit."
                        .to_owned(),
                },
                EnvironmentPrerequisite {
                    id: "linux-gui".to_owned(),
                    state: if gui_ready {
                        PrerequisiteState::Verified
                    } else {
                        PrerequisiteState::Missing
                    },
                    detail: "A working WSLg/GUI path is required before Chromium can be shown."
                        .to_owned(),
                },
                EnvironmentPrerequisite {
                    id: "concurrent-multi-silo".to_owned(),
                    state: PrerequisiteState::Unavailable,
                    detail: "V0.8 binds one persistent WSL Chromium Silo per distribution and also enforces a global active-Silo process binding."
                        .to_owned(),
                },
                EnvironmentPrerequisite {
                    id: "bundled-mihomo-tun".to_owned(),
                    state: PrerequisiteState::Unavailable,
                    detail: "V0.8 accepts only an explicit fixed proxy endpoint; it does not bundle Mihomo or claim TUN routing."
                        .to_owned(),
                },
            ],
        }
    }

    fn create(
        &mut self,
        _request: CreateEnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        Err(unavailable(
            self.backend_id(),
            EnvironmentOperation::Create,
            "Use an already-discovered WSL distribution; automatic import is not implemented.",
        ))
    }

    fn start(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        if !self
            .prerequisites
            .gui_distributions
            .contains(&self.distribution)
        {
            return Err(unavailable(
                self.backend_id(),
                EnvironmentOperation::Start,
                "The selected distribution has no verified WSLg/GUI capability.",
            ));
        }
        require_binding(&self.state_root, &self.binding(request.environment_id))?;
        let output = self.run_agent(
            EnvironmentOperation::Start,
            "start",
            request.environment_id,
            None,
        )?;
        self.validate_action_response(
            &output.stdout,
            request.environment_id,
            WslAgentAction::Start,
            WslAgentState::Started,
        )?;
        Ok(receipt(
            self.backend_id(),
            EnvironmentOperation::Start,
            request.environment_id,
            EnvironmentActionState::Started,
            "The fixed WSL guest agent accepted the Chromium start request.",
        ))
    }

    fn stop(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        require_binding(&self.state_root, &self.binding(request.environment_id))?;
        let output = self.run_agent(
            EnvironmentOperation::Stop,
            "stop",
            request.environment_id,
            None,
        )?;
        self.validate_action_response(
            &output.stdout,
            request.environment_id,
            WslAgentAction::Stop,
            WslAgentState::Stopped,
        )?;
        Ok(receipt(
            self.backend_id(),
            EnvironmentOperation::Stop,
            request.environment_id,
            EnvironmentActionState::Stopped,
            "The guest agent stopped only the selected Silo Chromium process.",
        ))
    }

    fn pause(
        &mut self,
        _request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        Err(unavailable(
            self.backend_id(),
            EnvironmentOperation::Pause,
            "A WSL distribution may contain unrelated workloads and is never paused by VeriSilo.",
        ))
    }

    fn snapshot(
        &mut self,
        _request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        Err(unavailable(
            self.backend_id(),
            EnvironmentOperation::Snapshot,
            "WSL export/snapshot is outside the V0.8 provider boundary.",
        ))
    }

    fn destroy(
        &mut self,
        request: DestroyEnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        if !request.confirm_destroy {
            return Err(EnvironmentBackendError::InvalidRequest(
                "WSL detach requires explicit destroy confirmation.".to_owned(),
            ));
        }
        let expected = self.binding(request.environment_id);
        let directory = environment_directory(&self.state_root, request.environment_id);
        match fs::symlink_metadata(&directory) {
            Ok(metadata) if metadata_is_reparse_point(&metadata) || !metadata.is_dir() => {
                return Err(EnvironmentBackendError::Protocol(
                    "WSL host state for this Silo is not a real directory.".to_owned(),
                ));
            }
            Ok(_) => {
                let binding = binding_path(&self.state_root, request.environment_id);
                match fs::symlink_metadata(&binding) {
                    Ok(_) => require_binding(&self.state_root, &expected)?,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(EnvironmentBackendError::Io(error)),
                }
                require_only_directory_entries(&directory, &["binding.json"])?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(EnvironmentBackendError::Io(error)),
        }
        let payload = serde_json::to_vec(&WslDetachRequest {
            schema_version: ENVIRONMENT_CONTRACT_VERSION,
            environment_id: request.environment_id,
            confirm_destroy: true,
        })?;
        let output = self.run_agent(
            EnvironmentOperation::Destroy,
            "detach",
            request.environment_id,
            Some(payload),
        )?;
        self.validate_action_response(
            &output.stdout,
            request.environment_id,
            WslAgentAction::Detach,
            WslAgentState::Destroyed,
        )?;
        remove_regular_file_if_exists(&binding_path(&self.state_root, request.environment_id))?;
        remove_empty_directory_if_exists(&directory)?;
        Ok(receipt(
            self.backend_id(),
            EnvironmentOperation::Destroy,
            request.environment_id,
            EnvironmentActionState::Destroyed,
            "Detached only the UUID-derived Chromium profile, readiness receipt, and host binding; the selected WSL distribution was not unregistered.",
        ))
    }

    fn configure_network(
        &mut self,
        request: ConfigureNetworkRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        request.network.validate()?;
        Self::validate_guest_network_profile(&request.network)?;
        if request.runtime_id == Uuid::nil() {
            return Err(EnvironmentBackendError::InvalidRequest(
                "WSL network configuration requires a non-zero controller runtime UUID.".to_owned(),
            ));
        }
        // A valid reconfiguration attempt is a state transition. Invalidate
        // the prior host authorization before the guest can fail so Start can
        // never reuse a stale DIRECT or proxy-ready receipt.
        invalidate_binding(&self.state_root, &self.binding(request.environment_id))?;
        let payload = serde_json::to_vec(&WslNetworkRequest {
            schema_version: ENVIRONMENT_CONTRACT_VERSION,
            environment_id: request.environment_id,
            runtime_id: request.runtime_id,
            network: request.network.clone(),
        })?;
        if payload.len() > 16 * 1024 {
            return Err(EnvironmentBackendError::InvalidRequest(
                "WSL network request exceeds 16 KiB.".to_owned(),
            ));
        }
        let output = self.run_agent(
            EnvironmentOperation::ConfigureNetwork,
            "configure-network",
            request.environment_id,
            Some(payload),
        )?;
        let response: WslNetworkResponse = serde_json::from_slice(&output.stdout)?;
        self.validate_agent_envelope(WslAgentEnvelope {
            schema_version: response.schema_version,
            environment_id: response.environment_id,
            expected_environment_id: request.environment_id,
            source: response.source,
            agent_version: &response.agent_version,
            agent_sha256: &response.agent_sha256,
            observed_at: &response.observed_at,
        })?;
        if response.evidence.schema_version != ENVIRONMENT_CONTRACT_VERSION
            || response.evidence.evidence_id == Uuid::nil()
            || response.evidence.environment_id != request.environment_id
            || response.evidence.source != GuestEvidenceSource::GuestAgent
            || response.evidence.runtime_id != request.runtime_id
            || response.evidence.profile_path != Self::guest_profile_path(request.environment_id)
            || response.evidence.agent_sha256 != self.prerequisites.expected_agent_sha256
            || response.evidence.guest_resolver != GuestEvidenceState::Unavailable
            || response.evidence.observed_at != response.observed_at
            || !response.evidence.is_fresh()
        {
            return Err(EnvironmentBackendError::Protocol(
                "Guest network evidence did not match the requested Silo, runtime, profile, agent, resolver boundary, or time window."
                    .to_owned(),
            ));
        }
        let expected_proxy_port = match &request.network {
            EnvironmentNetworkProfile::Direct => None,
            EnvironmentNetworkProfile::FixedProxy { port, .. } => Some(*port),
        };
        if response.evidence.proxy_port != expected_proxy_port {
            return Err(EnvironmentBackendError::Protocol(
                "Guest network evidence did not match the requested loopback proxy port."
                    .to_owned(),
            ));
        }
        let evidence_state_matches_request = match &request.network {
            EnvironmentNetworkProfile::Direct => {
                response.evidence.proxy == GuestEvidenceState::NotRequested
                    && response.evidence.exit == GuestEvidenceState::NotRequested
                    && response.evidence.proxy_dns == GuestEvidenceState::NotRequested
            }
            EnvironmentNetworkProfile::FixedProxy { .. } => {
                response.evidence.proxy == GuestEvidenceState::Verified
                    && response.evidence.exit == GuestEvidenceState::Verified
                    && matches!(
                        response.evidence.proxy_dns,
                        GuestEvidenceState::Verified | GuestEvidenceState::Unavailable
                    )
            }
        };
        if !evidence_state_matches_request {
            return Err(EnvironmentBackendError::Protocol(
                "Guest network evidence states did not exactly match the requested DIRECT or fixed-proxy profile."
                    .to_owned(),
            ));
        }
        if request.network.proxy_required()
            && !response.evidence.validates_required_proxy(
                request.environment_id,
                request.runtime_id,
                &Self::guest_profile_path(request.environment_id),
                expected_proxy_port.expect("fixed proxy has a port"),
                &self.prerequisites.expected_agent_sha256,
            )
        {
            return Err(EnvironmentBackendError::Protocol(
                "The required proxy lacks fresh guest-observed proxy DNS and exit evidence bound to this runtime; guest OS resolver evidence remains a separate unavailable claim and no DIRECT fallback is used."
                    .to_owned(),
            ));
        }
        ensure_binding(&self.state_root, &self.binding(request.environment_id))?;
        Ok(receipt(
            self.backend_id(),
            EnvironmentOperation::ConfigureNetwork,
            request.environment_id,
            EnvironmentActionState::Configured,
            "Network configuration and any exit/DNS claims came from the fixed guest agent.",
        ))
    }

    fn health(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        require_binding(&self.state_root, &self.binding(request.environment_id))?;
        let output = self.run_agent(
            EnvironmentOperation::Health,
            "health",
            request.environment_id,
            None,
        )?;
        self.validate_action_response(
            &output.stdout,
            request.environment_id,
            WslAgentAction::Health,
            WslAgentState::Healthy,
        )?;
        Ok(receipt(
            self.backend_id(),
            EnvironmentOperation::Health,
            request.environment_id,
            EnvironmentActionState::Healthy,
            "Health was reported by the fixed WSL guest agent.",
        ))
    }

    fn logs(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        require_binding(&self.state_root, &self.binding(request.environment_id))?;
        let output = self.run_agent(
            EnvironmentOperation::Logs,
            "logs",
            request.environment_id,
            None,
        )?;
        let response = self.validate_action_response(
            &output.stdout,
            request.environment_id,
            WslAgentAction::Logs,
            WslAgentState::LogsExported,
        )?;
        Ok(receipt(
            self.backend_id(),
            EnvironmentOperation::Logs,
            request.environment_id,
            EnvironmentActionState::LogsExported,
            format!(
                "Guest agent returned {} bytes of bounded logs.",
                response.retained_browser_log_bytes.unwrap_or(0)
            ),
        ))
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WslNetworkRequest {
    schema_version: u32,
    environment_id: Uuid,
    runtime_id: Uuid,
    network: EnvironmentNetworkProfile,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WslDetachRequest {
    schema_version: u32,
    environment_id: Uuid,
    confirm_destroy: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WslNetworkResponse {
    schema_version: u32,
    environment_id: Uuid,
    source: GuestEvidenceSource,
    agent_version: String,
    agent_sha256: String,
    observed_at: String,
    evidence: GuestNetworkEvidence,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum WslAgentAction {
    Start,
    Stop,
    Detach,
    Health,
    Logs,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WslAgentState {
    Started,
    Stopped,
    Destroyed,
    Healthy,
    LogsExported,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WslActionResponse {
    schema_version: u32,
    environment_id: Uuid,
    source: GuestEvidenceSource,
    action: WslAgentAction,
    state: WslAgentState,
    agent_version: String,
    agent_sha256: String,
    observed_at: String,
    #[serde(default)]
    retained_browser_log_bytes: Option<u64>,
}

pub(crate) fn validate_distribution_name(value: &str) -> Result<(), EnvironmentBackendError> {
    if value.trim() != value
        || value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b' '))
    {
        return Err(EnvironmentBackendError::InvalidRequest(
            "WSL distribution name must start with ASCII alphanumeric text and contain only the fixed safe character set."
                .to_owned(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Windows Sandbox

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxMappedFolder {
    pub host_folder: PathBuf,
    pub sandbox_folder: String,
    pub read_only: bool,
}

pub fn generate_sandbox_config(
    environment_id: Uuid,
    mapped_folders: &[SandboxMappedFolder],
    networking_enabled: bool,
) -> Result<String, EnvironmentBackendError> {
    let mut mappings = String::new();
    for mapping in mapped_folders {
        if !mapping.read_only {
            return Err(EnvironmentBackendError::InvalidRequest(
                "Windows Sandbox host mappings must be read-only.".to_owned(),
            ));
        }
        let host = mapping.host_folder.to_str().ok_or_else(|| {
            EnvironmentBackendError::InvalidRequest(
                "Windows Sandbox mapping path must be valid Unicode.".to_owned(),
            )
        })?;
        if host.trim().is_empty() || !mapping.host_folder.is_absolute() {
            return Err(EnvironmentBackendError::InvalidRequest(
                "Windows Sandbox mapping path must be absolute.".to_owned(),
            ));
        }
        if mapping.sandbox_folder.trim().is_empty()
            || !mapping.sandbox_folder.starts_with("C:\\")
            || mapping.sandbox_folder.contains("..")
        {
            return Err(EnvironmentBackendError::InvalidRequest(
                "Windows Sandbox destination must be an absolute C:\\ path without traversal."
                    .to_owned(),
            ));
        }
        mappings.push_str("    <MappedFolder>\n");
        mappings.push_str(&format!(
            "      <HostFolder>{}</HostFolder>\n",
            xml_escape(host)
        ));
        mappings.push_str(&format!(
            "      <SandboxFolder>{}</SandboxFolder>\n",
            xml_escape(&mapping.sandbox_folder)
        ));
        mappings.push_str("      <ReadOnly>true</ReadOnly>\n");
        mappings.push_str("    </MappedFolder>\n");
    }
    let mapped_block = if mappings.is_empty() {
        String::new()
    } else {
        format!("  <MappedFolders>\n{mappings}  </MappedFolders>\n")
    };
    Ok(format!(
        "<Configuration>\n  <VGpu>Disable</VGpu>\n  <Networking>{}</Networking>\n  <AudioInput>Disable</AudioInput>\n  <VideoInput>Disable</VideoInput>\n  <PrinterRedirection>Disable</PrinterRedirection>\n  <ClipboardRedirection>Disable</ClipboardRedirection>\n  <ProtectedClient>Enable</ProtectedClient>\n  <MemoryInMB>4096</MemoryInMB>\n{mapped_block}  <LogonCommand>\n    <Command>powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy AllSigned -File C:\\VeriSilo\\Bootstrap\\verisilo-sandbox-bootstrap.ps1 -SiloId {environment_id}</Command>\n  </LogonCommand>\n</Configuration>\n",
        if networking_enabled { "Enable" } else { "Disable" }
    ))
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub struct WindowsSandboxBackend<R: ProcessRunner> {
    supported_platform: bool,
    sandbox_available: bool,
    state_root: PathBuf,
    bootstrap_directory: PathBuf,
    system_tool_resolver: ProviderSystemToolResolver,
    runner: R,
}

impl<R: ProcessRunner> WindowsSandboxBackend<R> {
    pub fn new(
        supported_platform: bool,
        sandbox_available: bool,
        state_root: PathBuf,
        bootstrap_directory: PathBuf,
        runner: R,
    ) -> Result<Self, EnvironmentBackendError> {
        require_absolute_clean_path(&state_root, "Sandbox state root")?;
        require_absolute_clean_path(&bootstrap_directory, "Sandbox bootstrap directory")?;
        Ok(Self {
            supported_platform,
            sandbox_available,
            state_root,
            bootstrap_directory,
            system_tool_resolver: provider_system_tool,
            runner,
        })
    }

    #[cfg(test)]
    fn new_with_system_tool_resolver(
        supported_platform: bool,
        sandbox_available: bool,
        state_root: PathBuf,
        bootstrap_directory: PathBuf,
        system_tool_resolver: ProviderSystemToolResolver,
        runner: R,
    ) -> Result<Self, EnvironmentBackendError> {
        let mut backend = Self::new(
            supported_platform,
            sandbox_available,
            state_root,
            bootstrap_directory,
            runner,
        )?;
        backend.system_tool_resolver = system_tool_resolver;
        Ok(backend)
    }

    fn binding(&self, environment_id: Uuid) -> EnvironmentBinding {
        EnvironmentBinding::new(
            environment_id,
            EnvironmentBackendId::WindowsSandbox,
            "windows-sandbox-v0.8-ephemeral".to_owned(),
        )
    }

    fn environment_root(&self, environment_id: Uuid) -> PathBuf {
        environment_directory(&self.state_root, environment_id)
    }

    fn config_path(&self, environment_id: Uuid) -> PathBuf {
        self.environment_root(environment_id)
            .join("environment.wsb")
    }

    fn staged_bootstrap_directory(&self, environment_id: Uuid) -> PathBuf {
        self.environment_root(environment_id).join("bootstrap")
    }

    fn controller_script_path(&self) -> PathBuf {
        self.bootstrap_directory.join("verisilo-sandbox.ps1")
    }

    fn status_path(&self, environment_id: Uuid) -> PathBuf {
        self.environment_root(environment_id)
            .join("sandbox-status.json")
    }

    fn process_receipt_path(&self, environment_id: Uuid) -> PathBuf {
        self.environment_root(environment_id)
            .join("sandbox-process.json")
    }

    fn expected_config(&self, environment_id: Uuid) -> Result<String, EnvironmentBackendError> {
        generate_sandbox_config(
            environment_id,
            &[SandboxMappedFolder {
                host_folder: self.staged_bootstrap_directory(environment_id),
                sandbox_folder: "C:\\VeriSilo\\Bootstrap".to_owned(),
                read_only: true,
            }],
            true,
        )
    }

    fn stage_bootstrap(&self, environment_id: Uuid) -> Result<(), EnvironmentBackendError> {
        let source = self
            .bootstrap_directory
            .join("verisilo-sandbox-bootstrap.ps1");
        let bytes = read_bounded_regular_file(&source, 256 * 1024)?;
        let destination_directory = self.staged_bootstrap_directory(environment_id);
        ensure_state_directory(&destination_directory)?;
        write_idempotent_file(
            &destination_directory.join("verisilo-sandbox-bootstrap.ps1"),
            &bytes,
        )
    }

    fn validate_staged_bootstrap(
        &self,
        environment_id: Uuid,
    ) -> Result<(), EnvironmentBackendError> {
        let source = read_bounded_regular_file(
            &self
                .bootstrap_directory
                .join("verisilo-sandbox-bootstrap.ps1"),
            256 * 1024,
        )?;
        let staged_directory = self.staged_bootstrap_directory(environment_id);
        let metadata =
            fs::symlink_metadata(&staged_directory).map_err(EnvironmentBackendError::Io)?;
        if metadata_is_reparse_point(&metadata) || !metadata.is_dir() {
            return Err(EnvironmentBackendError::Protocol(
                "Sandbox bootstrap mapping must remain a real non-reparse directory.".to_owned(),
            ));
        }
        let staged = read_bounded_regular_file(
            &staged_directory.join("verisilo-sandbox-bootstrap.ps1"),
            256 * 1024,
        )?;
        if staged != source {
            return Err(EnvironmentBackendError::Protocol(
                "Staged Sandbox bootstrap differs from the fixed release resource.".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn command_spec(
        &self,
        request_path: &Path,
    ) -> Result<CommandSpec, EnvironmentBackendError> {
        require_absolute_clean_path(request_path, "Sandbox request path")?;
        let controller = self.controller_script_path();
        require_absolute_clean_path(&controller, "Sandbox controller path")?;
        let sandbox_executable = (self.system_tool_resolver)(ProviderSystemTool::WindowsSandbox)?;
        Ok(CommandSpec {
            program: (self.system_tool_resolver)(ProviderSystemTool::PowerShell)?,
            args: vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-ExecutionPolicy".into(),
                "AllSigned".into(),
                "-File".into(),
                controller.into_os_string(),
                "-RequestPath".into(),
                request_path.as_os_str().to_owned(),
                "-StateRoot".into(),
                self.state_root.as_os_str().to_owned(),
                "-SandboxExecutable".into(),
                sandbox_executable.into_os_string(),
            ],
            stdin: None,
            completion: CommandCompletion::WaitForExit,
            timeout: PROVIDER_TIMEOUT,
        })
    }

    fn invoke(
        &mut self,
        operation: EnvironmentOperation,
        action: SandboxAction,
        environment_id: Uuid,
        confirm_destroy: bool,
    ) -> Result<SandboxScriptResponse, EnvironmentBackendError> {
        self.ensure_available(operation)?;
        require_binding(&self.state_root, &self.binding(environment_id))?;
        let directory = self.environment_root(environment_id);
        ensure_state_directory(&directory)?;
        let request_path = directory.join(format!("{}.sandbox-request.json", Uuid::new_v4()));
        let request = SandboxScriptRequest {
            schema_version: ENVIRONMENT_CONTRACT_VERSION,
            action,
            environment_id,
            confirm_destroy,
        };
        write_new_file(&request_path, &serde_json::to_vec_pretty(&request)?)?;
        let output_result = self.runner.run(&self.command_spec(&request_path)?);
        remove_regular_file_if_exists(&request_path)?;
        let output = output_result?;
        if !output.success {
            return Err(process_failure(&output));
        }
        let response: SandboxScriptResponse = serde_json::from_slice(&output.stdout)?;
        let observed_is_fresh = chrono::DateTime::parse_from_rfc3339(&response.observed_at)
            .ok()
            .map(|observed| observed.with_timezone(&Utc))
            .is_some_and(|observed| {
                let now = Utc::now();
                observed <= now + Duration::seconds(30) && observed >= now - Duration::minutes(2)
            });
        if response.schema_version != ENVIRONMENT_CONTRACT_VERSION
            || response.action != action
            || response.environment_id != environment_id
            || !response.success
            || response.source != "sandbox-controller"
            || !observed_is_fresh
            || response.guest_health != GuestEvidenceState::Unavailable
            || response.proxy != GuestEvidenceState::Unavailable
            || response.exit != GuestEvidenceState::Unavailable
            || response.proxy_dns != GuestEvidenceState::Unavailable
            || response.guest_resolver != GuestEvidenceState::Unavailable
            || response.browser_ready != GuestEvidenceState::Unavailable
        {
            return Err(EnvironmentBackendError::Protocol(
                "Sandbox controller response did not match the exact Silo lifecycle action or preserve unavailable guest evidence."
                    .to_owned(),
            ));
        }
        let has_valid_process_id = response.process_id.is_some_and(|process_id| process_id > 0);
        let state_is_valid = match action {
            SandboxAction::Start | SandboxAction::Health => {
                response.state == SandboxControllerState::Running && has_valid_process_id
            }
            SandboxAction::Stop => {
                response.state == SandboxControllerState::Stopped && response.process_id.is_none()
            }
            SandboxAction::Logs => match response.state {
                SandboxControllerState::Running => response.process_id.is_some(),
                SandboxControllerState::Exited => response.process_id.is_none(),
                _ => false,
            },
            SandboxAction::AssertExited => {
                response.state == SandboxControllerState::Exited
                    && response.process_id.is_none()
                    && confirm_destroy
            }
        };
        if !state_is_valid {
            return Err(EnvironmentBackendError::Protocol(
                "Sandbox controller returned an impossible process state for the fixed action."
                    .to_owned(),
            ));
        }
        Ok(response)
    }

    fn ensure_available(
        &self,
        operation: EnvironmentOperation,
    ) -> Result<(), EnvironmentBackendError> {
        if self.supported_platform && self.sandbox_available {
            Ok(())
        } else {
            Err(unavailable(
                self.backend_id(),
                operation,
                "Windows Sandbox is not installed or is unavailable on this Windows edition.",
            ))
        }
    }

    fn ensure_supported_platform(
        &self,
        operation: EnvironmentOperation,
    ) -> Result<(), EnvironmentBackendError> {
        if self.supported_platform {
            Ok(())
        } else {
            Err(unavailable(
                self.backend_id(),
                operation,
                "Windows Sandbox descriptor cleanup is available only on Windows hosts.",
            ))
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum SandboxAction {
    Start,
    Stop,
    Health,
    Logs,
    AssertExited,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SandboxScriptRequest {
    schema_version: u32,
    action: SandboxAction,
    environment_id: Uuid,
    confirm_destroy: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SandboxControllerState {
    Running,
    Stopped,
    Exited,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SandboxScriptResponse {
    schema_version: u32,
    action: SandboxAction,
    environment_id: Uuid,
    success: bool,
    state: SandboxControllerState,
    process_id: Option<u32>,
    observed_at: String,
    source: String,
    guest_health: GuestEvidenceState,
    proxy: GuestEvidenceState,
    exit: GuestEvidenceState,
    proxy_dns: GuestEvidenceState,
    guest_resolver: GuestEvidenceState,
    browser_ready: GuestEvidenceState,
}

impl<R: ProcessRunner> EnvironmentBackend for WindowsSandboxBackend<R> {
    fn backend_id(&self) -> EnvironmentBackendId {
        EnvironmentBackendId::WindowsSandbox
    }

    fn status(&self) -> EnvironmentBackendStatus {
        let ready = self.supported_platform && self.sandbox_available;
        let conditional = |operation| {
            if ready {
                available(operation)
            } else {
                unsupported(operation, "Windows Sandbox is not available on this host.")
            }
        };
        let cleanup = if ready {
            available(EnvironmentOperation::Destroy)
        } else {
            unsupported(
                EnvironmentOperation::Destroy,
                "Safe Sandbox cleanup requires Windows plus the trusted exact-process controller.",
            )
        };
        EnvironmentBackendStatus {
            contract_version: ENVIRONMENT_CONTRACT_VERSION,
            backend: self.backend_id(),
            capabilities: vec![
                conditional(EnvironmentOperation::Create),
                conditional(EnvironmentOperation::Start),
                conditional(EnvironmentOperation::Stop),
                unsupported(
                    EnvironmentOperation::Pause,
                    "Windows Sandbox is disposable and does not support pause.",
                ),
                unsupported(
                    EnvironmentOperation::Snapshot,
                    "Windows Sandbox is disposable and does not support snapshots.",
                ),
                cleanup,
                conditional(EnvironmentOperation::ConfigureNetwork),
                conditional(EnvironmentOperation::Health),
                conditional(EnvironmentOperation::Logs),
            ],
            prerequisites: vec![
                EnvironmentPrerequisite {
                    id: "windows-sandbox-feature".to_owned(),
                    state: if ready {
                        PrerequisiteState::Verified
                    } else {
                        PrerequisiteState::Missing
                    },
                    detail:
                        "WindowsSandbox.exe and the optional Windows feature must both be available."
                            .to_owned(),
                },
                EnvironmentPrerequisite {
                    id: "default-deny-descriptor".to_owned(),
                    state: if ready {
                        PrerequisiteState::Configured
                    } else {
                        PrerequisiteState::Missing
                    },
                    detail: "The deterministic descriptor disables clipboard, devices, vGPU, and writable mappings; this is configured policy, not guest-observed enforcement."
                        .to_owned(),
                },
                EnvironmentPrerequisite {
                    id: "concurrent-multi-silo".to_owned(),
                    state: PrerequisiteState::Unavailable,
                    detail: "Windows Sandbox supports only one running instance; V0.8 does not claim concurrent Silo execution."
                        .to_owned(),
                },
                EnvironmentPrerequisite {
                    id: "bundled-mihomo-tun".to_owned(),
                    state: PrerequisiteState::Unavailable,
                    detail: "Mihomo and TUN routing are outside the Sandbox V0.8 capability boundary."
                        .to_owned(),
                },
                EnvironmentPrerequisite {
                    id: "guest-return-channel".to_owned(),
                    state: PrerequisiteState::Unavailable,
                    detail: "The controller can verify only its exact WindowsSandbox.exe process; guest health, network, DNS, and browser readiness have no reliable return channel and remain unavailable."
                        .to_owned(),
                },
            ],
        }
    }

    fn create(
        &mut self,
        request: CreateEnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        self.ensure_available(EnvironmentOperation::Create)?;
        request.network.validate()?;
        if request.network.is_fixed_proxy() {
            return Err(unavailable(
                self.backend_id(),
                EnvironmentOperation::Create,
                "Windows Sandbox V0.8 does not configure fixed proxies and never falls back to DIRECT.",
            ));
        }
        ensure_binding(&self.state_root, &self.binding(request.environment_id))?;
        self.stage_bootstrap(request.environment_id)?;
        let config_path = self.config_path(request.environment_id);
        let xml = self.expected_config(request.environment_id)?;
        write_idempotent_file(&config_path, xml.as_bytes())?;
        let mut result = receipt(
            self.backend_id(),
            EnvironmentOperation::Create,
            request.environment_id,
            EnvironmentActionState::Configured,
            "Disposable Sandbox configuration generated with host integrations denied by default.",
        );
        result.artifact_path = Some(config_path.to_string_lossy().into_owned());
        Ok(result)
    }

    fn start(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        self.ensure_available(EnvironmentOperation::Start)?;
        require_binding(&self.state_root, &self.binding(request.environment_id))?;
        self.validate_staged_bootstrap(request.environment_id)?;
        let config = self.config_path(request.environment_id);
        let expected = self.expected_config(request.environment_id)?;
        if read_bounded_regular_file(&config, expected.len())? != expected.as_bytes() {
            return Err(EnvironmentBackendError::Protocol(
                "Sandbox descriptor changed after deterministic creation.".to_owned(),
            ));
        }
        let response = self.invoke(
            EnvironmentOperation::Start,
            SandboxAction::Start,
            request.environment_id,
            false,
        )?;
        Ok(receipt(
            self.backend_id(),
            EnvironmentOperation::Start,
            request.environment_id,
            EnvironmentActionState::Started,
            format!(
                "The fixed controller started and durably bound WindowsSandbox.exe process {}; guest network, DNS, and browser readiness remain unavailable.",
                response.process_id.expect("validated running process")
            ),
        ))
    }

    fn stop(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        self.invoke(
            EnvironmentOperation::Stop,
            SandboxAction::Stop,
            request.environment_id,
            false,
        )?;
        Ok(receipt(
            self.backend_id(),
            EnvironmentOperation::Stop,
            request.environment_id,
            EnvironmentActionState::Stopped,
            "The controller requested graceful close only on the exact PID/start-time/executable binding and did not force-kill any Sandbox process.",
        ))
    }

    fn pause(
        &mut self,
        _request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        Err(unavailable(
            self.backend_id(),
            EnvironmentOperation::Pause,
            "Windows Sandbox has no persistent pause lifecycle.",
        ))
    }

    fn snapshot(
        &mut self,
        _request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        Err(unavailable(
            self.backend_id(),
            EnvironmentOperation::Snapshot,
            "Windows Sandbox is disposable and cannot be snapshotted.",
        ))
    }

    fn destroy(
        &mut self,
        request: DestroyEnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        self.ensure_supported_platform(EnvironmentOperation::Destroy)?;
        if !request.confirm_destroy {
            return Err(EnvironmentBackendError::InvalidRequest(
                "Destroy requires an explicit confirmation.".to_owned(),
            ));
        }
        let directory = self.environment_root(request.environment_id);
        match fs::symlink_metadata(&directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(receipt(
                    self.backend_id(),
                    EnvironmentOperation::Destroy,
                    request.environment_id,
                    EnvironmentActionState::Destroyed,
                    "Sandbox guest state is ephemeral and no local descriptor remained to clean up.",
                ));
            }
            Err(error) => return Err(EnvironmentBackendError::Io(error)),
            Ok(metadata) if metadata_is_reparse_point(&metadata) || !metadata.is_dir() => {
                return Err(EnvironmentBackendError::Protocol(
                    "Sandbox state root for this Silo is not a real directory.".to_owned(),
                ));
            }
            Ok(_) => {}
        }
        let binding = binding_path(&self.state_root, request.environment_id);
        if matches!(
            fs::symlink_metadata(&binding),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound
        ) {
            remove_empty_directory_if_exists(
                &self.staged_bootstrap_directory(request.environment_id),
            )?;
            remove_empty_directory_if_exists(&directory)?;
            return Ok(receipt(
                self.backend_id(),
                EnvironmentOperation::Destroy,
                request.environment_id,
                EnvironmentActionState::Destroyed,
                "Recovered an interrupted Sandbox descriptor cleanup with no binding or artifacts remaining.",
            ));
        }
        require_binding(&self.state_root, &self.binding(request.environment_id))?;
        self.invoke(
            EnvironmentOperation::Destroy,
            SandboxAction::AssertExited,
            request.environment_id,
            true,
        )?;
        remove_regular_file_if_exists(&self.config_path(request.environment_id))?;
        remove_regular_file_if_exists(&self.status_path(request.environment_id))?;
        remove_regular_file_if_exists(&self.process_receipt_path(request.environment_id))?;
        remove_regular_file_if_exists(
            &self
                .staged_bootstrap_directory(request.environment_id)
                .join("verisilo-sandbox-bootstrap.ps1"),
        )?;
        remove_empty_directory_if_exists(&self.staged_bootstrap_directory(request.environment_id))?;
        require_only_directory_entries(&directory, &["binding.json"])?;
        remove_regular_file_if_exists(&binding)?;
        remove_empty_directory_if_exists(&directory)?;
        Ok(receipt(
            self.backend_id(),
            EnvironmentOperation::Destroy,
            request.environment_id,
            EnvironmentActionState::Destroyed,
            "The exact Sandbox process was first confirmed exited; only then were the local descriptor, controller receipts, and read-only bootstrap copy deleted.",
        ))
    }

    fn configure_network(
        &mut self,
        request: ConfigureNetworkRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        self.ensure_available(EnvironmentOperation::ConfigureNetwork)?;
        request.network.validate()?;
        if request.network.is_fixed_proxy() {
            return Err(unavailable(
                self.backend_id(),
                EnvironmentOperation::ConfigureNetwork,
                "Sandbox fixed-proxy configuration is unavailable without guest-origin enforcement evidence; there is no DIRECT fallback.",
            ));
        }
        require_binding(&self.state_root, &self.binding(request.environment_id))?;
        let expected = self.expected_config(request.environment_id)?;
        if read_bounded_regular_file(&self.config_path(request.environment_id), expected.len())?
            != expected.as_bytes()
        {
            return Err(EnvironmentBackendError::Protocol(
                "Sandbox descriptor does not match its explicit DIRECT configuration.".to_owned(),
            ));
        }
        Ok(receipt(
            self.backend_id(),
            EnvironmentOperation::ConfigureNetwork,
            request.environment_id,
            EnvironmentActionState::Configured,
            "Sandbox networking is configured, not verified; no exit or DNS claim was made.",
        ))
    }

    fn health(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        let response = self.invoke(
            EnvironmentOperation::Health,
            SandboxAction::Health,
            request.environment_id,
            false,
        )?;
        Ok(receipt(
            self.backend_id(),
            EnvironmentOperation::Health,
            request.environment_id,
            EnvironmentActionState::Healthy,
            format!(
                "Exact Sandbox controller process {} is running; guest health, network, DNS, and browser readiness remain unavailable.",
                response.process_id.expect("validated running process")
            ),
        ))
    }

    fn logs(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        let response = self.invoke(
            EnvironmentOperation::Logs,
            SandboxAction::Logs,
            request.environment_id,
            false,
        )?;
        let log_path = self.status_path(request.environment_id);
        let persisted: SandboxScriptResponse =
            serde_json::from_slice(&read_bounded_regular_file(&log_path, 8 * 1024)?)?;
        if persisted != response {
            return Err(EnvironmentBackendError::Protocol(
                "Persisted Sandbox status did not match the fresh controller response.".to_owned(),
            ));
        }
        let mut result = receipt(
            self.backend_id(),
            EnvironmentOperation::Logs,
            request.environment_id,
            EnvironmentActionState::LogsExported,
            "Exported the bounded exact-process controller receipt; no guest log or guest evidence was claimed.",
        );
        result.artifact_path = Some(log_path.to_string_lossy().into_owned());
        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// Hyper-V

#[derive(Debug, Clone)]
pub struct HyperVPrerequisites {
    pub supported_platform: bool,
    pub supported_sku: bool,
    pub administrator: bool,
    pub virtualization_enabled: bool,
    pub hyperv_enabled: bool,
    pub reboot_required: bool,
    pub release_scripts_trusted: bool,
}

#[derive(Debug, Clone)]
pub struct ValidatedHyperVImage {
    pub file_name: String,
    pub sha256: String,
    pub verified: bool,
}

pub struct HyperVBackend<R: ProcessRunner> {
    prerequisites: HyperVPrerequisites,
    state_root: PathBuf,
    approved_image_root: PathBuf,
    script_path: PathBuf,
    image: Option<ValidatedHyperVImage>,
    runner: R,
}

impl<R: ProcessRunner> HyperVBackend<R> {
    pub fn new(
        prerequisites: HyperVPrerequisites,
        state_root: PathBuf,
        approved_image_root: PathBuf,
        script_path: PathBuf,
        image: Option<ValidatedHyperVImage>,
        runner: R,
    ) -> Result<Self, EnvironmentBackendError> {
        require_absolute_clean_path(&state_root, "Hyper-V state root")?;
        require_absolute_clean_path(&approved_image_root, "approved image root")?;
        require_absolute_clean_path(&script_path, "Hyper-V script path")?;
        if let Some(image) = &image {
            validate_image_descriptor(image)?;
        }
        Ok(Self {
            prerequisites,
            state_root,
            approved_image_root,
            script_path,
            image,
            runner,
        })
    }

    fn control_ready(&self) -> bool {
        self.prerequisites.supported_platform
            && self.prerequisites.supported_sku
            && self.prerequisites.administrator
            && self.prerequisites.virtualization_enabled
            && self.prerequisites.hyperv_enabled
            && !self.prerequisites.reboot_required
            && self.prerequisites.release_scripts_trusted
    }

    fn create_ready(&self) -> bool {
        self.control_ready() && self.image.as_ref().is_some_and(|image| image.verified)
    }

    fn binding(&self, environment_id: Uuid) -> EnvironmentBinding {
        EnvironmentBinding::new(
            environment_id,
            EnvironmentBackendId::HyperV,
            format!("VeriSilo-{environment_id}"),
        )
    }

    fn ensure_ready(
        &self,
        operation: EnvironmentOperation,
        action: HyperVAction,
    ) -> Result<(), EnvironmentBackendError> {
        if (action == HyperVAction::Create && self.create_ready())
            || (action != HyperVAction::Create && self.control_ready())
        {
            Ok(())
        } else {
            Err(unavailable(
                self.backend_id(),
                operation,
                "Hyper-V needs a supported SKU, administrator token, enabled virtualization/feature, no pending reboot, same-signer release scripts, and a build-pinned base-image manifest.",
            ))
        }
    }

    fn command_spec(
        &self,
        request_path: &Path,
        action: HyperVAction,
        environment_id: Uuid,
        request_nonce: Uuid,
    ) -> Result<CommandSpec, EnvironmentBackendError> {
        require_absolute_clean_path(request_path, "Hyper-V request path")?;
        Ok(CommandSpec {
            program: provider_system_tool(ProviderSystemTool::PowerShell)?,
            args: vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-ExecutionPolicy".into(),
                "AllSigned".into(),
                "-File".into(),
                self.script_path.as_os_str().to_owned(),
                "-RequestPath".into(),
                request_path.as_os_str().to_owned(),
                "-StateRoot".into(),
                self.state_root.as_os_str().to_owned(),
                "-ApprovedImageRoot".into(),
                self.approved_image_root.as_os_str().to_owned(),
                "-ExpectedEnvironmentId".into(),
                environment_id.to_string().into(),
                "-ExpectedAction".into(),
                serde_json::to_value(action)?
                    .as_str()
                    .expect("Hyper-V action serializes as a string")
                    .into(),
                "-ExpectedRequestNonce".into(),
                request_nonce.to_string().into(),
            ],
            stdin: None,
            completion: CommandCompletion::WaitForExit,
            timeout: PROVIDER_TIMEOUT,
        })
    }

    fn invoke(
        &mut self,
        operation: EnvironmentOperation,
        action: HyperVAction,
        environment_id: Uuid,
        confirm_destroy: bool,
    ) -> Result<HyperVScriptResponse, EnvironmentBackendError> {
        self.ensure_ready(operation, action)?;
        let binding = self.binding(environment_id);
        if action == HyperVAction::Create {
            ensure_binding(&self.state_root, &binding)?;
        } else {
            require_binding(&self.state_root, &binding)?;
        }
        let directory = environment_directory(&self.state_root, environment_id);
        ensure_state_directory(&directory)?;
        let request_nonce = Uuid::new_v4();
        let request_path = directory.join(format!("{request_nonce}.request.json"));
        let create_image = if action == HyperVAction::Create {
            Some(self.image.as_ref().ok_or_else(|| {
                unavailable(
                    self.backend_id(),
                    operation,
                    "Hyper-V create requires a build-pinned signed-manifest image.",
                )
            })?)
        } else {
            None
        };
        let image_lease = create_image
            .map(|image| {
                HyperVImageLease::acquire(
                    &self.approved_image_root,
                    &image.file_name,
                    &image.sha256,
                )
            })
            .transpose()?;
        let request = HyperVScriptRequest {
            schema_version: ENVIRONMENT_CONTRACT_VERSION,
            action,
            environment_id,
            request_nonce,
            confirm_destroy,
            manifest_schema_version: if action == HyperVAction::Create {
                Some(ENVIRONMENT_CONTRACT_VERSION)
            } else {
                None
            },
            manifest_image_file: if action == HyperVAction::Create {
                create_image.map(|image| image.file_name.clone())
            } else {
                None
            },
            manifest_image_sha256: if action == HyperVAction::Create {
                create_image.map(|image| image.sha256.clone())
            } else {
                None
            },
            manifest_trusted: if action == HyperVAction::Create {
                create_image.map(|image| image.verified)
            } else {
                None
            },
        };
        let bytes = serde_json::to_vec_pretty(&request)?;
        let request_lease =
            HyperVRequestLease::create(&self.state_root, &directory, &request_path, &bytes)?;
        let mut spec = self.command_spec(&request_path, action, environment_id, request_nonce)?;
        if action == HyperVAction::Create {
            spec.timeout = HYPERV_CREATE_TIMEOUT;
        }
        let output_result = self.runner.run(&spec);
        drop(request_lease);
        drop(image_lease);
        remove_regular_file_if_exists(&request_path)?;
        let output = output_result?;
        if !output.success {
            return Err(process_failure(&output));
        }
        let response: HyperVScriptResponse = serde_json::from_slice(&output.stdout)?;
        let observed_is_fresh = chrono::DateTime::parse_from_rfc3339(&response.observed_at)
            .ok()
            .map(|observed| observed.with_timezone(&Utc))
            .is_some_and(|observed| {
                let now = Utc::now();
                observed <= now + Duration::seconds(30) && observed >= now - Duration::minutes(2)
            });
        let image_hash_is_valid = response.base_image_sha256.len() == 64
            && response
                .base_image_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        let create_hash_matches = action != HyperVAction::Create
            || self.image.as_ref().is_some_and(|image| {
                response
                    .base_image_sha256
                    .eq_ignore_ascii_case(&image.sha256)
            });
        if response.schema_version != ENVIRONMENT_CONTRACT_VERSION
            || response.action != action
            || response.environment_id != environment_id
            || response.request_nonce != request_nonce
            || !response.success
            || response.source != "hyperv-controller"
            || !observed_is_fresh
            || response.vm_name != format!("VeriSilo-{environment_id}")
            || !hyperv_response_identity_is_valid(action, &response)
            || response.generation != 2
            || !image_hash_is_valid
            || !create_hash_matches
            || response.guest_agent_version.is_some()
            || response.guest_agent_sha256.is_some()
            || response.guest_profile != GuestEvidenceState::Unavailable
            || response.guest_health != GuestEvidenceState::Unavailable
            || response.proxy != GuestEvidenceState::Unavailable
            || response.exit != GuestEvidenceState::Unavailable
            || response.proxy_dns != GuestEvidenceState::Unavailable
            || response.guest_resolver != GuestEvidenceState::Unavailable
            || response.browser_ready != GuestEvidenceState::Unavailable
        {
            return Err(EnvironmentBackendError::Protocol(
                "Hyper-V response did not match the exact action, Silo, VM identity, generation, image receipt, or explicit unavailable guest-evidence boundary."
                    .to_owned(),
            ));
        }
        Ok(response)
    }
}

impl<R: ProcessRunner> EnvironmentBackend for HyperVBackend<R> {
    fn backend_id(&self) -> EnvironmentBackendId {
        EnvironmentBackendId::HyperV
    }

    fn status(&self) -> EnvironmentBackendStatus {
        let capability = |operation| {
            if self.control_ready() {
                available(operation)
            } else {
                unsupported(operation, "Hyper-V prerequisites are not all verified.")
            }
        };
        let state = |condition| {
            if condition {
                PrerequisiteState::Verified
            } else {
                PrerequisiteState::Missing
            }
        };
        EnvironmentBackendStatus {
            contract_version: ENVIRONMENT_CONTRACT_VERSION,
            backend: self.backend_id(),
            capabilities: vec![
                if self.create_ready() {
                    available(EnvironmentOperation::Create)
                } else {
                    unsupported(
                        EnvironmentOperation::Create,
                        "Hyper-V create additionally requires the build-pinned signed-manifest image.",
                    )
                },
                capability(EnvironmentOperation::Start),
                capability(EnvironmentOperation::Stop),
                capability(EnvironmentOperation::Pause),
                capability(EnvironmentOperation::Snapshot),
                capability(EnvironmentOperation::Destroy),
                unsupported(
                    EnvironmentOperation::ConfigureNetwork,
                    "Hyper-V host networking can be created, but guest proxy/exit/DNS configuration awaits the signed guest agent.",
                ),
                capability(EnvironmentOperation::Health),
                capability(EnvironmentOperation::Logs),
            ],
            prerequisites: vec![
                EnvironmentPrerequisite {
                    id: "windows-sku".to_owned(),
                    state: state(self.prerequisites.supported_platform
                        && self.prerequisites.supported_sku),
                    detail: "Hyper-V is not offered as directly enableable on Windows Home."
                        .to_owned(),
                },
                EnvironmentPrerequisite {
                    id: "administrator".to_owned(),
                    state: state(self.prerequisites.administrator),
                    detail: "VM lifecycle actions require an elevated administrator token."
                        .to_owned(),
                },
                EnvironmentPrerequisite {
                    id: "virtualization".to_owned(),
                    state: state(self.prerequisites.virtualization_enabled),
                    detail: "Firmware virtualization and the Hyper-V feature must be enabled."
                        .to_owned(),
                },
                EnvironmentPrerequisite {
                    id: "reboot".to_owned(),
                    state: if self.prerequisites.reboot_required {
                        PrerequisiteState::Missing
                    } else {
                        PrerequisiteState::Verified
                    },
                    detail: "A pending Hyper-V enablement reboot blocks VM operations.".to_owned(),
                },
                EnvironmentPrerequisite {
                    id: "signed-provider-scripts".to_owned(),
                    state: state(self.prerequisites.release_scripts_trusted),
                    detail: "The host probe and Hyper-V provider must have valid Authenticode signatures from the same signer."
                        .to_owned(),
                },
                EnvironmentPrerequisite {
                    id: "base-image".to_owned(),
                    state: state(
                        self.image
                            .as_ref()
                            .is_some_and(|image| image.verified),
                    ),
                    detail: "The image manifest flag and SHA-256 are both rechecked by the fixed script."
                        .to_owned(),
                },
                EnvironmentPrerequisite {
                    id: "guest-agent-receipt".to_owned(),
                    state: PrerequisiteState::Missing,
                    detail: "No lawful image with a pinned VeriSilo guest-agent version/hash is supplied; guest profile, health, network, DNS, and browser readiness remain unavailable."
                        .to_owned(),
                },
                EnvironmentPrerequisite {
                    id: "concurrent-multi-silo".to_owned(),
                    state: PrerequisiteState::Unavailable,
                    detail: "The fixed provider refuses to start a Hyper-V Silo while another VeriSilo VM is running."
                        .to_owned(),
                },
                EnvironmentPrerequisite {
                    id: "bundled-mihomo-tun".to_owned(),
                    state: PrerequisiteState::Unavailable,
                    detail: "The Hyper-V V0.8 provider creates only an isolated internal switch; Mihomo, TUN, and guest network enforcement remain gated."
                        .to_owned(),
                },
            ],
        }
    }

    fn create(
        &mut self,
        request: CreateEnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        request.network.validate()?;
        if request.network.is_fixed_proxy() {
            return Err(unavailable(
                self.backend_id(),
                EnvironmentOperation::Create,
                "Hyper-V fixed-proxy mode is unavailable until a pinned guest agent returns fresh exit and DNS evidence; there is no DIRECT fallback.",
            ));
        }
        self.invoke(
            EnvironmentOperation::Create,
            HyperVAction::Create,
            request.environment_id,
            false,
        )?;
        Ok(receipt(
            self.backend_id(),
            EnvironmentOperation::Create,
            request.environment_id,
            EnvironmentActionState::Configured,
            "Created an internal switch, differencing disk, and generation-2 VM through the fixed script.",
        ))
    }

    fn start(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        self.invoke(
            EnvironmentOperation::Start,
            HyperVAction::Start,
            request.environment_id,
            false,
        )?;
        Ok(receipt(
            self.backend_id(),
            EnvironmentOperation::Start,
            request.environment_id,
            EnvironmentActionState::Started,
            "Hyper-V reported that the selected VM start request succeeded.",
        ))
    }

    fn stop(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        self.invoke(
            EnvironmentOperation::Stop,
            HyperVAction::Stop,
            request.environment_id,
            false,
        )?;
        Ok(receipt(
            self.backend_id(),
            EnvironmentOperation::Stop,
            request.environment_id,
            EnvironmentActionState::Stopped,
            "Hyper-V reported that the selected VM stop request succeeded.",
        ))
    }

    fn pause(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        self.invoke(
            EnvironmentOperation::Pause,
            HyperVAction::Pause,
            request.environment_id,
            false,
        )?;
        Ok(receipt(
            self.backend_id(),
            EnvironmentOperation::Pause,
            request.environment_id,
            EnvironmentActionState::Stopped,
            "Hyper-V saved the VM state; this is not a snapshot.",
        ))
    }

    fn snapshot(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        self.invoke(
            EnvironmentOperation::Snapshot,
            HyperVAction::Checkpoint,
            request.environment_id,
            false,
        )?;
        Ok(receipt(
            self.backend_id(),
            EnvironmentOperation::Snapshot,
            request.environment_id,
            EnvironmentActionState::Configured,
            "Hyper-V created a production checkpoint for the selected VM.",
        ))
    }

    fn destroy(
        &mut self,
        request: DestroyEnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        if !request.confirm_destroy {
            return Err(EnvironmentBackendError::InvalidRequest(
                "Hyper-V destroy requires explicit confirmation.".to_owned(),
            ));
        }
        self.invoke(
            EnvironmentOperation::Destroy,
            HyperVAction::Remove,
            request.environment_id,
            true,
        )?;
        let directory = environment_directory(&self.state_root, request.environment_id);
        remove_regular_file_if_exists(&directory.join("hyperv-status.json"))?;
        remove_regular_file_if_exists(&directory.join("hyperv-receipt.json"))?;
        require_only_directory_entries(&directory, &["binding.json"])?;
        remove_regular_file_if_exists(&binding_path(&self.state_root, request.environment_id))?;
        remove_empty_directory_if_exists(&directory)?;
        Ok(receipt(
            self.backend_id(),
            EnvironmentOperation::Destroy,
            request.environment_id,
            EnvironmentActionState::Destroyed,
            "The explicitly confirmed VM, differencing disk, and per-environment switch were removed.",
        ))
    }

    fn configure_network(
        &mut self,
        _request: ConfigureNetworkRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        Err(unavailable(
            self.backend_id(),
            EnvironmentOperation::ConfigureNetwork,
            "Host switch creation is not guest network verification; signed guest-agent support is required.",
        ))
    }

    fn health(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        self.invoke(
            EnvironmentOperation::Health,
            HyperVAction::Health,
            request.environment_id,
            false,
        )?;
        Ok(receipt(
            self.backend_id(),
            EnvironmentOperation::Health,
            request.environment_id,
            EnvironmentActionState::Healthy,
            "Hyper-V control-plane health was read; this does not prove guest or network health.",
        ))
    }

    fn logs(
        &mut self,
        request: EnvironmentRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        let response = self.invoke(
            EnvironmentOperation::Logs,
            HyperVAction::Logs,
            request.environment_id,
            false,
        )?;
        let log_path = environment_directory(&self.state_root, request.environment_id)
            .join("hyperv-status.json");
        let persisted: HyperVScriptResponse =
            serde_json::from_slice(&read_bounded_regular_file(&log_path, 4 * 1024)?)?;
        if persisted != response {
            return Err(EnvironmentBackendError::Protocol(
                "Persisted Hyper-V status did not match the fresh controller response.".to_owned(),
            ));
        }
        let mut result = receipt(
            self.backend_id(),
            EnvironmentOperation::Logs,
            request.environment_id,
            EnvironmentActionState::LogsExported,
            "Exported bounded Hyper-V control-plane state; guest logs require a guest agent.",
        );
        result.artifact_path = Some(log_path.to_string_lossy().into_owned());
        Ok(result)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum HyperVAction {
    Create,
    Start,
    Stop,
    Pause,
    Checkpoint,
    Remove,
    Health,
    Logs,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum HyperVCleanupState {
    RemovedFromReceipt,
    RolledBackFromJournal,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HyperVScriptRequest {
    schema_version: u32,
    action: HyperVAction,
    environment_id: Uuid,
    request_nonce: Uuid,
    confirm_destroy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest_schema_version: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest_image_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest_image_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest_trusted: Option<bool>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HyperVScriptResponse {
    schema_version: u32,
    action: HyperVAction,
    environment_id: Uuid,
    request_nonce: Uuid,
    success: bool,
    source: String,
    observed_at: String,
    vm_name: String,
    vm_id: Option<Uuid>,
    cleanup_state: Option<HyperVCleanupState>,
    generation: u8,
    base_image_sha256: String,
    guest_agent_version: Option<String>,
    guest_agent_sha256: Option<String>,
    guest_profile: GuestEvidenceState,
    guest_health: GuestEvidenceState,
    proxy: GuestEvidenceState,
    exit: GuestEvidenceState,
    proxy_dns: GuestEvidenceState,
    guest_resolver: GuestEvidenceState,
    browser_ready: GuestEvidenceState,
}

fn hyperv_response_identity_is_valid(
    action: HyperVAction,
    response: &HyperVScriptResponse,
) -> bool {
    let has_non_nil_vm = response.vm_id.is_some_and(|vm_id| vm_id != Uuid::nil());
    match (action, response.cleanup_state) {
        (HyperVAction::Remove, Some(HyperVCleanupState::RemovedFromReceipt)) => has_non_nil_vm,
        (HyperVAction::Remove, Some(HyperVCleanupState::RolledBackFromJournal)) => {
            response.vm_id.is_none() || has_non_nil_vm
        }
        (HyperVAction::Remove, None) => false,
        (_, None) => has_non_nil_vm,
        (_, Some(_)) => false,
    }
}

fn validate_image_descriptor(image: &ValidatedHyperVImage) -> Result<(), EnvironmentBackendError> {
    let file_name_is_safe = image.file_name.len() <= 125
        && image.file_name.ends_with(".vhdx")
        && image.file_name.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || (index > 0 && matches!(byte, b'.' | b'_' | b'-'))
        })
        && !image.file_name.contains("..")
        && !matches!(
            image.file_name.split('.').next(),
            Some(
                "con"
                    | "prn"
                    | "aux"
                    | "nul"
                    | "com1"
                    | "com2"
                    | "com3"
                    | "com4"
                    | "com5"
                    | "com6"
                    | "com7"
                    | "com8"
                    | "com9"
                    | "lpt1"
                    | "lpt2"
                    | "lpt3"
                    | "lpt4"
                    | "lpt5"
                    | "lpt6"
                    | "lpt7"
                    | "lpt8"
                    | "lpt9"
            )
        );
    if !file_name_is_safe {
        return Err(EnvironmentBackendError::InvalidRequest(
            "Hyper-V image must be a strict lowercase VHDX leaf filename under the approved image root."
                .to_owned(),
        ));
    }
    if image.sha256.len() != 64
        || !image
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(EnvironmentBackendError::InvalidRequest(
            "Hyper-V image SHA-256 must contain exactly 64 hexadecimal characters.".to_owned(),
        ));
    }
    Ok(())
}

fn require_absolute_clean_path(path: &Path, label: &str) -> Result<(), EnvironmentBackendError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(EnvironmentBackendError::InvalidRequest(format!(
            "{label} must be an absolute normalized path without traversal."
        )));
    }
    reject_existing_reparse_components(path, label)?;
    Ok(())
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), EnvironmentBackendError> {
    reject_existing_reparse_components(path, "New environment artifact path")?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options.open(path).map_err(EnvironmentBackendError::Io)?;
    file.write_all(bytes).map_err(EnvironmentBackendError::Io)?;
    file.sync_all().map_err(EnvironmentBackendError::Io)
}

struct HyperVRequestLease {
    _request_file: fs::File,
    #[cfg(target_os = "windows")]
    _directory_chain: Vec<fs::File>,
}

struct HyperVImageLease {
    _image_file: fs::File,
    #[cfg(target_os = "windows")]
    _directory_chain: Vec<fs::File>,
}

impl HyperVImageLease {
    fn acquire(
        approved_image_root: &Path,
        image_file_name: &str,
        expected_sha256: &str,
    ) -> Result<Self, EnvironmentBackendError> {
        require_absolute_clean_path(approved_image_root, "approved Hyper-V image root")?;
        let image_path = approved_image_root.join(image_file_name);
        require_absolute_clean_path(&image_path, "approved Hyper-V image path")?;
        if image_path.parent() != Some(approved_image_root)
            || image_path.file_name().and_then(|name| name.to_str()) != Some(image_file_name)
        {
            return Err(EnvironmentBackendError::InvalidRequest(
                "Hyper-V base image must be the exact manifest leaf under ApprovedImageRoot."
                    .to_owned(),
            ));
        }

        #[cfg(target_os = "windows")]
        let directory_chain = open_locked_hyperv_directory_chain(approved_image_root)?;

        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(target_os = "windows")]
        {
            // The provider and New-VHD may read the parent VHDX, while writes,
            // rename, deletion, and directory replacement remain impossible.
            options
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
        let image_file = options
            .open(&image_path)
            .map_err(EnvironmentBackendError::Io)?;
        let metadata = image_file.metadata().map_err(EnvironmentBackendError::Io)?;
        if metadata_is_reparse_point(&metadata) || !metadata.is_file() || metadata.len() == 0 {
            return Err(EnvironmentBackendError::InvalidRequest(
                "Hyper-V base image must be a non-empty regular non-reparse file.".to_owned(),
            ));
        }

        #[cfg(target_os = "windows")]
        let image_file = {
            let mut image_file = image_file;
            let actual_sha256 = sha256_from_locked_file(&mut image_file)?;
            if actual_sha256 != expected_sha256 {
                return Err(EnvironmentBackendError::Protocol(
                    "The locked Hyper-V base image did not match the build-pinned SHA-256."
                        .to_owned(),
                ));
            }
            image_file
        };

        #[cfg(not(target_os = "windows"))]
        let _ = expected_sha256;

        Ok(Self {
            _image_file: image_file,
            #[cfg(target_os = "windows")]
            _directory_chain: directory_chain,
        })
    }
}

#[cfg(target_os = "windows")]
struct BCryptAlgorithm(BCRYPT_ALG_HANDLE);

#[cfg(target_os = "windows")]
impl Drop for BCryptAlgorithm {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                BCryptCloseAlgorithmProvider(self.0, 0);
            }
        }
    }
}

#[cfg(target_os = "windows")]
struct BCryptHash(BCRYPT_HASH_HANDLE);

#[cfg(target_os = "windows")]
impl Drop for BCryptHash {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                BCryptDestroyHash(self.0);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn require_nt_success(status: i32, operation: &str) -> Result<(), EnvironmentBackendError> {
    if status >= 0 {
        Ok(())
    } else {
        Err(EnvironmentBackendError::Protocol(format!(
            "Windows CNG failed to {operation} (NTSTATUS 0x{:08x}).",
            status as u32
        )))
    }
}

#[cfg(target_os = "windows")]
fn sha256_from_locked_file(file: &mut fs::File) -> Result<String, EnvironmentBackendError> {
    let mut algorithm = std::ptr::null_mut();
    require_nt_success(
        unsafe {
            BCryptOpenAlgorithmProvider(
                &mut algorithm,
                BCRYPT_SHA256_ALGORITHM,
                std::ptr::null(),
                0,
            )
        },
        "open SHA-256",
    )?;
    let algorithm = BCryptAlgorithm(algorithm);

    let mut hash = std::ptr::null_mut();
    require_nt_success(
        unsafe {
            BCryptCreateHash(
                algorithm.0,
                &mut hash,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
                0,
                0,
            )
        },
        "create SHA-256 state",
    )?;
    let hash = BCryptHash(hash);

    file.seek(SeekFrom::Start(0))
        .map_err(EnvironmentBackendError::Io)?;
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(EnvironmentBackendError::Io)?;
        if read == 0 {
            break;
        }
        require_nt_success(
            unsafe { BCryptHashData(hash.0, buffer.as_ptr(), read as u32, 0) },
            "hash the locked Hyper-V image",
        )?;
    }
    let mut digest = [0_u8; 32];
    require_nt_success(
        unsafe { BCryptFinishHash(hash.0, digest.as_mut_ptr(), digest.len() as u32, 0) },
        "finish the locked Hyper-V image SHA-256",
    )?;
    file.seek(SeekFrom::Start(0))
        .map_err(EnvironmentBackendError::Io)?;

    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(encoded)
}

impl HyperVRequestLease {
    fn create(
        state_root: &Path,
        request_parent: &Path,
        request_path: &Path,
        bytes: &[u8],
    ) -> Result<Self, EnvironmentBackendError> {
        require_absolute_clean_path(state_root, "Hyper-V request state root")?;
        require_absolute_clean_path(request_parent, "Hyper-V request parent")?;
        require_absolute_clean_path(request_path, "Hyper-V request path")?;
        if request_path.parent() != Some(request_parent)
            || request_parent.parent() != Some(state_root)
        {
            return Err(EnvironmentBackendError::InvalidRequest(
                "Hyper-V request must be a direct child of its exact environment directory."
                    .to_owned(),
            ));
        }

        #[cfg(target_os = "windows")]
        let directory_chain = open_locked_hyperv_directory_chain(request_parent)?;

        let mut options = fs::OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(target_os = "windows")]
        {
            // PowerShell may open the file for reading, but no peer can acquire
            // write or delete access while the elevated provider is running.
            options
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
        let mut request_file = options
            .open(request_path)
            .map_err(EnvironmentBackendError::Io)?;
        request_file
            .write_all(bytes)
            .map_err(EnvironmentBackendError::Io)?;
        request_file
            .sync_all()
            .map_err(EnvironmentBackendError::Io)?;
        let metadata = request_file
            .metadata()
            .map_err(EnvironmentBackendError::Io)?;
        if metadata_is_reparse_point(&metadata)
            || !metadata.is_file()
            || metadata.len() != bytes.len() as u64
        {
            return Err(EnvironmentBackendError::Protocol(
                "Hyper-V request lease did not resolve to the exact regular file that was written."
                    .to_owned(),
            ));
        }

        Ok(Self {
            _request_file: request_file,
            #[cfg(target_os = "windows")]
            _directory_chain: directory_chain,
        })
    }
}

#[cfg(target_os = "windows")]
fn open_locked_hyperv_directory_chain(
    request_parent: &Path,
) -> Result<Vec<fs::File>, EnvironmentBackendError> {
    let mut leases = Vec::new();
    for path in request_parent
        .ancestors()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        if path.as_os_str().is_empty() {
            continue;
        }
        let label = if path == request_parent {
            "Hyper-V request parent"
        } else {
            "Hyper-V request ancestor"
        };
        leases.push(open_locked_hyperv_directory(path, label)?);
    }
    Ok(leases)
}

#[cfg(target_os = "windows")]
fn open_locked_hyperv_directory(
    path: &Path,
    label: &str,
) -> Result<fs::File, EnvironmentBackendError> {
    let mut options = fs::OpenOptions::new();
    options
        .read(true)
        // Directory traversal and ordinary state writes remain available, but
        // replacement, rename, and deletion of the bound directory do not.
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    let directory = options.open(path).map_err(EnvironmentBackendError::Io)?;
    let metadata = directory.metadata().map_err(EnvironmentBackendError::Io)?;
    if metadata_is_reparse_point(&metadata) || !metadata.is_dir() {
        return Err(EnvironmentBackendError::InvalidRequest(format!(
            "{label} must be a real non-reparse directory for the full elevated operation."
        )));
    }
    Ok(directory)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[derive(Default)]
    struct RecordingRunner {
        specs: Vec<CommandSpec>,
        output: Option<CommandOutput>,
    }

    impl ProcessRunner for RecordingRunner {
        fn run(&mut self, spec: &CommandSpec) -> Result<CommandOutput, EnvironmentBackendError> {
            self.specs.push(spec.clone());
            Ok(self.output.clone().unwrap_or(CommandOutput {
                success: true,
                stdout: Vec::new(),
                stderr: Vec::new(),
            }))
        }
    }

    fn temporary_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("verisilo-environment-{label}-{}", Uuid::new_v4()))
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn absolute_verbatim_path_skips_the_non_openable_windows_prefix() {
        let root = temporary_root("verbatim-path");
        fs::create_dir_all(&root).expect("create verbatim path fixture");
        let verbatim_root = fs::canonicalize(&root).expect("canonicalize fixture");

        assert!(matches!(
            verbatim_root.components().next(),
            Some(Component::Prefix(_))
        ));
        require_absolute_clean_path(&verbatim_root, "Verbatim resource root")
            .expect("accept an absolute normalized verbatim path");

        fs::remove_dir_all(&root).expect("remove verbatim path fixture");
    }

    fn assert_fixed_system_tool(program: &Path, basename: &str) {
        #[cfg(target_os = "windows")]
        {
            assert!(program.is_absolute());
            assert_eq!(program.file_name(), Some(std::ffi::OsStr::new(basename)));
        }
        #[cfg(not(target_os = "windows"))]
        assert_eq!(program, Path::new(basename));
    }

    fn fixture_provider_system_tool(
        tool: ProviderSystemTool,
    ) -> Result<PathBuf, EnvironmentBackendError> {
        let basename = match tool {
            ProviderSystemTool::PowerShell => "powershell.exe",
            ProviderSystemTool::Wsl => "wsl.exe",
            ProviderSystemTool::WindowsSandbox => "WindowsSandbox.exe",
        };
        let current_directory = std::env::current_dir().map_err(EnvironmentBackendError::Io)?;
        #[cfg(target_os = "windows")]
        return Ok(current_directory.join(basename));

        #[cfg(not(target_os = "windows"))]
        {
            let _ = current_directory;
            Ok(PathBuf::from(basename))
        }
    }

    fn wsl_prerequisites(agent_sha256: &str) -> WslChromiumPrerequisites {
        WslChromiumPrerequisites {
            supported_platform: true,
            wsl_available: true,
            discovered_distributions: vec!["Ubuntu".to_owned()],
            guest_agent_distributions: vec!["Ubuntu".to_owned()],
            gui_distributions: vec!["Ubuntu".to_owned()],
            expected_agent_sha256: agent_sha256.to_owned(),
        }
    }

    fn wsl_network_output(
        environment_id: Uuid,
        runtime_id: Uuid,
        agent_sha256: &str,
        proxy_port: Option<u16>,
        proxy: &str,
        exit: &str,
        proxy_dns: &str,
    ) -> CommandOutput {
        let observed = Utc::now();
        let observed_at = observed.to_rfc3339();
        let valid_until = (observed + Duration::minutes(2)).to_rfc3339();
        CommandOutput {
            success: true,
            stdout: serde_json::to_vec(&serde_json::json!({
                "schemaVersion": ENVIRONMENT_CONTRACT_VERSION,
                "environmentId": environment_id,
                "source": "guest_agent",
                "agentVersion": WSL_GUEST_AGENT_VERSION,
                "agentSha256": agent_sha256,
                "observedAt": observed_at,
                "evidence": {
                    "schemaVersion": ENVIRONMENT_CONTRACT_VERSION,
                    "evidenceId": Uuid::new_v4(),
                    "environmentId": environment_id,
                    "source": "guest_agent",
                    "runtimeId": runtime_id,
                    "profilePath": WslChromiumBackend::<RecordingRunner>::guest_profile_path(environment_id),
                    "proxyPort": proxy_port,
                    "agentSha256": agent_sha256,
                    "proxy": proxy,
                    "exit": exit,
                    "proxyDns": proxy_dns,
                    "guestResolver": "unavailable",
                    "observedAt": observed_at,
                    "validUntil": valid_until,
                }
            }))
            .expect("network response"),
            stderr: Vec::new(),
        }
    }

    fn wsl_action_output(
        environment_id: Uuid,
        agent_sha256: &str,
        action: &str,
        state: &str,
    ) -> CommandOutput {
        CommandOutput {
            success: true,
            stdout: serde_json::to_vec(&serde_json::json!({
                "schemaVersion": ENVIRONMENT_CONTRACT_VERSION,
                "environmentId": environment_id,
                "source": "guest_agent",
                "action": action,
                "state": state,
                "agentVersion": WSL_GUEST_AGENT_VERSION,
                "agentSha256": agent_sha256,
                "observedAt": Utc::now().to_rfc3339(),
            }))
            .expect("action response"),
            stderr: Vec::new(),
        }
    }

    fn sandbox_output(
        environment_id: Uuid,
        action: &str,
        state: &str,
        process_id: Option<u32>,
    ) -> CommandOutput {
        CommandOutput {
            success: true,
            stdout: serde_json::to_vec(&serde_json::json!({
                "schemaVersion": ENVIRONMENT_CONTRACT_VERSION,
                "action": action,
                "environmentId": environment_id,
                "success": true,
                "state": state,
                "processId": process_id,
                "observedAt": Utc::now().to_rfc3339(),
                "source": "sandbox-controller",
                "guestHealth": "unavailable",
                "proxy": "unavailable",
                "exit": "unavailable",
                "proxyDns": "unavailable",
                "guestResolver": "unavailable",
                "browserReady": "unavailable",
            }))
            .expect("sandbox response"),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn global_environment_inventory_allows_missing_and_empty_provider_directories() {
        let environment_root = temporary_root("global-inventory-clean");
        assert!(local_environment_artifact_inventory(&environment_root)
            .expect("missing inventory root")
            .is_empty());
        ensure_no_local_environment_artifacts_for_restore(&environment_root)
            .expect("missing inventory root is clean");

        for directory in ["wsl", "sandbox", "hyperv"] {
            fs::create_dir_all(environment_root.join(directory)).expect("empty provider directory");
        }
        assert!(local_environment_artifact_inventory(&environment_root)
            .expect("empty provider inventory")
            .is_empty());
        ensure_no_local_environment_artifacts_for_restore(&environment_root)
            .expect("real empty provider directories are clean");
    }

    #[test]
    fn global_environment_inventory_counts_uuid_non_uuid_and_file_artifacts() {
        let environment_root = temporary_root("global-inventory-artifacts");
        fs::create_dir_all(
            environment_root
                .join("wsl")
                .join(Uuid::new_v4().to_string()),
        )
        .expect("UUID provider artifact");
        fs::create_dir_all(environment_root.join("sandbox").join("partial-create"))
            .expect("non-UUID provider artifact");
        fs::create_dir_all(environment_root.join("hyperv")).expect("Hyper-V provider directory");
        fs::write(
            environment_root.join("hyperv").join("interrupted.json"),
            b"partial",
        )
        .expect("file provider artifact");

        assert_eq!(
            local_environment_artifact_inventory(&environment_root)
                .expect("inventory every provider"),
            vec![
                EnvironmentBackendId::WslChromium,
                EnvironmentBackendId::WindowsSandbox,
                EnvironmentBackendId::HyperV,
            ]
        );
        assert!(ensure_no_local_environment_artifacts_for_restore(&environment_root).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn global_environment_inventory_counts_broken_symlinks_without_following_them() {
        let environment_root = temporary_root("global-inventory-child-link");
        let provider_root = environment_root.join("wsl");
        fs::create_dir_all(&provider_root).expect("provider directory");
        std::os::unix::fs::symlink(
            environment_root.join("missing-target"),
            provider_root.join("orphan-link"),
        )
        .expect("broken artifact symlink");

        assert_eq!(
            local_environment_artifact_inventory(&environment_root)
                .expect("broken link is inventoried without traversal"),
            vec![EnvironmentBackendId::WslChromium]
        );
    }

    #[test]
    fn global_environment_inventory_rejects_unexpected_top_level_entries() {
        let environment_root = temporary_root("global-inventory-unexpected");
        fs::create_dir_all(&environment_root).expect("inventory root");
        fs::write(environment_root.join("unknown-provider"), b"unexpected")
            .expect("unexpected top-level file");
        let error = local_environment_artifact_inventory(&environment_root)
            .expect_err("unexpected namespace must fail closed");
        assert!(error.to_string().contains("unexpected top-level entry"));
    }

    #[cfg(unix)]
    #[test]
    fn global_environment_inventory_rejects_linked_provider_namespaces() {
        let environment_root = temporary_root("global-inventory-provider-link");
        let linked_target = temporary_root("global-inventory-provider-link-target");
        fs::create_dir_all(&environment_root).expect("inventory root");
        fs::create_dir_all(&linked_target).expect("linked target");
        std::os::unix::fs::symlink(&linked_target, environment_root.join("sandbox"))
            .expect("linked provider namespace");

        let error = local_environment_artifact_inventory(&environment_root)
            .expect_err("provider link must fail closed");
        assert!(error.to_string().contains("real directories"));
    }

    #[test]
    fn persistent_silo_binding_is_idempotent_and_rejects_backend_drift() {
        let root = temporary_root("binding");
        let environment_id = Uuid::new_v4();
        let binding = EnvironmentBinding::new(
            environment_id,
            EnvironmentBackendId::WslChromium,
            "Ubuntu-24.04".to_owned(),
        );
        ensure_binding(&root, &binding).expect("first binding write");
        ensure_binding(&root, &binding).expect("idempotent binding write");
        require_binding(&root, &binding).expect("binding survives reconstruction");

        let drifted = EnvironmentBinding::new(
            environment_id,
            EnvironmentBackendId::HyperV,
            format!("VeriSilo-{environment_id}"),
        );
        assert!(require_binding(&root, &drifted).is_err());
    }

    #[test]
    fn wsl_uses_discovered_distribution_fixed_agent_and_argument_array() {
        let prerequisites = WslChromiumPrerequisites {
            supported_platform: true,
            wsl_available: true,
            discovered_distributions: vec!["Ubuntu-24.04".to_owned()],
            guest_agent_distributions: vec!["Ubuntu-24.04".to_owned()],
            gui_distributions: vec!["Ubuntu-24.04".to_owned()],
            expected_agent_sha256: "a".repeat(64),
        };
        let state_root = temporary_root("wsl-command");
        let backend = WslChromiumBackend::new(
            "Ubuntu-24.04".to_owned(),
            prerequisites.clone(),
            state_root.clone(),
            RecordingRunner::default(),
        )
        .expect("discovered distro");
        let environment_id = Uuid::new_v4();
        let spec = backend
            .command_spec("start", environment_id, None)
            .expect("fixed start spec");
        assert_fixed_system_tool(&spec.program, "wsl.exe");
        assert_eq!(spec.args[0], "-d");
        assert_eq!(spec.args[1], "Ubuntu-24.04");
        assert_eq!(spec.args[2], "--user");
        assert_eq!(spec.args[3], "root");
        assert_eq!(spec.args[4], "--exec");
        assert_eq!(spec.args[5], WSL_GUEST_AGENT_PATH);
        assert!(!spec
            .args
            .iter()
            .any(|argument| argument == "sh" || argument == "-c"));
        assert!(WslChromiumBackend::new(
            "attacker-controlled".to_owned(),
            prerequisites,
            state_root,
            RecordingRunner::default(),
        )
        .is_err());
    }

    #[test]
    fn wsl_start_requires_a_persistent_matching_silo_binding() {
        let prerequisites = WslChromiumPrerequisites {
            supported_platform: true,
            wsl_available: true,
            discovered_distributions: vec!["Ubuntu".to_owned()],
            guest_agent_distributions: vec!["Ubuntu".to_owned()],
            gui_distributions: vec!["Ubuntu".to_owned()],
            expected_agent_sha256: "b".repeat(64),
        };
        let mut backend = WslChromiumBackend::new(
            "Ubuntu".to_owned(),
            prerequisites,
            temporary_root("wsl-binding"),
            RecordingRunner::default(),
        )
        .expect("backend");
        let environment_id = Uuid::new_v4();
        let error = backend
            .start(EnvironmentRequest { environment_id })
            .expect_err("must fail closed");
        assert!(error.to_string().contains("filesystem"));
    }

    #[test]
    fn wsl_action_receipt_requires_fresh_exact_agent_identity() {
        let prerequisites = WslChromiumPrerequisites {
            supported_platform: true,
            wsl_available: true,
            discovered_distributions: vec!["Ubuntu".to_owned()],
            guest_agent_distributions: vec!["Ubuntu".to_owned()],
            gui_distributions: vec!["Ubuntu".to_owned()],
            expected_agent_sha256: "c".repeat(64),
        };
        let backend = WslChromiumBackend::new(
            "Ubuntu".to_owned(),
            prerequisites,
            temporary_root("wsl-receipt"),
            RecordingRunner::default(),
        )
        .expect("backend");
        let environment_id = Uuid::new_v4();
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 1,
            "environmentId": environment_id,
            "source": "guest_agent",
            "action": "health",
            "state": "healthy",
            "agentVersion": WSL_GUEST_AGENT_VERSION,
            "agentSha256": "c".repeat(64),
            "observedAt": Utc::now().to_rfc3339(),
        }))
        .expect("response JSON");
        backend
            .validate_action_response(
                &bytes,
                environment_id,
                WslAgentAction::Health,
                WslAgentState::Healthy,
            )
            .expect("fresh exact receipt");
    }

    #[test]
    fn wsl_rejects_discovered_names_that_can_be_parsed_as_options() {
        let prerequisites = WslChromiumPrerequisites {
            supported_platform: true,
            wsl_available: true,
            discovered_distributions: vec!["--exec".to_owned()],
            guest_agent_distributions: vec!["--exec".to_owned()],
            gui_distributions: vec!["--exec".to_owned()],
            expected_agent_sha256: "d".repeat(64),
        };
        assert!(WslChromiumBackend::new(
            "--exec".to_owned(),
            prerequisites,
            temporary_root("wsl-option-name"),
            RecordingRunner::default(),
        )
        .is_err());
    }

    #[test]
    fn wsl_network_profile_requires_camel_case_proxy_required() {
        let parsed = serde_json::from_value::<EnvironmentNetworkProfile>(serde_json::json!({
            "mode": "fixed_proxy",
            "proxyRequired": true,
            "scheme": "socks5",
            "host": "127.0.0.1",
            "port": 7890,
        }))
        .expect("camelCase network profile");
        assert!(parsed.proxy_required());

        assert!(
            serde_json::from_value::<EnvironmentNetworkProfile>(serde_json::json!({
                "mode": "fixed_proxy",
                "proxy_required": true,
                "scheme": "socks5",
                "host": "127.0.0.1",
                "port": 7890,
            }))
            .is_err()
        );
    }

    #[test]
    fn wsl_required_proxy_rejects_stale_guest_evidence() {
        let environment_id = Uuid::new_v4();
        let runtime_id = Uuid::new_v4();
        let agent_sha256 = "e".repeat(64);
        let profile_path =
            WslChromiumBackend::<RecordingRunner>::guest_profile_path(environment_id);
        let evidence = GuestNetworkEvidence {
            schema_version: ENVIRONMENT_CONTRACT_VERSION,
            evidence_id: Uuid::new_v4(),
            environment_id,
            source: GuestEvidenceSource::GuestAgent,
            runtime_id,
            profile_path: profile_path.clone(),
            proxy_port: Some(7890),
            agent_sha256: agent_sha256.clone(),
            proxy: GuestEvidenceState::Verified,
            exit: GuestEvidenceState::Verified,
            proxy_dns: GuestEvidenceState::Verified,
            guest_resolver: GuestEvidenceState::Unavailable,
            observed_at: (Utc::now() - Duration::minutes(3)).to_rfc3339(),
            valid_until: (Utc::now() - Duration::minutes(1)).to_rfc3339(),
        };

        assert!(!evidence.validates_required_proxy(
            environment_id,
            runtime_id,
            &profile_path,
            7890,
            &agent_sha256,
        ));
    }

    #[test]
    fn wsl_required_proxy_binds_runtime_profile_port_hash_and_split_dns_exit() {
        let environment_id = Uuid::new_v4();
        let runtime_id = Uuid::new_v4();
        let profile_path =
            WslChromiumBackend::<RecordingRunner>::guest_profile_path(environment_id);
        let agent_sha256 = "9".repeat(64);
        let observed = Utc::now();
        let evidence = GuestNetworkEvidence {
            schema_version: ENVIRONMENT_CONTRACT_VERSION,
            evidence_id: Uuid::new_v4(),
            environment_id,
            source: GuestEvidenceSource::GuestAgent,
            runtime_id,
            profile_path: profile_path.clone(),
            proxy_port: Some(7890),
            agent_sha256: agent_sha256.clone(),
            proxy: GuestEvidenceState::Verified,
            exit: GuestEvidenceState::Verified,
            proxy_dns: GuestEvidenceState::Verified,
            guest_resolver: GuestEvidenceState::Unavailable,
            observed_at: observed.to_rfc3339(),
            valid_until: (observed + Duration::minutes(2)).to_rfc3339(),
        };
        assert!(evidence.validates_required_proxy(
            environment_id,
            runtime_id,
            &profile_path,
            7890,
            &agent_sha256,
        ));
        assert!(!evidence.validates_required_proxy(
            Uuid::new_v4(),
            runtime_id,
            &profile_path,
            7890,
            &agent_sha256,
        ));
        assert!(!evidence.validates_required_proxy(
            environment_id,
            Uuid::new_v4(),
            &profile_path,
            7890,
            &agent_sha256,
        ));

        let mut split = evidence.clone();
        split.exit = GuestEvidenceState::Failed;
        assert!(!split.validates_required_proxy(
            environment_id,
            runtime_id,
            &profile_path,
            7890,
            &agent_sha256,
        ));
        split.exit = GuestEvidenceState::Verified;
        split.proxy_dns = GuestEvidenceState::Failed;
        assert!(!split.validates_required_proxy(
            environment_id,
            runtime_id,
            &profile_path,
            7890,
            &agent_sha256,
        ));
        split.proxy_dns = GuestEvidenceState::Verified;
        split.guest_resolver = GuestEvidenceState::Verified;
        assert!(!split.validates_required_proxy(
            environment_id,
            runtime_id,
            &profile_path,
            7890,
            &agent_sha256,
        ));
    }

    #[test]
    fn wsl_direct_evidence_rejects_proxy_claims_and_noncanonical_agent_hashes() {
        let environment_id = Uuid::new_v4();
        let runtime_id = Uuid::new_v4();
        let agent_sha256 = "a".repeat(64);
        let mut output = wsl_network_output(
            environment_id,
            runtime_id,
            &agent_sha256,
            None,
            "verified",
            "not_requested",
            "not_requested",
        );
        let state_root = temporary_root("wsl-direct-evidence-state");
        let mut backend = WslChromiumBackend::new(
            "Ubuntu".to_owned(),
            wsl_prerequisites(&agent_sha256),
            state_root.clone(),
            RecordingRunner {
                output: Some(output.clone()),
                ..RecordingRunner::default()
            },
        )
        .expect("backend");
        backend
            .configure_network(ConfigureNetworkRequest {
                environment_id,
                runtime_id,
                network: EnvironmentNetworkProfile::Direct,
            })
            .expect_err("DIRECT must reject invented proxy evidence");
        assert!(!binding_path(&state_root, environment_id).exists());

        let mut response: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("network fixture");
        response["agentSha256"] = serde_json::json!("A".repeat(64));
        response["evidence"]["agentSha256"] = serde_json::json!("A".repeat(64));
        output.stdout = serde_json::to_vec(&response).expect("mutated network fixture");
        backend.runner.output = Some(output);
        backend
            .configure_network(ConfigureNetworkRequest {
                environment_id,
                runtime_id,
                network: EnvironmentNetworkProfile::Direct,
            })
            .expect_err("agent hash serialization must be canonical lowercase");
    }

    #[test]
    fn wsl_rejects_non_loopback_or_non_socks_guest_evidence_profiles() {
        for network in [
            EnvironmentNetworkProfile::FixedProxy {
                proxy_required: true,
                scheme: ProxyScheme::Http,
                host: "127.0.0.1".to_owned(),
                port: 7890,
            },
            EnvironmentNetworkProfile::FixedProxy {
                proxy_required: true,
                scheme: ProxyScheme::Socks5,
                host: "192.0.2.10".to_owned(),
                port: 7890,
            },
        ] {
            assert!(
                WslChromiumBackend::<RecordingRunner>::validate_guest_network_profile(&network)
                    .is_err()
            );
        }
    }

    #[test]
    fn wsl_configure_detach_closes_local_artifact_lifecycle_without_unregistering_distribution() {
        let environment_root = temporary_root("wsl-detach-lifecycle");
        let state_root = environment_root.join("wsl");
        let environment_id = Uuid::new_v4();
        let runtime_id = Uuid::new_v4();
        let agent_sha256 = "e".repeat(64);
        let mut backend = WslChromiumBackend::new(
            "Ubuntu".to_owned(),
            wsl_prerequisites(&agent_sha256),
            state_root,
            RecordingRunner {
                output: Some(wsl_network_output(
                    environment_id,
                    runtime_id,
                    &agent_sha256,
                    None,
                    "not_requested",
                    "not_requested",
                    "not_requested",
                )),
                ..RecordingRunner::default()
            },
        )
        .expect("backend");

        backend
            .configure_network(ConfigureNetworkRequest {
                environment_id,
                runtime_id,
                network: EnvironmentNetworkProfile::Direct,
            })
            .expect("configure direct");
        assert_eq!(
            local_environment_artifacts(&environment_root, environment_id)
                .expect("inspect local artifacts"),
            vec![EnvironmentBackendId::WslChromium]
        );
        assert!(backend
            .destroy(DestroyEnvironmentRequest {
                environment_id,
                confirm_destroy: false,
            })
            .is_err());

        backend.runner.output = Some(wsl_action_output(
            environment_id,
            &agent_sha256,
            "detach",
            "destroyed",
        ));
        backend
            .destroy(DestroyEnvironmentRequest {
                environment_id,
                confirm_destroy: true,
            })
            .expect("detach profile");
        assert!(
            local_environment_artifacts(&environment_root, environment_id)
                .expect("inspect after detach")
                .is_empty()
        );

        let spec = backend.runner.specs.last().expect("detach command");
        assert_eq!(spec.args[6], "detach");
        assert!(!spec.args.iter().any(|argument| argument == "--unregister"));
        let request: serde_json::Value =
            serde_json::from_slice(spec.stdin.as_deref().expect("confirmation payload"))
                .expect("strict detach JSON");
        assert_eq!(request["environmentId"], serde_json::json!(environment_id));
        assert_eq!(request["confirmDestroy"], serde_json::json!(true));
    }

    #[test]
    fn legacy_cleanup_discovers_only_exact_uuid_bound_provider_owners() {
        let environment_root = temporary_root("legacy-binding-owner");
        let unbound_id = Uuid::new_v4();
        assert_eq!(
            local_environment_binding_provider(
                &environment_root,
                unbound_id,
                EnvironmentBackendId::WslChromium,
            )
            .expect("missing legacy owner"),
            None
        );

        let bound_id = Uuid::new_v4();
        ensure_binding(
            &environment_root.join("wsl"),
            &EnvironmentBinding::new(
                bound_id,
                EnvironmentBackendId::WslChromium,
                "Ubuntu-24.04".to_owned(),
            ),
        )
        .expect("write exact WSL owner");
        assert_eq!(
            local_environment_binding_provider(
                &environment_root,
                bound_id,
                EnvironmentBackendId::WslChromium,
            )
            .expect("read exact WSL owner"),
            Some("Ubuntu-24.04".to_owned())
        );

        let partial_id = Uuid::new_v4();
        fs::create_dir_all(environment_root.join("wsl").join(partial_id.to_string()))
            .expect("create partial legacy owner");
        assert!(local_environment_binding_provider(
            &environment_root,
            partial_id,
            EnvironmentBackendId::WslChromium,
        )
        .is_err());

        let wrong_backend_id = Uuid::new_v4();
        ensure_binding(
            &environment_root.join("wsl"),
            &EnvironmentBinding::new(
                wrong_backend_id,
                EnvironmentBackendId::WindowsSandbox,
                "windows-sandbox-v0.8-ephemeral".to_owned(),
            ),
        )
        .expect("write mismatched provider owner");
        assert!(local_environment_binding_provider(
            &environment_root,
            wrong_backend_id,
            EnvironmentBackendId::WslChromium,
        )
        .is_err());

        fs::remove_dir_all(environment_root).expect("remove legacy owner root");
    }

    #[test]
    fn wsl_failed_required_proxy_reconfigure_invalidates_old_direct_start_binding() {
        let environment_id = Uuid::new_v4();
        let direct_runtime_id = Uuid::new_v4();
        let required_runtime_id = Uuid::new_v4();
        let agent_sha256 = "f".repeat(64);
        let state_root = temporary_root("wsl-stale-binding");
        let mut backend = WslChromiumBackend::new(
            "Ubuntu".to_owned(),
            wsl_prerequisites(&agent_sha256),
            state_root.clone(),
            RecordingRunner {
                output: Some(wsl_network_output(
                    environment_id,
                    direct_runtime_id,
                    &agent_sha256,
                    None,
                    "not_requested",
                    "not_requested",
                    "not_requested",
                )),
                ..RecordingRunner::default()
            },
        )
        .expect("backend");
        backend
            .configure_network(ConfigureNetworkRequest {
                environment_id,
                runtime_id: direct_runtime_id,
                network: EnvironmentNetworkProfile::Direct,
            })
            .expect("initial direct binding");
        require_binding(&state_root, &backend.binding(environment_id))
            .expect("direct binding exists");

        backend.runner.output = Some(wsl_network_output(
            environment_id,
            required_runtime_id,
            &agent_sha256,
            Some(7890),
            "verified",
            "verified",
            "unavailable",
        ));
        let error = backend
            .configure_network(ConfigureNetworkRequest {
                environment_id,
                runtime_id: required_runtime_id,
                network: EnvironmentNetworkProfile::FixedProxy {
                    proxy_required: true,
                    scheme: ProxyScheme::Socks5,
                    host: "127.0.0.1".to_owned(),
                    port: 7890,
                },
            })
            .expect_err("DNS-unverified required proxy must fail");
        assert!(error.to_string().contains("proxy DNS and exit evidence"));
        assert!(!binding_path(&state_root, environment_id).exists());

        backend.runner.output = Some(wsl_action_output(
            environment_id,
            &agent_sha256,
            "start",
            "started",
        ));
        let command_count = backend.runner.specs.len();
        backend
            .start(EnvironmentRequest { environment_id })
            .expect_err("Start must not reuse the old DIRECT binding");
        assert_eq!(backend.runner.specs.len(), command_count);
    }

    #[test]
    fn sandbox_xml_denies_integrations_and_escapes_read_only_mapping() {
        let environment_id = Uuid::new_v4();
        let host_folder = temporary_root("sandbox-xml").join("A&B<source>");
        let host_text = host_folder.to_str().expect("Unicode test path");
        let xml = generate_sandbox_config(
            environment_id,
            &[SandboxMappedFolder {
                host_folder: host_folder.clone(),
                sandbox_folder: "C:\\Read&Only".to_owned(),
                read_only: true,
            }],
            true,
        )
        .expect("safe config");
        for denied in [
            "<VGpu>Disable</VGpu>",
            "<AudioInput>Disable</AudioInput>",
            "<VideoInput>Disable</VideoInput>",
            "<PrinterRedirection>Disable</PrinterRedirection>",
            "<ClipboardRedirection>Disable</ClipboardRedirection>",
            "<ProtectedClient>Enable</ProtectedClient>",
            "<MemoryInMB>4096</MemoryInMB>",
            "<ReadOnly>true</ReadOnly>",
        ] {
            assert!(xml.contains(denied), "missing {denied}");
        }
        assert!(xml.contains(&xml_escape(host_text)));
        assert!(xml.contains("C:\\Read&amp;Only"));
        assert!(!xml.contains(host_text));
    }

    #[test]
    fn sandbox_rejects_writable_mapping_and_has_explicit_lifecycle_gaps() {
        let result = generate_sandbox_config(
            Uuid::new_v4(),
            &[SandboxMappedFolder {
                host_folder: PathBuf::from("/tmp/source"),
                sandbox_folder: "C:\\Source".to_owned(),
                read_only: false,
            }],
            true,
        );
        assert!(result.is_err());

        let root = temporary_root("sandbox");
        let bootstrap = temporary_root("bootstrap");
        let backend = WindowsSandboxBackend::new(
            true,
            true,
            root,
            bootstrap.clone(),
            RecordingRunner::default(),
        )
        .expect("backend");
        let status = backend.status();
        for operation in [EnvironmentOperation::Pause, EnvironmentOperation::Snapshot] {
            let capability = status
                .capabilities
                .iter()
                .find(|capability| capability.operation == operation)
                .expect("capability");
            assert!(matches!(
                capability.availability,
                OperationAvailability::Unavailable { .. }
            ));
        }
    }

    #[test]
    fn sandbox_controller_uses_fixed_powershell_script_and_typed_paths() {
        let root = temporary_root("sandbox-command");
        let bootstrap = temporary_root("sandbox-command-bootstrap");
        let backend = WindowsSandboxBackend::new_with_system_tool_resolver(
            true,
            true,
            root,
            bootstrap.clone(),
            fixture_provider_system_tool,
            RecordingRunner::default(),
        )
        .expect("backend");
        let request = temporary_root("sandbox-command-request").join("request.json");
        let spec = backend.command_spec(&request).expect("trusted executable");
        assert_fixed_system_tool(&spec.program, "powershell.exe");
        assert!(spec
            .args
            .contains(&bootstrap.join("verisilo-sandbox.ps1").into_os_string()));
        assert!(spec.args.contains(&"-RequestPath".into()));
        assert!(spec.args.contains(&request.into_os_string()));
        assert!(spec.args.contains(&"-SandboxExecutable".into()));
        assert!(spec.args.contains(
            &fixture_provider_system_tool(ProviderSystemTool::WindowsSandbox)
                .expect("fixture Sandbox executable")
                .into_os_string()
        ));
        assert!(!spec.args.iter().any(|argument| argument == "-Command"));
        assert_eq!(spec.completion, CommandCompletion::WaitForExit);
    }

    #[test]
    fn sandbox_descriptor_recovers_after_backend_reconstruction_and_destroy_retries() {
        let root = temporary_root("sandbox-recovery");
        let bootstrap = temporary_root("sandbox-recovery-bootstrap");
        fs::create_dir_all(&bootstrap).expect("bootstrap root");
        fs::write(
            bootstrap.join("verisilo-sandbox-bootstrap.ps1"),
            b"# fixed signed fixture",
        )
        .expect("bootstrap fixture");
        let environment_id = Uuid::new_v4();
        let request = CreateEnvironmentRequest {
            environment_id,
            network: EnvironmentNetworkProfile::Direct,
        };
        let mut first = WindowsSandboxBackend::new_with_system_tool_resolver(
            true,
            true,
            root.clone(),
            bootstrap.clone(),
            fixture_provider_system_tool,
            RecordingRunner::default(),
        )
        .expect("first backend");
        first.create(request.clone()).expect("create descriptor");
        first.create(request).expect("idempotent create");

        let mut recovered = WindowsSandboxBackend::new_with_system_tool_resolver(
            true,
            true,
            root,
            bootstrap,
            fixture_provider_system_tool,
            RecordingRunner {
                output: Some(sandbox_output(
                    environment_id,
                    "start",
                    "running",
                    Some(4200),
                )),
                ..RecordingRunner::default()
            },
        )
        .expect("reconstructed backend");
        recovered
            .start(EnvironmentRequest { environment_id })
            .expect("start from persisted descriptor");
        let destroy = DestroyEnvironmentRequest {
            environment_id,
            confirm_destroy: true,
        };
        recovered.runner.output = Some(sandbox_output(
            environment_id,
            "assert-exited",
            "exited",
            None,
        ));
        recovered.destroy(destroy.clone()).expect("first destroy");
        recovered.destroy(destroy).expect("idempotent destroy");
    }

    #[test]
    fn sandbox_rejects_even_optional_fixed_proxy_instead_of_falling_back() {
        let root = temporary_root("sandbox-no-fallback");
        let bootstrap = temporary_root("sandbox-no-fallback-bootstrap");
        let mut backend =
            WindowsSandboxBackend::new(true, true, root, bootstrap, RecordingRunner::default())
                .expect("backend");
        let error = backend
            .create(CreateEnvironmentRequest {
                environment_id: Uuid::new_v4(),
                network: EnvironmentNetworkProfile::FixedProxy {
                    proxy_required: false,
                    scheme: ProxyScheme::Http,
                    host: "127.0.0.1".to_owned(),
                    port: 8080,
                },
            })
            .expect_err("optional fixed proxy must not become direct");
        assert!(error.to_string().contains("never falls back"));
    }

    #[test]
    fn hyperv_uses_fixed_script_and_request_file_not_command_text() {
        let root = temporary_root("hyperv");
        let images = temporary_root("hyperv-images");
        let script = temporary_root("scripts").join("verisilo-hyperv.ps1");
        let backend = HyperVBackend::new(
            HyperVPrerequisites {
                supported_platform: true,
                supported_sku: true,
                administrator: true,
                virtualization_enabled: true,
                hyperv_enabled: true,
                reboot_required: false,
                release_scripts_trusted: true,
            },
            root,
            images,
            script.clone(),
            Some(ValidatedHyperVImage {
                file_name: "windows-11-base.vhdx".to_owned(),
                sha256: "a".repeat(64),
                verified: true,
            }),
            RecordingRunner::default(),
        )
        .expect("backend");
        let request = temporary_root("request").join("request.json");
        let environment_id = Uuid::new_v4();
        let request_nonce = Uuid::new_v4();
        let spec = backend
            .command_spec(
                &request,
                HyperVAction::Health,
                environment_id,
                request_nonce,
            )
            .expect("command spec");
        assert_fixed_system_tool(&spec.program, "powershell.exe");
        assert!(spec.args.contains(&script.into_os_string()));
        assert!(spec.args.contains(&"-RequestPath".into()));
        assert!(spec.args.contains(&"-ExpectedEnvironmentId".into()));
        assert!(spec.args.contains(&environment_id.to_string().into()));
        assert!(spec.args.contains(&"-ExpectedAction".into()));
        assert!(spec.args.contains(&"health".into()));
        assert!(spec.args.contains(&"-ExpectedRequestNonce".into()));
        assert!(spec.args.contains(&request_nonce.to_string().into()));
        assert!(!spec.args.iter().any(|argument| argument == "-Command"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn hyperv_request_lease_blocks_write_delete_rename_and_parent_replacement() {
        let fixture_root = temporary_root("hyperv-request-lease");
        let app_data_root = fixture_root.join("app-data");
        let verisilo_root = app_data_root.join("VeriSilo");
        let environments_root = verisilo_root.join("environments");
        let state_root = environments_root.join("hyperv");
        let environment_id = Uuid::new_v4();
        let request_nonce = Uuid::new_v4();
        let request_parent = state_root.join(environment_id.to_string());
        fs::create_dir_all(&request_parent).expect("request parent");
        let request_path = request_parent.join(format!("{request_nonce}.request.json"));
        let replacement = request_parent.join("replacement.request.json");
        let renamed_parent = state_root.join("renamed-environment");
        let renamed_root = environments_root.join("renamed-hyperv");
        let renamed_environments = verisilo_root.join("renamed-environments");
        let renamed_app_data = fixture_root.join("renamed-app-data");
        let bytes = br#"{"schemaVersion":1}"#;

        let lease = HyperVRequestLease::create(&state_root, &request_parent, &request_path, bytes)
            .expect("locked request");
        assert_eq!(
            fs::read(&request_path).expect("request remains readable"),
            bytes
        );
        assert!(
            fs::OpenOptions::new()
                .write(true)
                .open(&request_path)
                .is_err(),
            "a second writer must be denied while the elevated operation runs"
        );
        assert!(
            fs::remove_file(&request_path).is_err(),
            "request deletion must be denied while leased"
        );
        assert!(
            fs::rename(&request_path, &replacement).is_err(),
            "request rename must be denied while leased"
        );
        assert!(
            fs::rename(&request_parent, &renamed_parent).is_err(),
            "the exact request parent must not be replaceable while leased"
        );
        assert!(
            fs::rename(&state_root, &renamed_root).is_err(),
            "the state root must not be replaceable while leased"
        );
        assert!(
            fs::rename(&environments_root, &renamed_environments).is_err(),
            "an ancestor of the state root must not be replaceable while leased"
        );
        assert!(
            fs::rename(&app_data_root, &renamed_app_data).is_err(),
            "the app-data anchor must not be replaceable while leased"
        );

        drop(lease);
        fs::remove_file(&request_path).expect("request is removable after lease release");
        fs::remove_dir_all(&fixture_root).expect("request fixture cleanup");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn hyperv_request_lease_rejects_reparse_ancestors_and_prepositioned_files() {
        use std::os::windows::fs::{symlink_dir, symlink_file};

        let fixture_root = temporary_root("hyperv-request-reparse");
        let target_root = fixture_root.join("target");
        let link_root = fixture_root.join("linked-state");
        let environment_id = Uuid::new_v4();
        let request_nonce = Uuid::new_v4();
        let target_parent = target_root.join(environment_id.to_string());
        fs::create_dir_all(&target_parent).expect("reparse target");

        if symlink_dir(&target_root, &link_root).is_ok() {
            let linked_parent = link_root.join(environment_id.to_string());
            let linked_request = linked_parent.join(format!("{request_nonce}.request.json"));
            assert!(
                HyperVRequestLease::create(&link_root, &linked_parent, &linked_request, b"trusted")
                    .is_err(),
                "a reparse-point ancestor must be rejected"
            );
            fs::remove_dir(&link_root).expect("remove directory symlink");
        }

        let request_path = target_parent.join(format!("{request_nonce}.request.json"));
        let attacker_file = fixture_root.join("attacker-request.json");
        fs::write(&attacker_file, b"attacker").expect("attacker file");
        if symlink_file(&attacker_file, &request_path).is_ok() {
            assert!(
                HyperVRequestLease::create(&target_root, &target_parent, &request_path, b"trusted")
                    .is_err(),
                "a prepositioned reparse-point request must be rejected"
            );
            fs::remove_file(&request_path).expect("remove request symlink");
        }

        fs::remove_dir_all(&fixture_root).expect("remove reparse fixture");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn hyperv_image_lease_hashes_the_locked_handle_and_blocks_path_replacement() {
        let fixture_root = temporary_root("hyperv-image-lease");
        let app_data_root = fixture_root.join("app-data");
        let approved_root = app_data_root.join("VeriSilo").join("images");
        fs::create_dir_all(&approved_root).expect("approved image root");
        let image_path = approved_root.join("base.vhdx");
        fs::write(&image_path, b"abc").expect("base image fixture");
        let expected_sha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

        let lease = HyperVImageLease::acquire(&approved_root, "base.vhdx", expected_sha256)
            .expect("locked and hashed image");
        assert!(
            fs::OpenOptions::new()
                .write(true)
                .open(&image_path)
                .is_err(),
            "a second writer must not modify the verified base image"
        );
        assert!(
            fs::remove_file(&image_path).is_err(),
            "the verified base image must not be deleted"
        );
        assert!(
            fs::rename(&image_path, approved_root.join("replacement.vhdx")).is_err(),
            "the verified base image must not be renamed"
        );
        assert!(
            fs::rename(&approved_root, app_data_root.join("replacement-images")).is_err(),
            "the approved image root must remain path-bound"
        );
        assert!(
            fs::rename(&app_data_root, fixture_root.join("replacement-app-data")).is_err(),
            "an ancestor of the image root must remain path-bound"
        );

        drop(lease);
        assert!(
            HyperVImageLease::acquire(&approved_root, "base.vhdx", &"0".repeat(64)).is_err(),
            "the SHA-256 must be calculated from and bound to the held image handle"
        );
        fs::remove_dir_all(&fixture_root).expect("remove image fixture");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn hyperv_image_lease_rejects_reparse_roots_and_image_files() {
        use std::os::windows::fs::{symlink_dir, symlink_file};

        let fixture_root = temporary_root("hyperv-image-reparse");
        let target_root = fixture_root.join("target-images");
        let link_root = fixture_root.join("linked-images");
        fs::create_dir_all(&target_root).expect("image target root");
        let target_image = target_root.join("base.vhdx");
        fs::write(&target_image, b"abc").expect("target image");
        let expected_sha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

        if symlink_dir(&target_root, &link_root).is_ok() {
            assert!(
                HyperVImageLease::acquire(&link_root, "base.vhdx", expected_sha256).is_err(),
                "a reparse-point image root must be rejected"
            );
            fs::remove_dir(&link_root).expect("remove image-root symlink");
        }

        let attacker_image = fixture_root.join("attacker.vhdx");
        fs::write(&attacker_image, b"abc").expect("attacker image");
        let linked_image = target_root.join("linked.vhdx");
        if symlink_file(&attacker_image, &linked_image).is_ok() {
            assert!(
                HyperVImageLease::acquire(&target_root, "linked.vhdx", expected_sha256).is_err(),
                "a reparse-point image file must be rejected"
            );
            fs::remove_file(&linked_image).expect("remove image symlink");
        }

        fs::remove_dir_all(&fixture_root).expect("remove image reparse fixture");
    }

    #[test]
    fn hyperv_rejects_traversal_and_keeps_unverified_images_create_gated() {
        for file_name in ["..\\outside.vhdx", "base.vhdx:stream", "CON.vhdx"] {
            let invalid = ValidatedHyperVImage {
                file_name: file_name.to_owned(),
                sha256: "0".repeat(64),
                verified: true,
            };
            assert!(validate_image_descriptor(&invalid).is_err());
        }
        assert!(validate_image_descriptor(&ValidatedHyperVImage {
            file_name: "base.vhdx".to_owned(),
            sha256: "A".repeat(64),
            verified: true,
        })
        .is_err());
        let invalid = ValidatedHyperVImage {
            file_name: "base.vhdx".to_owned(),
            sha256: "0".repeat(64),
            verified: false,
        };
        assert!(validate_image_descriptor(&invalid).is_ok());
        let backend = HyperVBackend::new(
            HyperVPrerequisites {
                supported_platform: true,
                supported_sku: true,
                administrator: true,
                virtualization_enabled: true,
                hyperv_enabled: true,
                reboot_required: false,
                release_scripts_trusted: true,
            },
            temporary_root("hyperv-unverified"),
            temporary_root("hyperv-unverified-images"),
            temporary_root("hyperv-unverified-script"),
            Some(invalid),
            RecordingRunner::default(),
        )
        .expect("control backend");
        let create = backend
            .status()
            .capabilities
            .into_iter()
            .find(|capability| capability.operation == EnvironmentOperation::Create)
            .expect("create capability");
        assert!(matches!(
            create.availability,
            OperationAvailability::Unavailable { .. }
        ));
    }

    #[test]
    fn hyperv_response_is_strict_and_bound_to_a_typed_action() {
        let environment_id = Uuid::new_v4();
        let request_nonce = Uuid::new_v4();
        let response: HyperVScriptResponse = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "action": "health",
            "environmentId": environment_id,
            "requestNonce": request_nonce,
            "success": true,
            "source": "hyperv-controller",
            "observedAt": Utc::now().to_rfc3339(),
            "vmName": format!("VeriSilo-{environment_id}"),
            "vmId": Uuid::new_v4(),
            "generation": 2,
            "baseImageSha256": "a".repeat(64),
            "guestAgentVersion": null,
            "guestAgentSha256": null,
            "guestProfile": "unavailable",
            "guestHealth": "unavailable",
            "proxy": "unavailable",
            "exit": "unavailable",
            "proxyDns": "unavailable",
            "guestResolver": "unavailable",
            "browserReady": "unavailable",
        }))
        .expect("strict response");
        assert_eq!(response.action, HyperVAction::Health);
        assert!(response.vm_id.is_some());
        assert!(response.cleanup_state.is_none());
        assert!(hyperv_response_identity_is_valid(
            HyperVAction::Health,
            &response
        ));
        assert!(
            serde_json::from_value::<HyperVScriptResponse>(serde_json::json!({
                "schemaVersion": 1,
                "action": "health",
                "environmentId": environment_id,
                "requestNonce": request_nonce,
                "success": true,
                "command": "Get-VM",
            }))
            .is_err()
        );
    }

    #[test]
    fn hyperv_remove_accepts_only_typed_receipt_or_journal_cleanup_identity() {
        let environment_id = Uuid::new_v4();
        let request_nonce = Uuid::new_v4();
        let response_value =
            |vm_id: serde_json::Value, cleanup_state: Option<&str>| -> serde_json::Value {
                let mut response = serde_json::json!({
                    "schemaVersion": 1,
                    "action": "remove",
                    "environmentId": environment_id,
                    "requestNonce": request_nonce,
                    "success": true,
                    "source": "hyperv-controller",
                    "observedAt": Utc::now().to_rfc3339(),
                    "vmName": format!("VeriSilo-{environment_id}"),
                    "vmId": vm_id,
                    "generation": 2,
                    "baseImageSha256": "a".repeat(64),
                    "guestAgentVersion": null,
                    "guestAgentSha256": null,
                    "guestProfile": "unavailable",
                    "guestHealth": "unavailable",
                    "proxy": "unavailable",
                    "exit": "unavailable",
                    "proxyDns": "unavailable",
                    "guestResolver": "unavailable",
                    "browserReady": "unavailable",
                });
                if let Some(cleanup_state) = cleanup_state {
                    response
                        .as_object_mut()
                        .expect("response object")
                        .insert("cleanupState".to_owned(), serde_json::json!(cleanup_state));
                }
                response
            };

        let receipt: HyperVScriptResponse = serde_json::from_value(response_value(
            serde_json::json!(Uuid::new_v4()),
            Some("removed_from_receipt"),
        ))
        .expect("receipt cleanup response");
        assert!(hyperv_response_identity_is_valid(
            HyperVAction::Remove,
            &receipt
        ));

        let journal_without_vm: HyperVScriptResponse = serde_json::from_value(response_value(
            serde_json::Value::Null,
            Some("rolled_back_from_journal"),
        ))
        .expect("partial journal cleanup response");
        assert!(hyperv_response_identity_is_valid(
            HyperVAction::Remove,
            &journal_without_vm
        ));

        let missing_cleanup: HyperVScriptResponse =
            serde_json::from_value(response_value(serde_json::Value::Null, None))
                .expect("shape remains parseable for semantic rejection");
        assert!(!hyperv_response_identity_is_valid(
            HyperVAction::Remove,
            &missing_cleanup
        ));

        let receipt_without_vm: HyperVScriptResponse = serde_json::from_value(response_value(
            serde_json::Value::Null,
            Some("removed_from_receipt"),
        ))
        .expect("shape remains parseable for semantic rejection");
        assert!(!hyperv_response_identity_is_valid(
            HyperVAction::Remove,
            &receipt_without_vm
        ));

        let nil_journal_vm: HyperVScriptResponse = serde_json::from_value(response_value(
            serde_json::json!(Uuid::nil()),
            Some("rolled_back_from_journal"),
        ))
        .expect("shape remains parseable for semantic rejection");
        assert!(!hyperv_response_identity_is_valid(
            HyperVAction::Remove,
            &nil_journal_vm
        ));
    }
}
