#![cfg(feature = "acceptance-tests")]

use std::{
    env,
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use verisilo_desktop_lib::{
    domain::{
        BrowserKind, CreateSiloInput, NetworkProfile, RuntimeEvidenceState,
        RuntimeNetworkEvidenceProvenance, RuntimeState, SiloExecutionTarget, VaultLockState,
    },
    engine::SiloEngineConfig,
    launcher::{profile_in_use, LauncherError, RuntimeManager},
    vault::VaultRuntime,
};
use zeroize::Zeroizing;

const REQUEST_SCHEMA: &str = "urn:verisilo:windows-acceptance-request:1";
const RECEIPT_SCHEMA: &str = "urn:verisilo:windows-acceptance-receipt:1";
const ROOT_SENTINEL_FILE: &str = ".verisilo-acceptance-sentinel";
const RECEIPT_FILE: &str = "acceptance-receipt.json";
const COMPILED_SOURCE_REVISION: Option<&str> = option_env!("VERISILO_ACCEPTANCE_SOURCE_REVISION");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AcceptanceRequest {
    schema: String,
    schema_version: u32,
    root: PathBuf,
    sentinel: String,
    passphrase: String,
    browser: BrowserRequest,
    candidate: CandidateBinding,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BrowserRequest {
    kind: String,
    executable: PathBuf,
    extension_id: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CandidateBinding {
    repository: String,
    artifact_id: u64,
    artifact_sha256: String,
    source_revision: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeRecord {
    silo_id: uuid::Uuid,
    pid: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AcceptanceReceipt {
    schema: &'static str,
    schema_version: u32,
    result: &'static str,
    candidate: CandidateBinding,
    driver_build: DriverBuild,
    browser: BrowserReceipt,
    safety: SafetyReceipt,
    results: Vec<AcceptanceResult>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DriverBuild {
    source_revision: String,
    cargo_feature: &'static str,
    credential_transport: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserReceipt {
    kind: String,
    version: String,
    isolated_user_data_dir: bool,
    companion_state: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SafetyReceipt {
    os_temporary_root_validated: bool,
    random_sentinel_validated: bool,
    production_roots_refused: bool,
    exact_runtime_termination: bool,
    unrelated_process_survived: bool,
    profile_preserved: bool,
}

#[derive(Serialize)]
struct AcceptanceResult {
    name: String,
    status: &'static str,
    detail: &'static str,
}

struct ExactProcessTreeGuard {
    pid: Option<u32>,
    taskkill: PathBuf,
    _process_handle: ExactProcessHandle,
}

#[cfg(target_os = "windows")]
struct ExactProcessHandle(*mut std::ffi::c_void);

#[cfg(target_os = "windows")]
impl ExactProcessHandle {
    fn open(pid: u32) -> Result<Self, String> {
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        const SYNCHRONIZE: u32 = 0x0010_0000;
        #[link(name = "kernel32")]
        extern "system" {
            fn OpenProcess(
                desired_access: u32,
                inherit_handle: i32,
                process_id: u32,
            ) -> *mut std::ffi::c_void;
        }

        let handle =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            return Err("could not hold the exact recorded browser process handle".to_owned());
        }
        Ok(Self(handle))
    }
}

#[cfg(target_os = "windows")]
impl Drop for ExactProcessHandle {
    fn drop(&mut self) {
        #[link(name = "kernel32")]
        extern "system" {
            fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
        }
        let _ = unsafe { CloseHandle(self.0) };
    }
}

#[cfg(not(target_os = "windows"))]
struct ExactProcessHandle;

#[cfg(not(target_os = "windows"))]
impl ExactProcessHandle {
    fn open(_pid: u32) -> Result<Self, String> {
        Err("exact process handles require Windows".to_owned())
    }
}

impl ExactProcessTreeGuard {
    fn terminate(&mut self) -> Result<(), String> {
        let Some(pid) = self.pid else {
            return Ok(());
        };
        terminate_exact_process_tree(&self.taskkill, pid)?;
        self.pid = None;
        Ok(())
    }
}

impl Drop for ExactProcessTreeGuard {
    fn drop(&mut self) {
        if let Some(pid) = self.pid.take() {
            let _ = terminate_exact_process_tree(&self.taskkill, pid);
        }
    }
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn main() {
    if !cfg!(all(target_os = "windows", feature = "acceptance-tests")) {
        eprintln!("The acceptance driver is available only on Windows with acceptance-tests.");
        std::process::exit(2);
    }

    match run() {
        Ok(()) => println!("Windows desktop-core acceptance receipt written."),
        Err(error) => {
            eprintln!("Windows desktop-core acceptance failed: {error}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<(), String> {
    let request = read_request_from_anonymous_stdin()?;
    validate_request_envelope(&request)?;
    let root = validate_acceptance_root(&request.root, &request.sentinel)?;
    let compiled_revision = COMPILED_SOURCE_REVISION
        .ok_or_else(|| "driver was not compiled with an exact source revision".to_owned())?;
    if compiled_revision != request.candidate.source_revision {
        return Err("driver source revision does not match the verified candidate".to_owned());
    }

    let browser_kind = parse_browser_kind(&request.browser.kind)?;
    let passphrase = Zeroizing::new(request.passphrase);
    let mut vault = VaultRuntime::default();
    vault
        .initialize(&root, passphrase.as_str())
        .map_err(|error| format!("real Vault initialization failed: {error}"))?;
    if !matches!(vault.status(&root).state, VaultLockState::Unlocked) {
        return Err("initialized Vault did not report unlocked".to_owned());
    }
    vault.lock();
    if !matches!(vault.status(&root).state, VaultLockState::Locked) {
        return Err("locked Vault did not report locked".to_owned());
    }
    vault
        .unlock(&root, passphrase.as_str())
        .map_err(|error| format!("real Vault unlock failed: {error}"))?;

    let create_input = CreateSiloInput {
        name: format!("Windows {} acceptance", request.browser.kind),
        color: "#2457d6".to_owned(),
        browser_kind,
        executable_path: request.browser.executable.to_string_lossy().to_string(),
        network_profile: NetworkProfile::Direct {
            proxy_required: false,
        },
        execution_target: SiloExecutionTarget::Local,
        engine: SiloEngineConfig::default(),
        proxy_credentials: None,
        mihomo_controller_secret: None,
    };
    let silo = vault
        .create_silo(&root, create_input.clone())
        .map_err(|error| format!("real Silo creation failed: {error}"))?;
    let profile = fs::canonicalize(&silo.profile_directory)
        .map_err(|error| format!("could not resolve the managed profile: {error}"))?;
    assert_isolated_profile(&root, &profile, &request.browser.kind)?;

    let preservation_marker = profile.join("acceptance-preservation-marker.txt");
    fs::write(&preservation_marker, b"preserve-this-managed-profile\n")
        .map_err(|error| format!("could not create the profile preservation marker: {error}"))?;

    let silo_count_before_locked_refusal = vault
        .list_silos()
        .map_err(|error| format!("could not count Silo records: {error}"))?
        .len();
    vault.lock();
    if vault.list_silos().is_ok() || vault.get_silo(silo.id).is_ok() {
        return Err("a locked Vault exposed sensitive Silo metadata".to_owned());
    }
    if vault.create_silo(&root, create_input).is_ok() {
        return Err("a locked Vault accepted a sensitive Silo creation".to_owned());
    }
    vault
        .unlock(&root, passphrase.as_str())
        .map_err(|error| format!("Vault did not unlock after locked refusal: {error}"))?;
    if vault
        .list_silos()
        .map_err(|error| format!("could not re-read Silo records: {error}"))?
        .len()
        != silo_count_before_locked_refusal
    {
        return Err("locked operation changed the Silo record set".to_owned());
    }

    let managed_profiles = vault
        .managed_profile_directories()
        .map_err(|error| format!("could not enumerate managed profiles: {error}"))?;
    let mut runtime = RuntimeManager::open(&root);
    let launched = runtime
        .launch(&silo, &managed_profiles, None, None)
        .map_err(|error| format!("desktop core stock launch failed: {error}"))?;
    if launched.state != RuntimeState::Running || launched.active_silo_id != Some(silo.id) {
        return Err("desktop core did not report the exact Silo as running".to_owned());
    }
    let runtime_record = read_runtime_record(&root, silo.id)?;
    let taskkill = trusted_system32_tool("taskkill.exe")?;
    // Holding this kernel handle prevents the recorded PID from being reused
    // between launch evidence and the exact /PID /T termination request.
    let exact_process_handle = ExactProcessHandle::open(runtime_record.pid)?;
    let mut managed_guard = ExactProcessTreeGuard {
        pid: Some(runtime_record.pid),
        taskkill,
        _process_handle: exact_process_handle,
    };

    wait_for_profile_lock(&profile)?;
    let mut refusal_runtime = RuntimeManager::default();
    match refusal_runtime.launch(&silo, &managed_profiles, None, None) {
        Err(LauncherError::ProfileInUse) => {}
        Err(error) => {
            return Err(format!(
                "locked Profile returned the wrong refusal: {error}"
            ))
        }
        Ok(_) => return Err("desktop core launched an already locked managed Profile".to_owned()),
    }
    if !profile_in_use(&profile) || !preservation_marker.is_file() {
        return Err("safe refusal changed the browser lock or managed Profile".to_owned());
    }

    let network = launched.network_evidence.as_ref().ok_or_else(|| {
        "stock launch omitted explicit network/Companion evidence state".to_owned()
    })?;
    thread::sleep(Duration::from_secs(2));
    if !matches!(
        &network.provenance,
        RuntimeNetworkEvidenceProvenance::DesktopControlPlane
    ) || network.exit != RuntimeEvidenceState::NotRequested
        || network.dns != RuntimeEvidenceState::NotRequested
        || network.web_rtc != RuntimeEvidenceState::NotRequested
        || !vault
            .list_network_evidence(Some(silo.id))
            .map_err(|error| format!("could not read Companion evidence history: {error}"))?
            .is_empty()
        || companion_material_exists(&profile, &request.browser.extension_id)
    {
        return Err(
            "extension-absent launch did not remain explicitly desktop-only/not-connected"
                .to_owned(),
        );
    }

    // Dropping a Child handle does not terminate the browser. This simulates a
    // desktop-core exception, after which a new manager must interpret the
    // persisted exact PID together with the still browser-owned Profile lock.
    drop(runtime);
    let mut recovered_runtime = RuntimeManager::open(&root);
    if !recovered_runtime.needs_reconciliation() {
        return Err("desktop restart did not find the persisted exact runtime".to_owned());
    }
    let live_recovery = recovered_runtime.reconcile_persisted(&silo, None);
    if live_recovery.state != RuntimeState::Running
        || live_recovery.active_silo_id != Some(silo.id)
        || !profile_in_use(&profile)
    {
        return Err(
            "desktop restart did not recover the live exact PID/Profile binding".to_owned(),
        );
    }

    let powershell = trusted_windows_powershell()?;
    let unrelated = Command::new(powershell)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep -Seconds 120",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("could not start the unrelated process sentinel: {error}"))?;
    let mut unrelated = ChildGuard(unrelated);
    if unrelated
        .0
        .try_wait()
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Err("unrelated process sentinel exited before recovery".to_owned());
    }

    managed_guard.terminate()?;
    let recovered = wait_for_persisted_exit_recovery(&mut recovered_runtime, &silo)?;
    if !matches!(
        &recovered.state,
        RuntimeState::Stopped | RuntimeState::RecoveryRequired
    ) || (recovered.state == RuntimeState::Stopped && recovered.active_silo_id.is_some())
        || (recovered.state == RuntimeState::RecoveryRequired
            && recovered.active_silo_id != Some(silo.id))
    {
        return Err(
            "exact managed runtime exit received an inconsistent recovery state".to_owned(),
        );
    }
    if unrelated
        .0
        .try_wait()
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Err("exact runtime recovery terminated an unrelated process".to_owned());
    }
    if fs::read(&preservation_marker).ok().as_deref()
        != Some(b"preserve-this-managed-profile\n".as_slice())
        || !profile.is_dir()
    {
        return Err("exception recovery deleted or modified the managed Profile".to_owned());
    }
    if vault
        .get_silo(silo.id)
        .map_err(|error| format!("exception recovery lost the Silo record: {error}"))?
        .profile_directory
        != silo.profile_directory
    {
        return Err("exception recovery rebound the managed Profile".to_owned());
    }

    let version = silo
        .browser
        .version
        .clone()
        .unwrap_or_else(|| "verified-version-unavailable".to_owned());
    let receipt = AcceptanceReceipt {
        schema: RECEIPT_SCHEMA,
        schema_version: 1,
        result: "PASS",
        candidate: request.candidate,
        driver_build: DriverBuild {
            source_revision: compiled_revision.to_owned(),
            cargo_feature: "acceptance-tests",
            credential_transport: "anonymous-stdin-pipe",
        },
        browser: BrowserReceipt {
            kind: request.browser.kind.clone(),
            version,
            isolated_user_data_dir: true,
            companion_state: "not_connected_no_extension_evidence",
        },
        safety: SafetyReceipt {
            os_temporary_root_validated: true,
            random_sentinel_validated: true,
            production_roots_refused: true,
            exact_runtime_termination: true,
            unrelated_process_survived: true,
            profile_preserved: true,
        },
        results: vec![
            pass_result(
                format!("{}_desktop_vault_init_unlock_silo_create", request.browser.kind),
                "Initialized/unlocked a real encrypted Vault and created a real stock-browser Silo.",
            ),
            pass_result(
                format!("{}_desktop_isolated_user_data_dir", request.browser.kind),
                "Desktop core launched only the sentinel-bound temporary managed Profile, outside default browser data.",
            ),
            pass_result(
                "vault_locked_sensitive_operation_refusal".to_owned(),
                "Locked Vault refused Silo metadata reads and creation without changing the managed record set.",
            ),
            pass_result(
                "verisilo_profile_lock_safe_refusal".to_owned(),
                "Desktop RuntimeManager refused a real Chromium-locked managed Profile without deleting its lock or ending it.",
            ),
            pass_result(
                "extension_absent_desktop_degradation".to_owned(),
                "Stock launch remained running with no Companion material/history and explicit desktop-only, not-requested observations.",
            ),
            pass_result(
                "desktop_recovery_after_exception".to_owned(),
                "Restarted desktop core recovered the live exact PID/Profile binding; after exact-tree abnormal exit it preserved the Profile, reported stopped or recovery-required according to the retained lock, and left an unrelated process alive.",
            ),
        ],
    };
    write_acceptance_receipt(&root, &receipt)
}

fn pass_result(name: String, detail: &'static str) -> AcceptanceResult {
    AcceptanceResult {
        name,
        status: "PASS",
        detail,
    }
}

fn read_request_from_anonymous_stdin() -> Result<AcceptanceRequest, String> {
    let mut bytes = Vec::new();
    io::stdin()
        .take(64 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("could not read anonymous stdin: {error}"))?;
    if bytes.is_empty() || bytes.len() >= 64 * 1024 {
        return Err("acceptance request is empty or oversized".to_owned());
    }
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid acceptance request: {error}"))
}

fn write_acceptance_receipt(root: &Path, receipt: &AcceptanceReceipt) -> Result<(), String> {
    let path = root.join(RECEIPT_FILE);
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("could not create the acceptance receipt: {error}"))?;
    serde_json::to_writer_pretty(&mut output, receipt)
        .map_err(|error| format!("could not serialize the acceptance receipt: {error}"))?;
    output
        .write_all(b"\n")
        .and_then(|()| output.flush())
        .map_err(|error| format!("could not finish the acceptance receipt: {error}"))
}

fn validate_request_envelope(request: &AcceptanceRequest) -> Result<(), String> {
    if request.schema != REQUEST_SCHEMA || request.schema_version != 1 {
        return Err("unsupported acceptance request schema".to_owned());
    }
    if request.candidate.repository != "QianQIUlp/VeriSilo"
        || request.candidate.artifact_id == 0
        || !is_lower_hex(&request.candidate.artifact_sha256, 64)
        || request
            .candidate
            .artifact_sha256
            .bytes()
            .all(|value| value == b'0')
        || !is_lower_hex(&request.candidate.source_revision, 40)
        || !is_extension_id(&request.browser.extension_id)
        || !request.browser.executable.is_file()
    {
        return Err("candidate or browser binding is malformed".to_owned());
    }
    Ok(())
}

fn validate_acceptance_root(root: &Path, sentinel: &str) -> Result<PathBuf, String> {
    if !is_lower_hex(sentinel, 64) || sentinel.bytes().all(|value| value == b'0') {
        return Err("acceptance root sentinel is not a random 256-bit value".to_owned());
    }
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("acceptance root is unavailable: {error}"))?;
    let canonical_temp = fs::canonicalize(env::temp_dir())
        .map_err(|error| format!("OS temporary root is unavailable: {error}"))?;
    if !is_strict_descendant(&canonical_root, &canonical_temp) {
        return Err("refusing non-temporary acceptance root".to_owned());
    }
    assert_not_reparse_point(&canonical_root, "acceptance root")?;

    let local_app_data = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "LOCALAPPDATA is unavailable".to_owned())?;
    let forbidden = [
        local_app_data.join("VeriSilo"),
        local_app_data.join("Google/Chrome/User Data"),
        local_app_data.join("Microsoft/Edge/User Data"),
    ];
    if forbidden
        .iter()
        .any(|path| is_same_or_resolved_descendant(&canonical_root, path))
    {
        return Err("refusing production Vault or default browser Profile root".to_owned());
    }

    let sentinel_path = canonical_root.join(ROOT_SENTINEL_FILE);
    assert_not_reparse_point(&sentinel_path, "acceptance sentinel")?;
    let entries = fs::read_dir(&canonical_root)
        .map_err(|error| format!("acceptance root is unreadable: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("acceptance root enumeration failed: {error}"))?;
    if entries.len() != 1 || entries[0].file_name() != ROOT_SENTINEL_FILE {
        return Err("acceptance root must contain only its random sentinel".to_owned());
    }
    let actual = fs::read_to_string(&sentinel_path)
        .map_err(|error| format!("acceptance sentinel is unreadable: {error}"))?;
    if actual != sentinel {
        return Err("acceptance sentinel does not match the anonymous request".to_owned());
    }
    Ok(canonical_root)
}

fn assert_isolated_profile(root: &Path, profile: &Path, browser_kind: &str) -> Result<(), String> {
    if !is_strict_descendant(profile, &root.join("silos")) {
        return Err("Silo Profile escaped the sentinel-bound acceptance root".to_owned());
    }
    let local_app_data = PathBuf::from(
        env::var_os("LOCALAPPDATA").ok_or_else(|| "LOCALAPPDATA is unavailable".to_owned())?,
    );
    let default_user_data = match browser_kind {
        "Chrome" => local_app_data.join("Google/Chrome/User Data"),
        "Edge" => local_app_data.join("Microsoft/Edge/User Data"),
        _ => return Err("unsupported browser kind".to_owned()),
    };
    if is_same_or_resolved_descendant(profile, &default_user_data) {
        return Err("desktop core selected a default browser Profile".to_owned());
    }
    Ok(())
}

fn parse_browser_kind(kind: &str) -> Result<BrowserKind, String> {
    match kind {
        "Chrome" => Ok(BrowserKind::Chrome),
        "Edge" => Ok(BrowserKind::Edge),
        _ => Err("acceptance driver supports only Chrome or Edge".to_owned()),
    }
}

fn wait_for_profile_lock(profile: &Path) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if profile_in_use(profile) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("real Chromium launch did not establish a managed Profile lock".to_owned())
}

fn wait_for_persisted_exit_recovery(
    runtime: &mut RuntimeManager,
    silo: &verisilo_desktop_lib::domain::Silo,
) -> Result<verisilo_desktop_lib::domain::RuntimeActivation, String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let activation = runtime.reconcile_persisted(silo, None);
        if matches!(
            &activation.state,
            RuntimeState::Stopped | RuntimeState::RecoveryRequired
        ) {
            return Ok(activation);
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("desktop core did not reconcile the exact terminated PID/Profile state".to_owned())
}

fn read_runtime_record(root: &Path, expected_silo_id: uuid::Uuid) -> Result<RuntimeRecord, String> {
    let path = root.join("runtime/browser-session.json");
    let bytes =
        fs::read(path).map_err(|error| format!("runtime record is unavailable: {error}"))?;
    let record: RuntimeRecord = serde_json::from_slice(&bytes)
        .map_err(|error| format!("runtime record is malformed: {error}"))?;
    if record.silo_id != expected_silo_id || record.pid == 0 || record.pid == std::process::id() {
        return Err("runtime record is not bound to the exact launched Silo process".to_owned());
    }
    Ok(record)
}

fn companion_material_exists(profile: &Path, extension_id: &str) -> bool {
    let extension_directory_exists = [
        profile.join("Extensions").join(extension_id),
        profile.join("Default/Extensions").join(extension_id),
        profile.join("Profile 1/Extensions").join(extension_id),
    ]
    .iter()
    .any(|path| path.exists());
    extension_directory_exists
        || [
            profile.join("Preferences"),
            profile.join("Secure Preferences"),
            profile.join("Default/Preferences"),
            profile.join("Default/Secure Preferences"),
        ]
        .iter()
        .filter_map(|path| fs::read(path).ok())
        .any(|bytes| {
            bytes
                .windows(extension_id.len())
                .any(|window| window == extension_id.as_bytes())
        })
}

fn trusted_windows_powershell() -> Result<PathBuf, String> {
    trusted_system32_tool("WindowsPowerShell/v1.0/powershell.exe")
}

fn trusted_system32_tool(relative: &str) -> Result<PathBuf, String> {
    let system_root = env::var_os("SystemRoot")
        .map(PathBuf::from)
        .ok_or_else(|| "SystemRoot is unavailable".to_owned())?;
    let system32 = fs::canonicalize(system_root.join("System32"))
        .map_err(|error| format!("System32 is unavailable: {error}"))?;
    let tool = fs::canonicalize(system32.join(relative))
        .map_err(|error| format!("required System32 tool is unavailable: {error}"))?;
    if !is_strict_descendant(&tool, &system32) || !tool.is_file() {
        return Err("required process tool escaped System32".to_owned());
    }
    assert_not_reparse_point(&tool, "System32 process tool")?;
    Ok(tool)
}

fn terminate_exact_process_tree(taskkill: &Path, pid: u32) -> Result<(), String> {
    if pid == 0 || pid == std::process::id() {
        return Err("refusing to terminate a non-runtime PID".to_owned());
    }
    let pid_string = pid.to_string();
    let status = Command::new(taskkill)
        .args(["/PID", pid_string.as_str(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("could not terminate the exact runtime tree: {error}"))?;
    if !status.success() {
        return Err("exact runtime tree termination failed".to_owned());
    }
    Ok(())
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_extension_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| (b'a'..=b'p').contains(&byte))
}

fn normalized_path(path: &Path) -> String {
    let normalized = path
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase();
    if let Some(value) = normalized.strip_prefix("\\\\?\\unc\\") {
        format!("\\\\{value}")
    } else {
        normalized
            .strip_prefix("\\\\?\\")
            .unwrap_or(&normalized)
            .to_owned()
    }
}

fn is_strict_descendant(candidate: &Path, ancestor: &Path) -> bool {
    let candidate = normalized_path(candidate);
    let ancestor = normalized_path(ancestor);
    candidate != ancestor && candidate.starts_with(&format!("{ancestor}\\"))
}

fn is_same_or_descendant(candidate: &Path, ancestor: &Path) -> bool {
    normalized_path(candidate) == normalized_path(ancestor)
        || is_strict_descendant(candidate, ancestor)
}

fn is_same_or_resolved_descendant(candidate: &Path, ancestor: &Path) -> bool {
    is_same_or_descendant(candidate, ancestor)
        || fs::canonicalize(ancestor)
            .ok()
            .is_some_and(|resolved| is_same_or_descendant(candidate, &resolved))
}

#[cfg(target_os = "windows")]
fn assert_not_reparse_point(path: &Path, label: &str) -> Result<(), String> {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("{label} metadata is unavailable: {error}"))?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(format!("refusing reparse-point {label}"));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn assert_not_reparse_point(_path: &Path, _label: &str) -> Result<(), String> {
    Err("acceptance root validation requires Windows".to_owned())
}
