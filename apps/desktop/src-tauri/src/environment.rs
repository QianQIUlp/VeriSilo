use std::path::PathBuf;

#[cfg(target_os = "windows")]
use std::ffi::OsString;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(target_os = "windows")]
use crate::domain::{trusted_windows_system_tool, WindowsSystemTool};

#[path = "environment_backend.rs"]
pub mod backend;

use backend::{
    ConfigureNetworkRequest, CreateEnvironmentRequest, DestroyEnvironmentRequest,
    EnvironmentActionReceipt, EnvironmentBackend, EnvironmentBackendError, EnvironmentBackendId,
    EnvironmentBackendStatus, EnvironmentCapability, EnvironmentNetworkProfile,
    EnvironmentOperation, EnvironmentPrerequisite, EnvironmentRequest, HyperVBackend,
    HyperVPrerequisites, OperationAvailability, PrerequisiteState, SystemProcessRunner,
    ValidatedHyperVImage, WindowsSandboxBackend, WslChromiumBackend, WslChromiumPrerequisites,
    ENVIRONMENT_CONTRACT_VERSION,
};

#[cfg(target_os = "windows")]
use backend::{
    CommandCompletion, CommandSpec, ProcessRunner, WSL_GUEST_AGENT_PATH, WSL_GUEST_AGENT_VERSION,
};

#[cfg(target_os = "windows")]
const MAX_PROBE_OUTPUT_BYTES: usize = 16 * 1024;

#[cfg(target_os = "windows")]
const EMBEDDED_WSL_GUEST_AGENT: &[u8] =
    include_bytes!("../../../../scripts/verisilo-wsl-guest-agent.sh");

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WslStatus {
    pub supported_platform: bool,
    pub available: bool,
    pub distributions: Vec<String>,
    pub message: String,
}

#[cfg(target_os = "windows")]
pub fn detect_wsl() -> WslStatus {
    let Some(status) = run_fixed_windows_process(WindowsSystemTool::Wsl, &["--status"], None, &[])
    else {
        return WslStatus {
            supported_platform: true,
            available: false,
            distributions: Vec::new(),
            message: "Windows 未找到 wsl.exe；VeriSilo 没有修改系统功能。".to_owned(),
        };
    };
    if !status.success {
        return WslStatus {
            supported_platform: true,
            available: false,
            distributions: Vec::new(),
            message: "WSL 当前不可用；VeriSilo 只做了只读检查。".to_owned(),
        };
    }

    let distributions =
        run_fixed_windows_process(WindowsSystemTool::Wsl, &["--list", "--quiet"], None, &[])
            .filter(|output| output.success)
            .map(|output| decode_windows_output(&output.stdout))
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .filter(|line| backend::validate_distribution_name(line).is_ok())
            .map(str::to_owned)
            .collect::<Vec<_>>();
    WslStatus {
        supported_platform: true,
        available: true,
        message: if distributions.is_empty() {
            "WSL 可用，但未发现已安装的 Linux 发行版。".to_owned()
        } else {
            format!("WSL 可用，发现 {} 个 Linux 发行版。", distributions.len())
        },
        distributions,
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum EnvironmentOperationRequest {
    Create {
        backend: EnvironmentBackendId,
        environment_id: Uuid,
        network: EnvironmentNetworkProfile,
    },
    Start {
        backend: EnvironmentBackendId,
        environment_id: Uuid,
    },
    Stop {
        backend: EnvironmentBackendId,
        environment_id: Uuid,
    },
    Pause {
        backend: EnvironmentBackendId,
        environment_id: Uuid,
    },
    Snapshot {
        backend: EnvironmentBackendId,
        environment_id: Uuid,
    },
    Destroy {
        backend: EnvironmentBackendId,
        environment_id: Uuid,
        confirm_destroy: bool,
    },
    ConfigureNetwork {
        backend: EnvironmentBackendId,
        environment_id: Uuid,
        network: EnvironmentNetworkProfile,
    },
    Health {
        backend: EnvironmentBackendId,
        environment_id: Uuid,
    },
    Logs {
        backend: EnvironmentBackendId,
        environment_id: Uuid,
    },
}

impl EnvironmentOperationRequest {
    fn backend(&self) -> EnvironmentBackendId {
        match self {
            Self::Create { backend, .. }
            | Self::Start { backend, .. }
            | Self::Stop { backend, .. }
            | Self::Pause { backend, .. }
            | Self::Snapshot { backend, .. }
            | Self::Destroy { backend, .. }
            | Self::ConfigureNetwork { backend, .. }
            | Self::Health { backend, .. }
            | Self::Logs { backend, .. } => *backend,
        }
    }

    fn operation(&self) -> EnvironmentOperation {
        match self {
            Self::Create { .. } => EnvironmentOperation::Create,
            Self::Start { .. } => EnvironmentOperation::Start,
            Self::Stop { .. } => EnvironmentOperation::Stop,
            Self::Pause { .. } => EnvironmentOperation::Pause,
            Self::Snapshot { .. } => EnvironmentOperation::Snapshot,
            Self::Destroy { .. } => EnvironmentOperation::Destroy,
            Self::ConfigureNetwork { .. } => EnvironmentOperation::ConfigureNetwork,
            Self::Health { .. } => EnvironmentOperation::Health,
            Self::Logs { .. } => EnvironmentOperation::Logs,
        }
    }

    pub fn environment_id(&self) -> Uuid {
        match self {
            Self::Create { environment_id, .. }
            | Self::Start { environment_id, .. }
            | Self::Stop { environment_id, .. }
            | Self::Pause { environment_id, .. }
            | Self::Snapshot { environment_id, .. }
            | Self::Destroy { environment_id, .. }
            | Self::ConfigureNetwork { environment_id, .. }
            | Self::Health { environment_id, .. }
            | Self::Logs { environment_id, .. } => *environment_id,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WindowsEnvironmentProbe {
    schema_version: u32,
    supported_sku: bool,
    administrator: bool,
    virtualization_enabled: bool,
    hyperv_enabled: bool,
    reboot_required: bool,
    sandbox_available: bool,
    release_scripts_trusted: bool,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WslAgentIdentity {
    schema_version: u32,
    agent_version: String,
    sha256: String,
    owner_uid: u32,
    mode: String,
    path: String,
    browser_user: String,
    browser_uid: u32,
}

/// Owns only fixed provider implementations. No caller-controlled executable,
/// shell text, PowerShell fragment, or filesystem path crosses this boundary.
pub struct EnvironmentManager {
    root: PathBuf,
    resource_root: PathBuf,
    probe: WindowsEnvironmentProbe,
    wsl: Option<WslChromiumBackend<SystemProcessRunner>>,
    selected_wsl_distribution: Option<String>,
    sandbox: WindowsSandboxBackend<SystemProcessRunner>,
    hyperv: Option<HyperVBackend<SystemProcessRunner>>,
}

impl EnvironmentManager {
    pub fn new(root: PathBuf, resource_root: PathBuf) -> Result<Self, EnvironmentBackendError> {
        let probe = probe_windows_environment(&resource_root);
        let environment_root = root.join("environments");
        let scripts_root = resource_root.join("environment");
        let sandbox_bootstrap = scripts_root.join("verisilo-sandbox-bootstrap.ps1");
        let sandbox_controller = scripts_root.join("verisilo-sandbox.ps1");
        let sandbox = WindowsSandboxBackend::new(
            cfg!(target_os = "windows"),
            probe.sandbox_available
                && probe.release_scripts_trusted
                && sandbox_bootstrap.is_file()
                && sandbox_controller.is_file(),
            environment_root.join("sandbox"),
            scripts_root.clone(),
            SystemProcessRunner,
        )?;
        let hyperv = build_hyperv_backend(&environment_root, &scripts_root, &probe).ok();
        Ok(Self {
            root,
            resource_root,
            probe,
            wsl: None,
            selected_wsl_distribution: None,
            sandbox,
            hyperv,
        })
    }

    pub fn select_wsl_distribution(
        &mut self,
        distribution: String,
    ) -> Result<EnvironmentBackendStatus, EnvironmentBackendError> {
        if self
            .selected_wsl_distribution
            .as_ref()
            .is_some_and(|selected| selected != &distribution)
        {
            return Err(EnvironmentBackendError::InvalidRequest(
                "Restart VeriSilo before changing the selected WSL distribution; in-memory environment state is never silently moved between guests."
                    .to_owned(),
            ));
        }
        let detected = detect_wsl();
        if !detected.available
            || !detected
                .distributions
                .iter()
                .any(|candidate| candidate == &distribution)
        {
            return Err(EnvironmentBackendError::InvalidRequest(
                "Select an exact WSL distribution returned by the current discovery result."
                    .to_owned(),
            ));
        }
        backend::validate_distribution_name(&distribution)?;
        let expected_agent_sha256 = expected_wsl_agent_sha256(&self.resource_root);
        let agent_ready = expected_agent_sha256
            .as_deref()
            .is_some_and(|expected| probe_wsl_agent_identity(&distribution, expected));
        let gui_ready = fixed_wsl_test(&distribution, "-d", "/mnt/wslg")
            && fixed_wsl_test(&distribution, "-x", "/usr/bin/chromium");
        let prerequisites = WslChromiumPrerequisites {
            supported_platform: detected.supported_platform,
            wsl_available: detected.available,
            discovered_distributions: detected.distributions,
            guest_agent_distributions: agent_ready
                .then(|| distribution.clone())
                .into_iter()
                .collect(),
            gui_distributions: gui_ready
                .then(|| distribution.clone())
                .into_iter()
                .collect(),
            expected_agent_sha256: expected_agent_sha256.unwrap_or_default(),
        };
        let backend = WslChromiumBackend::new(
            distribution.clone(),
            prerequisites,
            self.root.join("environments").join("wsl"),
            SystemProcessRunner,
        )?;
        let status = backend.status();
        self.selected_wsl_distribution = Some(distribution);
        self.wsl = Some(backend);
        Ok(status)
    }

    pub fn statuses(&self) -> Vec<EnvironmentBackendStatus> {
        vec![
            self.wsl.as_ref().map(EnvironmentBackend::status).unwrap_or_else(|| {
                unavailable_status(
                    EnvironmentBackendId::WslChromium,
                    "Select a currently discovered WSL distribution before using the fixed guest agent.",
                    vec![EnvironmentPrerequisite {
                        id: "selected-distribution".to_owned(),
                        state: PrerequisiteState::Missing,
                        detail: "No WSL distribution has been selected for this desktop session."
                            .to_owned(),
                    }],
                )
            }),
            self.sandbox.status(),
            self.hyperv.as_ref().map(EnvironmentBackend::status).unwrap_or_else(|| {
                unavailable_status(
                    EnvironmentBackendId::HyperV,
                    "Hyper-V requires a signed release probe and a build-locked, SHA-256-pinned base image.",
                    hyperv_missing_prerequisites(&self.probe),
                )
            }),
        ]
    }

    pub fn execute(
        &mut self,
        request: EnvironmentOperationRequest,
    ) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
        let backend = request.backend();
        let operation = request.operation();
        match backend {
            EnvironmentBackendId::WslChromium => {
                let provider = self.wsl.as_mut().ok_or_else(|| {
                    unavailable_environment(
                        backend,
                        operation,
                        "Select and verify a discovered WSL distribution first.",
                    )
                })?;
                dispatch_environment_request(provider, request)
            }
            EnvironmentBackendId::WindowsSandbox => {
                dispatch_environment_request(&mut self.sandbox, request)
            }
            EnvironmentBackendId::HyperV => {
                let provider = self.hyperv.as_mut().ok_or_else(|| {
                    unavailable_environment(
                        backend,
                        operation,
                        "Hyper-V is unavailable until host prerequisites and a build-locked image are verified.",
                    )
                })?;
                dispatch_environment_request(provider, request)
            }
        }
    }

    pub fn selected_wsl_distribution(&self) -> Option<&str> {
        self.selected_wsl_distribution.as_deref()
    }

    pub fn ensure_no_local_environment_artifacts(
        &self,
        environment_id: Uuid,
    ) -> Result<(), EnvironmentBackendError> {
        let artifacts =
            backend::local_environment_artifacts(&self.root.join("environments"), environment_id)?;
        if artifacts.is_empty() {
            return Ok(());
        }
        Err(EnvironmentBackendError::InvalidRequest(format!(
            "Destroy or detach the local environment before archiving or deleting this Silo; durable artifacts remain for {artifacts:?}."
        )))
    }

    pub fn ensure_no_local_environment_artifacts_for_restore(
        &self,
    ) -> Result<(), EnvironmentBackendError> {
        backend::ensure_no_local_environment_artifacts_for_restore(&self.root.join("environments"))
    }

    pub fn roots(&self) -> (&std::path::Path, &std::path::Path) {
        (&self.root, &self.resource_root)
    }
}

fn dispatch_environment_request(
    backend: &mut dyn EnvironmentBackend,
    request: EnvironmentOperationRequest,
) -> Result<EnvironmentActionReceipt, EnvironmentBackendError> {
    match request {
        EnvironmentOperationRequest::Create {
            environment_id,
            network,
            ..
        } => backend.create(CreateEnvironmentRequest {
            environment_id,
            network,
        }),
        EnvironmentOperationRequest::Start { environment_id, .. } => {
            backend.start(EnvironmentRequest { environment_id })
        }
        EnvironmentOperationRequest::Stop { environment_id, .. } => {
            backend.stop(EnvironmentRequest { environment_id })
        }
        EnvironmentOperationRequest::Pause { environment_id, .. } => {
            backend.pause(EnvironmentRequest { environment_id })
        }
        EnvironmentOperationRequest::Snapshot { environment_id, .. } => {
            backend.snapshot(EnvironmentRequest { environment_id })
        }
        EnvironmentOperationRequest::Destroy {
            environment_id,
            confirm_destroy,
            ..
        } => backend.destroy(DestroyEnvironmentRequest {
            environment_id,
            confirm_destroy,
        }),
        EnvironmentOperationRequest::ConfigureNetwork {
            environment_id,
            network,
            ..
        } => backend.configure_network(ConfigureNetworkRequest {
            environment_id,
            runtime_id: Uuid::new_v4(),
            network,
        }),
        EnvironmentOperationRequest::Health { environment_id, .. } => {
            backend.health(EnvironmentRequest { environment_id })
        }
        EnvironmentOperationRequest::Logs { environment_id, .. } => {
            backend.logs(EnvironmentRequest { environment_id })
        }
    }
}

fn unavailable_status(
    backend: EnvironmentBackendId,
    reason: &str,
    prerequisites: Vec<EnvironmentPrerequisite>,
) -> EnvironmentBackendStatus {
    let capabilities = [
        EnvironmentOperation::Create,
        EnvironmentOperation::Start,
        EnvironmentOperation::Stop,
        EnvironmentOperation::Pause,
        EnvironmentOperation::Snapshot,
        EnvironmentOperation::Destroy,
        EnvironmentOperation::ConfigureNetwork,
        EnvironmentOperation::Health,
        EnvironmentOperation::Logs,
    ]
    .into_iter()
    .map(|operation| EnvironmentCapability {
        operation,
        availability: OperationAvailability::Unavailable {
            reason: reason.to_owned(),
        },
    })
    .collect();
    EnvironmentBackendStatus {
        contract_version: ENVIRONMENT_CONTRACT_VERSION,
        backend,
        capabilities,
        prerequisites,
    }
}

fn unavailable_environment(
    backend: EnvironmentBackendId,
    operation: EnvironmentOperation,
    reason: &str,
) -> EnvironmentBackendError {
    EnvironmentBackendError::Unavailable {
        backend,
        operation,
        reason: reason.to_owned(),
    }
}

fn hyperv_missing_prerequisites(probe: &WindowsEnvironmentProbe) -> Vec<EnvironmentPrerequisite> {
    let state = |ready| {
        if ready {
            PrerequisiteState::Verified
        } else {
            PrerequisiteState::Missing
        }
    };
    vec![
        EnvironmentPrerequisite {
            id: "signed-host-probe".to_owned(),
            state: state(probe.schema_version == ENVIRONMENT_CONTRACT_VERSION),
            detail: "A fixed Authenticode-signed host probe must return contract version 1."
                .to_owned(),
        },
        EnvironmentPrerequisite {
            id: "windows-sku".to_owned(),
            state: state(probe.supported_sku),
            detail: "Hyper-V requires a supported Windows edition.".to_owned(),
        },
        EnvironmentPrerequisite {
            id: "administrator".to_owned(),
            state: state(probe.administrator),
            detail: "VM lifecycle operations require an elevated administrator token.".to_owned(),
        },
        EnvironmentPrerequisite {
            id: "virtualization".to_owned(),
            state: state(probe.virtualization_enabled && probe.hyperv_enabled),
            detail: "Firmware virtualization and the Hyper-V feature must be enabled.".to_owned(),
        },
        EnvironmentPrerequisite {
            id: "base-image".to_owned(),
            state: PrerequisiteState::Missing,
            detail: "No release-pinned Hyper-V base image is compiled into this build.".to_owned(),
        },
        EnvironmentPrerequisite {
            id: "guest-agent-receipt".to_owned(),
            state: PrerequisiteState::Missing,
            detail: "No legal VHDX with a pinned guest-agent version/hash and guest profile/network receipt is available."
                .to_owned(),
        },
        EnvironmentPrerequisite {
            id: "signed-provider-scripts".to_owned(),
            state: state(probe.release_scripts_trusted),
            detail: "The fixed host probe and Hyper-V provider must be validly signed by the same release signer."
                .to_owned(),
        },
    ]
}

fn build_hyperv_backend(
    environment_root: &std::path::Path,
    scripts_root: &std::path::Path,
    probe: &WindowsEnvironmentProbe,
) -> Result<HyperVBackend<SystemProcessRunner>, EnvironmentBackendError> {
    let images_root = scripts_root.join("images");
    let script_path = scripts_root.join("verisilo-hyperv.ps1");
    let script_metadata = std::fs::symlink_metadata(&script_path).ok();
    if !script_metadata.is_some_and(|metadata| {
        metadata.is_file() && !backend::metadata_is_reparse_point(&metadata)
    }) {
        return Err(EnvironmentBackendError::InvalidRequest(
            "The fixed Hyper-V provider script is absent or is a symbolic link.".to_owned(),
        ));
    }
    let image = match (
        option_env!("VERISILO_HYPERV_IMAGE_FILE"),
        option_env!("VERISILO_HYPERV_IMAGE_SHA256"),
    ) {
        (Some(file_name), Some(sha256)) => {
            let image_path = images_root.join(file_name);
            std::fs::symlink_metadata(&image_path)
                .ok()
                .filter(|metadata| {
                    metadata.is_file() && !backend::metadata_is_reparse_point(metadata)
                })
                .map(|_| ValidatedHyperVImage {
                    file_name: file_name.to_owned(),
                    sha256: sha256.to_owned(),
                    verified: probe.schema_version == ENVIRONMENT_CONTRACT_VERSION
                        && probe.release_scripts_trusted,
                })
        }
        _ => None,
    };
    HyperVBackend::new(
        HyperVPrerequisites {
            supported_platform: cfg!(target_os = "windows"),
            supported_sku: probe.supported_sku,
            administrator: probe.administrator,
            virtualization_enabled: probe.virtualization_enabled,
            hyperv_enabled: probe.hyperv_enabled,
            reboot_required: probe.reboot_required,
            release_scripts_trusted: probe.release_scripts_trusted,
        },
        environment_root.join("hyperv"),
        images_root,
        script_path,
        image,
        SystemProcessRunner,
    )
}

#[cfg(target_os = "windows")]
fn probe_windows_environment(resource_root: &std::path::Path) -> WindowsEnvironmentProbe {
    let Some(expected_signer_sha256) = option_env!("VERISILO_AUTHENTICODE_SIGNER_SHA256")
        .filter(|value| valid_lowercase_sha256(value))
    else {
        return WindowsEnvironmentProbe::default();
    };
    let script = resource_root
        .join("environment")
        .join("verisilo-environment-probe.ps1");
    if !script.is_file() {
        return WindowsEnvironmentProbe::default();
    }
    let output = run_fixed_windows_process(
        WindowsSystemTool::PowerShell,
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "AllSigned",
            "-File",
        ],
        Some(script.into_os_string()),
        &["-ExpectedSignerCertificateSha256", expected_signer_sha256],
    );
    let Some(output) = output else {
        return WindowsEnvironmentProbe::default();
    };
    if !output.success || output.stdout.len() > MAX_PROBE_OUTPUT_BYTES {
        return WindowsEnvironmentProbe::default();
    }
    serde_json::from_slice::<WindowsEnvironmentProbe>(&output.stdout)
        .ok()
        .filter(|probe| probe.schema_version == ENVIRONMENT_CONTRACT_VERSION)
        .unwrap_or_default()
}

#[cfg(not(target_os = "windows"))]
fn probe_windows_environment(_resource_root: &std::path::Path) -> WindowsEnvironmentProbe {
    WindowsEnvironmentProbe::default()
}

#[cfg(target_os = "windows")]
fn fixed_wsl_test(distribution: &str, predicate: &str, value: &str) -> bool {
    matches!(predicate, "-x" | "-d")
        && run_fixed_windows_process(
            WindowsSystemTool::Wsl,
            &[
                "-d",
                distribution,
                "--user",
                "root",
                "--exec",
                "/usr/bin/test",
                predicate,
                value,
            ],
            None,
            &[],
        )
        .is_some_and(|output| output.success)
}

#[cfg(not(target_os = "windows"))]
fn fixed_wsl_test(_distribution: &str, _predicate: &str, _value: &str) -> bool {
    false
}

#[cfg(target_os = "windows")]
fn probe_wsl_agent_identity(distribution: &str, expected_sha256: &str) -> bool {
    let output = run_fixed_windows_process(
        WindowsSystemTool::Wsl,
        &[
            "-d",
            distribution,
            "--user",
            "root",
            "--exec",
            WSL_GUEST_AGENT_PATH,
            "identity",
            "--silo-id",
        ],
        Some(Uuid::nil().to_string().into()),
        &[],
    );
    let Some(output) = output else {
        return false;
    };
    if !output.success || output.stdout.len() > 4 * 1024 {
        return false;
    }
    serde_json::from_slice::<WslAgentIdentity>(&output.stdout)
        .ok()
        .is_some_and(|identity| {
            identity.schema_version == ENVIRONMENT_CONTRACT_VERSION
                && identity.agent_version == WSL_GUEST_AGENT_VERSION
                && identity.sha256 == expected_sha256
                && identity.owner_uid == 0
                && identity.mode == "755"
                && identity.path == WSL_GUEST_AGENT_PATH
                && identity.browser_user == "verisilo-browser"
                && (1000..65534).contains(&identity.browser_uid)
        })
}

#[cfg(not(target_os = "windows"))]
fn probe_wsl_agent_identity(_distribution: &str, _expected_sha256: &str) -> bool {
    false
}

#[cfg(target_os = "windows")]
fn expected_wsl_agent_sha256(resource_root: &std::path::Path) -> Option<String> {
    let path = resource_root
        .join("environment")
        .join("verisilo-wsl-guest-agent.sh");
    if !path.is_file() {
        return None;
    }
    let metadata = std::fs::symlink_metadata(&path).ok()?;
    if backend::metadata_is_reparse_point(&metadata)
        || metadata.len() != EMBEDDED_WSL_GUEST_AGENT.len() as u64
        || std::fs::read(&path).ok()?.as_slice() != EMBEDDED_WSL_GUEST_AGENT
    {
        return None;
    }
    let output = run_fixed_windows_process(
        WindowsSystemTool::Certutil,
        &["-hashfile"],
        Some(path.into_os_string()),
        &["SHA256"],
    )?;
    if !output.success || output.stdout.len() > 16 * 1024 {
        return None;
    }
    decode_windows_output(&output.stdout)
        .lines()
        .map(str::trim)
        .map(|line| line.replace(' ', "").to_ascii_lowercase())
        .find(|line| line.len() == 64 && line.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

#[cfg(not(target_os = "windows"))]
fn expected_wsl_agent_sha256(_resource_root: &std::path::Path) -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
fn run_fixed_windows_process(
    tool: WindowsSystemTool,
    fixed_args: &[&str],
    trailing_arg: Option<OsString>,
    final_fixed_args: &[&str],
) -> Option<backend::CommandOutput> {
    let mut args = fixed_args
        .iter()
        .map(|argument| OsString::from(*argument))
        .collect::<Vec<_>>();
    if let Some(argument) = trailing_arg {
        args.push(argument);
    }
    args.extend(
        final_fixed_args
            .iter()
            .map(|argument| OsString::from(*argument)),
    );
    let program = trusted_windows_system_tool(tool).ok()?;
    let mut runner = SystemProcessRunner;
    runner
        .run(&CommandSpec {
            program,
            args,
            stdin: None,
            completion: CommandCompletion::WaitForExit,
            timeout: std::time::Duration::from_secs(30),
        })
        .ok()
}

#[cfg(any(target_os = "windows", test))]
fn valid_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(not(target_os = "windows"))]
pub fn detect_wsl() -> WslStatus {
    WslStatus {
        supported_platform: false,
        available: false,
        distributions: Vec::new(),
        message: "WSL Provider 只适用于 Windows；本次没有执行系统命令。".to_owned(),
    }
}

#[cfg(target_os = "windows")]
fn decode_windows_output(bytes: &[u8]) -> String {
    if bytes.chunks(2).skip(1).any(|pair| pair.get(1) == Some(&0)) {
        let words = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        String::from_utf16_lossy(&words)
            .trim_matches('\u{feff}')
            .to_owned()
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;

    fn manager() -> EnvironmentManager {
        let root = std::env::temp_dir().join(format!("verisilo-environment-{}", Uuid::new_v4()));
        let resources = std::env::temp_dir().join(format!("verisilo-resources-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create environment test root");
        fs::create_dir_all(&resources).expect("create resource test root");
        EnvironmentManager::new(root, resources).expect("construct fail-closed manager")
    }

    #[test]
    fn manager_reports_every_operation_once_without_inventing_availability() {
        let manager = manager();
        let statuses = manager.statuses();
        assert_eq!(statuses.len(), 3);
        for status in statuses {
            assert_eq!(status.contract_version, ENVIRONMENT_CONTRACT_VERSION);
            assert_eq!(status.capabilities.len(), 9);
            let operations = status
                .capabilities
                .iter()
                .map(|capability| capability.operation)
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(operations.len(), 9);
        }
    }

    #[test]
    fn operation_request_rejects_unknown_fields_and_preserves_silo_identity() {
        let environment_id = Uuid::new_v4();
        let parsed: EnvironmentOperationRequest = serde_json::from_value(json!({
            "operation": "health",
            "backend": "hyper-v",
            "environmentId": environment_id,
        }))
        .expect("parse strict health request");
        assert_eq!(parsed.environment_id(), environment_id);

        assert!(
            serde_json::from_value::<EnvironmentOperationRequest>(json!({
                "operation": "health",
                "backend": "hyper-v",
                "environmentId": environment_id,
                "command": "Get-VM",
            }))
            .is_err()
        );
    }

    #[test]
    fn unavailable_backend_execution_is_an_error_not_a_successful_noop() {
        let mut manager = manager();
        let environment_id = Uuid::new_v4();
        let error = manager
            .execute(EnvironmentOperationRequest::Health {
                backend: EnvironmentBackendId::HyperV,
                environment_id,
            })
            .expect_err("unconfigured Hyper-V must fail closed");
        assert!(matches!(
            error,
            EnvironmentBackendError::Unavailable {
                backend: EnvironmentBackendId::HyperV,
                operation: EnvironmentOperation::Health,
                ..
            }
        ));
    }

    #[test]
    fn local_environment_artifacts_block_silo_archive_and_delete_until_cleanup() {
        let manager = manager();
        let environment_id = Uuid::new_v4();
        let root = manager.roots().0.to_path_buf();
        let artifact = root
            .join("environments")
            .join("sandbox")
            .join(environment_id.to_string());
        fs::create_dir_all(&artifact).expect("partial local environment artifact");
        let error = manager
            .ensure_no_local_environment_artifacts(environment_id)
            .expect_err("archive/delete must fail closed");
        assert!(error.to_string().contains("Destroy or detach"));

        fs::remove_dir(&artifact).expect("simulated confirmed provider cleanup");
        manager
            .ensure_no_local_environment_artifacts(environment_id)
            .expect("Silo lifecycle may continue after provider cleanup");
    }

    #[test]
    fn global_local_environment_inventory_blocks_restore_until_cleanup() {
        let manager = manager();
        manager
            .ensure_no_local_environment_artifacts_for_restore()
            .expect("missing environments root is clean");

        let artifact = manager
            .roots()
            .0
            .join("environments")
            .join("wsl")
            .join("partial-provider-state");
        fs::create_dir_all(&artifact).expect("partial provider artifact");
        let error = manager
            .ensure_no_local_environment_artifacts_for_restore()
            .expect_err("Vault restore must fail closed");
        assert!(error.to_string().contains("before restoring the Vault"));

        fs::remove_dir(&artifact).expect("remove partial provider artifact");
        manager
            .ensure_no_local_environment_artifacts_for_restore()
            .expect("empty real provider directory is clean");
    }

    #[test]
    fn authenticode_signer_pin_requires_exact_lowercase_sha256() {
        assert!(valid_lowercase_sha256(&"a".repeat(64)));
        assert!(!valid_lowercase_sha256(&"A".repeat(64)));
        assert!(!valid_lowercase_sha256(&"a".repeat(63)));
        assert!(!valid_lowercase_sha256(&format!("{}g", "a".repeat(63))));
    }
}
