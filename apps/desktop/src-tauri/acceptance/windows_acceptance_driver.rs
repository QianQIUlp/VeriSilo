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

#[cfg(target_os = "windows")]
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
#[cfg(target_os = "windows")]
use chrono::TimeZone;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
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
    started_at: DateTime<Utc>,
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
    powershell: PathBuf,
    process_handle: ExactProcessHandle,
    expectation: RuntimeProcessExpectation,
}

#[derive(Clone)]
struct RuntimeProcessExpectation {
    pid: u32,
    launch_requested_at: DateTime<Utc>,
    recorded_started_at: DateTime<Utc>,
    browser_executable: PathBuf,
    profile: PathBuf,
}

#[derive(Clone)]
struct RuntimeProcessEvidence {
    pid: u32,
    creation_time: DateTime<Utc>,
    handle_image: PathBuf,
    reported_image: PathBuf,
    arguments: Vec<String>,
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

    fn process_creation_time(&self) -> Result<DateTime<Utc>, String> {
        #[repr(C)]
        struct FileTime {
            low: u32,
            high: u32,
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GetProcessTimes(
                process: *mut std::ffi::c_void,
                creation: *mut FileTime,
                exit: *mut FileTime,
                kernel: *mut FileTime,
                user: *mut FileTime,
            ) -> i32;
        }

        let mut creation = FileTime { low: 0, high: 0 };
        let mut exit = FileTime { low: 0, high: 0 };
        let mut kernel = FileTime { low: 0, high: 0 };
        let mut user = FileTime { low: 0, high: 0 };
        if unsafe { GetProcessTimes(self.0, &mut creation, &mut exit, &mut kernel, &mut user) } == 0
        {
            return Err("could not read the exact browser process creation time".to_owned());
        }
        windows_file_time_to_utc(creation.low, creation.high)
    }

    fn process_image(&self) -> Result<PathBuf, String> {
        #[link(name = "kernel32")]
        extern "system" {
            fn QueryFullProcessImageNameW(
                process: *mut std::ffi::c_void,
                flags: u32,
                executable_name: *mut u16,
                size: *mut u32,
            ) -> i32;
        }

        let mut buffer = vec![0_u16; 32_768];
        let mut length = buffer.len() as u32;
        if unsafe { QueryFullProcessImageNameW(self.0, 0, buffer.as_mut_ptr(), &mut length) } == 0 {
            return Err("could not read the exact browser process image".to_owned());
        }
        let image = String::from_utf16(&buffer[..length as usize])
            .map_err(|_| "exact browser process image was not valid UTF-16".to_owned())?;
        fs::canonicalize(image)
            .map_err(|error| format!("could not canonicalize exact browser image: {error}"))
    }

    fn ensure_running(&self) -> Result<(), String> {
        const WAIT_OBJECT_0: u32 = 0;
        const WAIT_TIMEOUT: u32 = 0x0000_0102;
        #[link(name = "kernel32")]
        extern "system" {
            fn WaitForSingleObject(handle: *mut std::ffi::c_void, milliseconds: u32) -> u32;
        }

        match unsafe { WaitForSingleObject(self.0, 0) } {
            WAIT_TIMEOUT => Ok(()),
            WAIT_OBJECT_0 => Err("the exact browser process already exited".to_owned()),
            _ => Err("could not confirm the exact browser process is still running".to_owned()),
        }
    }

    fn evidence(&self, powershell: &Path, pid: u32) -> Result<RuntimeProcessEvidence, String> {
        self.ensure_running()?;
        let creation_time = self.process_creation_time()?;
        let handle_image = self.process_image()?;
        let (reported_pid, reported_image, command_line) =
            query_exact_process_command_line(powershell, pid)?;
        let arguments = parse_windows_command_line(&command_line)?;
        self.ensure_running()?;
        Ok(RuntimeProcessEvidence {
            pid: reported_pid,
            creation_time,
            handle_image,
            reported_image,
            arguments,
        })
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

    fn evidence(&self, _powershell: &Path, _pid: u32) -> Result<RuntimeProcessEvidence, String> {
        Err("exact process evidence requires Windows".to_owned())
    }
}

impl ExactProcessTreeGuard {
    fn open(
        taskkill: PathBuf,
        powershell: PathBuf,
        expectation: RuntimeProcessExpectation,
    ) -> Result<Self, String> {
        let process_handle = ExactProcessHandle::open(expectation.pid)?;
        let mut guard = Self {
            pid: None,
            taskkill,
            powershell,
            process_handle,
            expectation,
        };
        guard.verify_binding()?;
        guard.pid = Some(guard.expectation.pid);
        Ok(guard)
    }

    fn verify_binding(&self) -> Result<(), String> {
        let evidence = self
            .process_handle
            .evidence(&self.powershell, self.expectation.pid)?;
        validate_runtime_process_evidence(&self.expectation, &evidence)
    }

    fn terminate(&mut self) -> Result<(), String> {
        let Some(pid) = self.pid.take() else {
            return Ok(());
        };
        self.verify_binding()?;
        terminate_exact_process_tree(&self.taskkill, pid)?;
        Ok(())
    }
}

impl Drop for ExactProcessTreeGuard {
    fn drop(&mut self) {
        if self.pid.is_some() {
            let _ = self.terminate();
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
    let browser_executable = fs::canonicalize(&request.browser.executable)
        .map_err(|error| format!("could not resolve requested browser executable: {error}"))?;
    let powershell = trusted_windows_powershell()?;
    let launch_requested_at = Utc::now();
    let launched = runtime
        .launch(&silo, &managed_profiles, None, None)
        .map_err(|error| format!("desktop core stock launch failed: {error}"))?;
    if launched.state != RuntimeState::Running || launched.active_silo_id != Some(silo.id) {
        return Err("desktop core did not report the exact Silo as running".to_owned());
    }
    let runtime_record = read_runtime_record(&root, silo.id)?;
    let taskkill = trusted_system32_tool("taskkill.exe")?;
    // The held kernel handle prevents PID reuse, while creation time, image,
    // and parsed run-owned Profile argv bind both the initial and final checks.
    let mut managed_guard = ExactProcessTreeGuard::open(
        taskkill,
        powershell.clone(),
        RuntimeProcessExpectation {
            pid: runtime_record.pid,
            launch_requested_at,
            recorded_started_at: runtime_record.started_at,
            browser_executable,
            profile: profile.clone(),
        },
    )?;

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
                "Restarted desktop core recovered the live exact PID/Profile binding; immediately before exact-tree abnormal exit the driver revalidated the held PID's OS creation time, handle/WMI image, and parsed run-owned --user-data-dir argv, then preserved the Profile and left an unrelated process alive.",
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

fn validate_runtime_process_evidence(
    expectation: &RuntimeProcessExpectation,
    evidence: &RuntimeProcessEvidence,
) -> Result<(), String> {
    let clock_tolerance = ChronoDuration::seconds(2);
    if evidence.pid != expectation.pid {
        return Err("process command-line evidence returned a different PID".to_owned());
    }
    if evidence.creation_time < expectation.launch_requested_at - clock_tolerance
        || evidence.creation_time > expectation.recorded_started_at + clock_tolerance
    {
        return Err(
            "browser process creation time is outside the exact launch interval".to_owned(),
        );
    }
    if normalized_path(&evidence.handle_image) != normalized_path(&expectation.browser_executable)
        || normalized_path(&evidence.reported_image)
            != normalized_path(&expectation.browser_executable)
    {
        return Err("browser process image does not match the requested executable".to_owned());
    }
    let profile_arguments = evidence
        .arguments
        .iter()
        .filter_map(|argument| argument.strip_prefix("--user-data-dir="))
        .collect::<Vec<_>>();
    if profile_arguments.len() != 1 || profile_arguments[0].is_empty() {
        return Err("browser command line must contain one exact --user-data-dir".to_owned());
    }
    let command_profile = fs::canonicalize(profile_arguments[0])
        .map_err(|error| format!("browser command-line Profile is unavailable: {error}"))?;
    if normalized_path(&command_profile) != normalized_path(&expectation.profile) {
        return Err("browser command line is not bound to the run-owned Profile".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn windows_file_time_to_utc(low: u32, high: u32) -> Result<DateTime<Utc>, String> {
    const WINDOWS_TO_UNIX_TICKS: u64 = 116_444_736_000_000_000;
    const TICKS_PER_SECOND: u64 = 10_000_000;
    let windows_ticks = ((high as u64) << 32) | low as u64;
    let unix_ticks = windows_ticks
        .checked_sub(WINDOWS_TO_UNIX_TICKS)
        .ok_or_else(|| "browser process creation time predates the Unix epoch".to_owned())?;
    let seconds = (unix_ticks / TICKS_PER_SECOND) as i64;
    let nanoseconds = ((unix_ticks % TICKS_PER_SECOND) * 100) as u32;
    Utc.timestamp_opt(seconds, nanoseconds)
        .single()
        .ok_or_else(|| "browser process creation time is out of range".to_owned())
}

#[cfg(target_os = "windows")]
fn query_exact_process_command_line(
    powershell: &Path,
    pid: u32,
) -> Result<(u32, PathBuf, String), String> {
    const QUERY_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$runtimeProcessId = [uint32]$env:VERISILO_ACCEPTANCE_RUNTIME_PID
$query = "SELECT ProcessId, ExecutablePath, CommandLine FROM Win32_Process WHERE ProcessId = $runtimeProcessId"
$searcher = [System.Management.ManagementObjectSearcher]::new($query)
try {
  $matches = @($searcher.Get())
  if ($matches.Count -ne 1) { throw 'exact process query did not return one process' }
  $item = $matches[0]
  $reportedPid = [uint32]$item.ProcessId
  $reportedImage = [string]$item.ExecutablePath
  $reportedCommandLine = [string]$item.CommandLine
  if ([string]::IsNullOrWhiteSpace($reportedImage) -or [string]::IsNullOrWhiteSpace($reportedCommandLine)) {
    throw 'exact process query omitted image or command line'
  }
  $utf8 = [Text.UTF8Encoding]::new($false)
  [Console]::Out.WriteLine([string]$reportedPid)
  [Console]::Out.WriteLine([Convert]::ToBase64String($utf8.GetBytes($reportedImage)))
  [Console]::Out.WriteLine([Convert]::ToBase64String($utf8.GetBytes($reportedCommandLine)))
} finally {
  $searcher.Dispose()
}
"#;

    let output = Command::new(powershell)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            QUERY_SCRIPT,
        ])
        .env("VERISILO_ACCEPTANCE_RUNTIME_PID", pid.to_string())
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("could not query the exact browser process: {error}"))?;
    if !output.status.success() {
        return Err("trusted exact browser process query failed".to_owned());
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| "exact browser process query returned non-UTF-8 output".to_owned())?;
    let lines = stdout.lines().collect::<Vec<_>>();
    if lines.len() != 3 {
        return Err("exact browser process query returned an unexpected shape".to_owned());
    }
    let reported_pid = lines[0]
        .parse::<u32>()
        .map_err(|_| "exact browser process query returned an invalid PID".to_owned())?;
    let decode = |value: &str, label: &str| -> Result<String, String> {
        let bytes = BASE64_STANDARD
            .decode(value)
            .map_err(|_| format!("exact browser process query returned invalid {label}"))?;
        String::from_utf8(bytes)
            .map_err(|_| format!("exact browser process query returned non-UTF-8 {label}"))
    };
    let reported_image = fs::canonicalize(decode(lines[1], "image")?)
        .map_err(|error| format!("could not canonicalize reported browser image: {error}"))?;
    let command_line = decode(lines[2], "command line")?;
    Ok((reported_pid, reported_image, command_line))
}

#[cfg(target_os = "windows")]
fn parse_windows_command_line(command_line: &str) -> Result<Vec<String>, String> {
    #[link(name = "shell32")]
    extern "system" {
        fn CommandLineToArgvW(command_line: *const u16, argument_count: *mut i32) -> *mut *mut u16;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(memory: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }

    if command_line.is_empty() || command_line.encode_utf16().any(|value| value == 0) {
        return Err("browser command line is empty or contains NUL".to_owned());
    }
    let wide = command_line
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut argument_count = 0_i32;
    let arguments = unsafe { CommandLineToArgvW(wide.as_ptr(), &mut argument_count) };
    if arguments.is_null() || argument_count <= 0 {
        return Err("could not parse the exact browser command line".to_owned());
    }
    let parsed = (|| {
        let pointers = unsafe { std::slice::from_raw_parts(arguments, argument_count as usize) };
        pointers
            .iter()
            .map(|pointer| {
                if pointer.is_null() {
                    return Err("browser command line contained a null argument".to_owned());
                }
                let mut length = 0_usize;
                while unsafe { *pointer.add(length) } != 0 {
                    length += 1;
                }
                String::from_utf16(unsafe { std::slice::from_raw_parts(*pointer, length) })
                    .map_err(|_| "browser command line contained invalid UTF-16".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()
    })();
    let _ = unsafe { LocalFree(arguments.cast()) };
    parsed
}

#[cfg(not(target_os = "windows"))]
fn parse_windows_command_line(_command_line: &str) -> Result<Vec<String>, String> {
    Err("Windows command-line parsing requires Windows".to_owned())
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration as ChronoDuration, Utc};

    fn process_binding_fixture() -> (
        PathBuf,
        RuntimeProcessExpectation,
        RuntimeProcessEvidence,
        PathBuf,
        PathBuf,
    ) {
        let root = env::temp_dir().join(format!(
            "verisilo-acceptance-process-binding-{}",
            uuid::Uuid::new_v4()
        ));
        let profile = root.join("silos/profile-a");
        fs::create_dir_all(&profile).expect("create process binding Profile");
        let browser = root.join("msedge.exe");
        let other_browser = root.join("other.exe");
        fs::write(&browser, []).expect("create browser fixture");
        fs::write(&other_browser, []).expect("create other browser fixture");
        let browser = fs::canonicalize(browser).expect("canonical browser fixture");
        let other_browser =
            fs::canonicalize(other_browser).expect("canonical other browser fixture");
        let profile = fs::canonicalize(profile).expect("canonical Profile fixture");
        let now = Utc::now();
        let expectation = RuntimeProcessExpectation {
            pid: 4242,
            launch_requested_at: now,
            recorded_started_at: now + ChronoDuration::seconds(2),
            browser_executable: browser.clone(),
            profile: profile.clone(),
        };
        let evidence = RuntimeProcessEvidence {
            pid: 4242,
            creation_time: now + ChronoDuration::seconds(1),
            handle_image: browser.clone(),
            reported_image: browser.clone(),
            arguments: vec![
                browser.to_string_lossy().to_string(),
                format!("--user-data-dir={}", profile.display()),
                "about:blank".to_owned(),
            ],
        };
        (root, expectation, evidence, other_browser, profile)
    }

    #[test]
    fn exact_runtime_binding_requires_creation_image_and_run_owned_profile() {
        let (root, expectation, evidence, other_browser, profile) = process_binding_fixture();
        validate_runtime_process_evidence(&expectation, &evidence)
            .expect("valid exact runtime evidence");

        let mut wrong = evidence.clone();
        wrong.creation_time = expectation.launch_requested_at - ChronoDuration::seconds(3);
        assert!(validate_runtime_process_evidence(&expectation, &wrong).is_err());

        let mut wrong = evidence.clone();
        wrong.creation_time = expectation.recorded_started_at + ChronoDuration::seconds(3);
        assert!(validate_runtime_process_evidence(&expectation, &wrong).is_err());

        let mut wrong = evidence.clone();
        wrong.handle_image = other_browser.clone();
        assert!(validate_runtime_process_evidence(&expectation, &wrong).is_err());

        let mut wrong = evidence.clone();
        wrong.reported_image = other_browser;
        assert!(validate_runtime_process_evidence(&expectation, &wrong).is_err());

        let mut wrong = evidence.clone();
        wrong
            .arguments
            .retain(|argument| !argument.starts_with("--user-data-dir="));
        assert!(validate_runtime_process_evidence(&expectation, &wrong).is_err());

        let mut wrong = evidence.clone();
        wrong
            .arguments
            .push(format!("--user-data-dir={}", profile.display()));
        assert!(validate_runtime_process_evidence(&expectation, &wrong).is_err());

        let wrong_profile = root.join("silos/profile-a-other");
        fs::create_dir_all(&wrong_profile).expect("create wrong Profile fixture");
        let wrong_profile = fs::canonicalize(wrong_profile).expect("canonical wrong Profile");
        let mut wrong = evidence;
        wrong.arguments[1] = format!("--user-data-dir={}", wrong_profile.display());
        assert!(validate_runtime_process_evidence(&expectation, &wrong).is_err());

        fs::remove_dir_all(root).expect("remove process binding fixture");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_command_line_parser_preserves_quoted_profile_argument() {
        let arguments = parse_windows_command_line(
            r#""C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" "--user-data-dir=C:\Temp\Profile A" about:blank"#,
        )
        .expect("parse Windows browser command line");
        assert_eq!(
            arguments,
            vec![
                r#"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"#,
                r#"--user-data-dir=C:\Temp\Profile A"#,
                "about:blank",
            ]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn exact_process_query_matches_the_held_test_process() {
        let pid = std::process::id();
        let powershell = trusted_windows_powershell().expect("trusted Windows PowerShell");
        let handle = ExactProcessHandle::open(pid).expect("hold exact test process");
        let handle_image = handle.process_image().expect("test process handle image");
        let (reported_pid, reported_image, command_line) =
            query_exact_process_command_line(&powershell, pid)
                .expect("query exact test process command line");
        assert_eq!(reported_pid, pid);
        assert_eq!(
            normalized_path(&reported_image),
            normalized_path(&handle_image)
        );
        assert!(!parse_windows_command_line(&command_line)
            .expect("parse exact test process command line")
            .is_empty());
        handle.ensure_running().expect("test process remains live");
    }
}
