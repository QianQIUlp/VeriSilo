//! Native-Windows M3-WI real Host/browser evidence harness.
//!
//! This module is compiled only for Windows test targets.  It deliberately
//! cannot be selected by a production build and never claims package
//! verification: the real Python Host is launched through the same
//! RuntimeManager/transport path with `package_verification = None`.

use std::{
    collections::BTreeSet,
    env,
    fs::{self, OpenOptions},
    io::Write,
    net::{SocketAddr, TcpListener, TcpStream},
    panic::{catch_unwind, resume_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

use super::{
    process_is_alive, EngineRuntimeProtocol, LauncherError, RuntimeManager,
    M3_WI_REAL_HOST_ADAPTER_VERSION,
};
use crate::{
    domain::{
        BrowserDescriptor, BrowserKind, NetworkProfile, ProxyScheme, RuntimeState, Silo,
        SiloExecutionTarget, SCHEMA_VERSION,
    },
    engine::{
        BrowserFamily, CamoufoxArtifactBindingV1, CamoufoxHostLaunch, DerivedIdentityToken,
        EngineAdapter, EngineAdapterId, EngineCapabilityId, EngineChannel, EngineControlPlan,
        EngineDescriptor, EngineError, EngineHealth, EngineLaunchPlan, EngineLaunchRequest,
        EngineMaintenanceReceipt, EngineNegotiation, EnginePackageRequest, EngineTransport,
        IdentityDerivationContext, IdentityTemplate, IdentityTokenDeriver, SiloEngineConfig,
        SiteFallbackRule, CAMOUFOX_ARTIFACT_SCHEMA, CAMOUFOX_ARTIFACT_SCHEMA_V6,
        CAMOUFOX_HOST_PROTOCOL, ENGINE_CONTRACT_VERSION,
    },
    vault::ProxyAuthentication,
};

const BROWSER_RELEASE: &str = "v152.0.4-beta.28";
const BROWSER_ASSET_SHA256: &str =
    "386fc2f41139685f9a1a9cef0d024bc041d899c315ea538d561171b5b282e57d";
const ARTIFACT_ID: &str = "identity-win-a";
const ARTIFACT_SHA256: &str = "a214c21ccf4a68c97040af6e5f81b05e40903a127dea33ace6dce7d8f133279f";
const TREE_CANONICAL_SHA256: &str =
    "1c749534d139b7efcb425faf03de9cfe1d59004034a1fe1c5ba423b86239c37b";
const VAULT_TOKEN_SENTINEL: &str = "M3-WI-VAULT-TOKEN-SENTINEL-DO-NOT-EMIT";
const PROXY_USERNAME_SENTINEL: &str = "M3-WI-PROXY-USERNAME-SENTINEL-DO-NOT-EMIT";
const PROXY_PASSWORD_SENTINEL: &str = "M3-WI-PROXY-PASSWORD-SENTINEL-DO-NOT-EMIT";

#[derive(Clone)]
struct M3WiRealHostAdapter {
    python_path: PathBuf,
    host_script: PathBuf,
    browser_tree_manifest_path: PathBuf,
    browser_tree_manifest_raw_sha256: String,
    probe_port: u16,
    expected_release: String,
    expected_asset_sha256: String,
    asset_lock_path: Option<PathBuf>,
    browser_root_path: Option<PathBuf>,
}

impl M3WiRealHostAdapter {
    fn test_descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            contract_version: ENGINE_CONTRACT_VERSION,
            id: EngineAdapterId::Camoufox,
            adapter_version: M3_WI_REAL_HOST_ADAPTER_VERSION.to_owned(),
            engine_version: "152.0.4-beta.28".to_owned(),
            channel: EngineChannel::Experimental,
            browser_family: BrowserFamily::Firefox,
            platform: "windows-x64".to_owned(),
            externally_packaged: true,
            emergency_disabled: false,
        }
    }

    fn with_host_script(mut self, host_script: PathBuf) -> Self {
        self.host_script = host_script;
        self
    }
}

impl EngineAdapter for M3WiRealHostAdapter {
    fn descriptor(&self) -> EngineDescriptor {
        self.test_descriptor()
    }

    fn negotiate(&self, _requested: &[EngineCapabilityId]) -> EngineNegotiation {
        panic!("M3-WI test-only adapter does not negotiate production capabilities")
    }

    fn install(
        &mut self,
        _request: &EnginePackageRequest,
    ) -> Result<EngineMaintenanceReceipt, EngineError> {
        Err(EngineError::CapabilityUnavailable(
            "M3-WI test-only adapter cannot install packages".to_owned(),
        ))
    }

    fn update(
        &mut self,
        _request: &EnginePackageRequest,
    ) -> Result<EngineMaintenanceReceipt, EngineError> {
        Err(EngineError::CapabilityUnavailable(
            "M3-WI test-only adapter cannot update packages".to_owned(),
        ))
    }

    fn launch_plan(&self, request: &EngineLaunchRequest) -> Result<EngineLaunchPlan, EngineError> {
        if request.derived_token.is_some() {
            return Err(EngineError::CapabilityUnavailable(
                "Camoufox Host must not receive a Vault-derived token".to_owned(),
            ));
        }
        let required_proxy = matches!(
            request.network_profile,
            NetworkProfile::FixedProxy {
                proxy_required: true,
                scheme: ProxyScheme::Http | ProxyScheme::Socks5,
                ..
            }
        );
        if (!matches!(
            request.network_profile,
            NetworkProfile::Direct {
                proxy_required: false
            }
        ) && !required_proxy)
            || !request.fallback_rules.is_empty()
        {
            return Err(EngineError::CapabilityUnavailable(
                "M3-WI real Host permits Direct(false) or required FixedProxy HTTP/SOCKS5 profiles"
                    .to_owned(),
            ));
        }
        let roots = request.camoufox_roots.as_ref().ok_or_else(|| {
            EngineError::UnsafePath("M3-WI plan is missing app-owned Host roots".to_owned())
        })?;
        let artifact = request.camoufox_artifact_binding.as_ref().ok_or_else(|| {
            EngineError::InvalidIdentityTemplate(
                "M3-WI plan is missing the Artifact binding".to_owned(),
            )
        })?;
        if required_proxy && artifact.schema != CAMOUFOX_ARTIFACT_SCHEMA_V6 {
            return Err(EngineError::CapabilityUnavailable(
                "Camoufox required FixedProxy launches require an Artifact/Policy v6 network-bound binding"
                    .to_owned(),
            ));
        }
        if required_proxy
            && !request
                .identity
                .as_ref()
                .is_some_and(|identity| identity.network.proxy_required)
        {
            return Err(EngineError::CapabilityUnavailable(
                "Camoufox Host v1 identity template network.proxyRequired must be true for required FixedProxy"
                    .to_owned(),
            ));
        }
        if self.asset_lock_path.is_some() != self.browser_root_path.is_some() {
            return Err(EngineError::CapabilityUnavailable(
                "M3-WI explicit asset lock and browser root must be paired".to_owned(),
            ));
        }
        let silo_id = request.silo_id.ok_or_else(|| {
            EngineError::InvalidIdentityTemplate("M3-WI plan is missing the Silo ID".to_owned())
        })?;
        let profile_id = format!("silo-{}", silo_id.simple());
        let arguments = vec![
            "-u".to_owned(),
            self.host_script.to_string_lossy().into_owned(),
            "--artifact-root".to_owned(),
            roots.artifact_root.to_string_lossy().into_owned(),
            "--profile-root".to_owned(),
            roots.profile_root.to_string_lossy().into_owned(),
            "--state-root".to_owned(),
            roots.state_root.to_string_lossy().into_owned(),
            "--tree-manifest".to_owned(),
            self.browser_tree_manifest_path
                .to_string_lossy()
                .into_owned(),
            "--probe-port".to_owned(),
            self.probe_port.to_string(),
        ];
        let mut arguments = arguments;
        if let (Some(asset_lock_path), Some(browser_root_path)) =
            (&self.asset_lock_path, &self.browser_root_path)
        {
            arguments.extend([
                "--asset-lock".to_owned(),
                asset_lock_path.to_string_lossy().into_owned(),
                "--browser-root".to_owned(),
                browser_root_path.to_string_lossy().into_owned(),
            ]);
        }
        if arguments.iter().any(|argument| {
            argument.starts_with("--proxy")
                || argument == "--no-proxy-server"
                || argument.contains(PROXY_USERNAME_SENTINEL)
                || argument.contains(PROXY_PASSWORD_SENTINEL)
                || argument.contains(VAULT_TOKEN_SENTINEL)
        }) {
            return Err(EngineError::CapabilityUnavailable(
                "M3-WI Host argv crossed the frozen network/secret boundary".to_owned(),
            ));
        }
        Ok(EngineLaunchPlan {
            adapter: self.test_descriptor(),
            transport: EngineTransport::CamoufoxHostJsonlV1,
            executable_path: self.python_path.clone(),
            arguments,
            profile_directory: roots.profile_root.join(&profile_id),
            shell: false,
            capabilities: Vec::new(),
            identity_delivery: None,
            control: None,
            camoufox_host: Some(CamoufoxHostLaunch {
                protocol: CAMOUFOX_HOST_PROTOCOL.to_owned(),
                host_version: "0.1.0".to_owned(),
                platform: "windows-x64".to_owned(),
                artifact_id: artifact.artifact_id.clone(),
                artifact_file_sha256: artifact.artifact_file_sha256.clone(),
                profile_id,
                browser_release: self.expected_release.clone(),
                browser_asset_sha256: self.expected_asset_sha256.clone(),
                browser_tree_manifest_path: self.browser_tree_manifest_path.clone(),
                browser_tree_manifest_sha256: self.browser_tree_manifest_raw_sha256.clone(),
                browser_proxy_server: None,
            }),
            // This is the key trust boundary: the real Host integration run is
            // not a production package/signature verification receipt.
            package_verification: None,
        })
    }

    fn health(&self) -> EngineHealth {
        panic!("M3-WI test-only adapter has no production health authority")
    }

    fn rollback(&mut self) -> Result<EngineMaintenanceReceipt, EngineError> {
        Err(EngineError::CapabilityUnavailable(
            "M3-WI test-only adapter cannot roll back packages".to_owned(),
        ))
    }

    fn set_emergency_disabled(
        &mut self,
        _disabled: bool,
        _reason: Option<String>,
    ) -> Result<(), EngineError> {
        Err(EngineError::CapabilityUnavailable(
            "M3-WI test-only adapter cannot change production emergency state".to_owned(),
        ))
    }

    fn validate_identity_template(&self, _template: &IdentityTemplate) -> Result<(), EngineError> {
        Ok(())
    }

    fn derive_identity_token(
        &self,
        _context: &IdentityDerivationContext,
        _deriver: &dyn IdentityTokenDeriver,
    ) -> Result<DerivedIdentityToken, EngineError> {
        Err(EngineError::CapabilityUnavailable(
            "M3-WI Camoufox Host never derives a bootstrap token".to_owned(),
        ))
    }

    fn control_plan(
        &self,
        _session_id: Uuid,
        _template: &IdentityTemplate,
        _rules: &[SiteFallbackRule],
    ) -> Result<EngineControlPlan, EngineError> {
        Err(EngineError::CapabilityUnavailable(
            "M3-WI Host has no generic control-receipt plan".to_owned(),
        ))
    }
}

struct SentinelDeriver {
    called: Arc<AtomicBool>,
}

impl IdentityTokenDeriver for SentinelDeriver {
    fn derive_session_token(
        &self,
        context: &IdentityDerivationContext,
    ) -> Result<DerivedIdentityToken, EngineError> {
        self.called.store(true, Ordering::SeqCst);
        Ok(DerivedIdentityToken {
            token_id: context.session_id,
            token: VAULT_TOKEN_SENTINEL.to_owned(),
            expires_at: context.expires_at,
        })
    }
}

struct ExactChildGuard {
    child: Child,
}

impl ExactChildGuard {
    fn start() -> Self {
        let child = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 300",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start unrelated sentinel process");
        Self { child }
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    fn assert_alive(&mut self) {
        assert!(
            self.child
                .try_wait()
                .expect("query unrelated sentinel")
                .is_none(),
            "unrelated sentinel was terminated by the Host lifecycle"
        );
    }
}

impl Drop for ExactChildGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct RuntimeGuard {
    runtime: RuntimeManager,
    silo_id: Uuid,
    armed: bool,
}

impl RuntimeGuard {
    fn new(runtime: RuntimeManager, silo_id: Uuid) -> Self {
        Self {
            runtime,
            silo_id,
            armed: true,
        }
    }

    fn into_desktop_drop(mut self) -> RuntimeManager {
        self.armed = false;
        std::mem::take(&mut self.runtime)
    }
}

impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if self.runtime.child.is_some() {
            let _ = self.runtime.stop_managed_camoufox(self.silo_id);
        }
        if let Some(child) = self.runtime.child.as_mut() {
            // Exact handle only; never enumerate or kill by name.
            let _ = child.kill();
            let _ = child.wait();
        }
        self.runtime.engine_runtime = None;
    }
}

struct EnvironmentGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvironmentGuard {
    fn set(key: &'static str, value: &Path) -> Self {
        let previous = env::var(key).ok();
        env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvironmentGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.as_ref() {
            env::set_var(self.key, previous);
        } else {
            env::remove_var(self.key);
        }
    }
}

fn required_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("missing required M3-WI runner input {name}"))
}

fn identity_template() -> IdentityTemplate {
    serde_json::from_value(json!({
        "schemaVersion": 1,
        "templateId": Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
        "os": { "family": "windows", "version": "11", "architecture": "x64" },
        "browser": {
            "family": "firefox",
            "majorVersion": 152,
            "userAgent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:152.0) Gecko/20100101 Firefox/152.0",
            "uaCh": null
        },
        "languages": { "primary": "en-US", "accepted": ["en-US"] },
        "timezone": "UTC",
        "screen": {
            "width": 1920, "height": 1080,
            "availableWidth": 1920, "availableHeight": 1040,
            "devicePixelRatio": 1.0, "colorDepth": 24
        },
        "render": { "canvas": "native", "webGlVendor": null, "webGlRenderer": null },
        "fonts": { "families": ["Segoe UI"] },
        "media": { "microphones": 1, "cameras": 1, "speakers": 1, "labelsExposed": true },
        "network": {
            "proxyRequired": false, "countryCode": null,
            "timezone": "UTC", "locale": "en-US", "desiredQuic": "browser_default"
        }
    }))
    .expect("M3-WI identity template")
}

fn silo_for(app_root: &Path, artifact_sha256: &str) -> Silo {
    let control_profile = app_root.join("silo-control-profile");
    fs::create_dir_all(&control_profile).expect("create run-owned Silo control profile");
    let placeholder_browser = app_root.join("not-used-by-camoufox.exe");
    fs::write(&placeholder_browser, []).expect("create run-owned browser descriptor placeholder");
    Silo {
        id: Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap(),
        schema_version: SCHEMA_VERSION,
        name: "m3-wi-real-host".to_owned(),
        color: "#4f46e5".to_owned(),
        browser: BrowserDescriptor {
            kind: BrowserKind::Chrome,
            executable_path: placeholder_browser.to_string_lossy().into_owned(),
            version: None,
        },
        execution_target: SiloExecutionTarget::Local,
        profile_directory: control_profile.to_string_lossy().into_owned(),
        network_profile: NetworkProfile::Direct {
            proxy_required: false,
        },
        engine: SiloEngineConfig::Camoufox {
            identity_template: identity_template(),
            fallback_rules: Vec::new(),
            artifact_binding: Some(CamoufoxArtifactBindingV1 {
                artifact_id: ARTIFACT_ID.to_owned(),
                artifact_file_sha256: artifact_sha256.to_owned(),
                schema: CAMOUFOX_ARTIFACT_SCHEMA.to_owned(),
            }),
        },
        seed_reference: Uuid::parse_str("55555555-5555-4555-8555-555555555555").unwrap(),
        created_at: Utc::now(),
        identity_locked_at: None,
        archived_at: None,
    }
}

fn copy_artifact(repo_root: &Path, app_root: &Path) {
    let source_root = repo_root.join("tests/fixtures/camoufox");
    let destination = app_root.join("camoufox/artifacts");
    fs::create_dir_all(&destination).expect("create run-owned Artifact root");
    for name in ["identity-win-a.json", "identity-win-a.json.sha256"] {
        fs::copy(source_root.join(name), destination.join(name))
            .unwrap_or_else(|error| panic!("copy {name} into run-owned Artifact root: {error}"));
    }
    fs::create_dir_all(app_root.join("camoufox/profiles")).expect("create run-owned profile root");
    fs::create_dir_all(app_root.join("camoufox/state")).expect("create run-owned state root");
}

fn adapter_for(
    repo_root: &Path,
    probe_port: u16,
    expected_release: &str,
    expected_asset_sha256: &str,
    tree_raw_sha256: &str,
) -> M3WiRealHostAdapter {
    let host_dir = repo_root.join("apps/camoufox-host");
    M3WiRealHostAdapter {
        python_path: PathBuf::from(required_env("VERISILO_M3_WI_PYTHON_PATH")),
        host_script: host_dir.join("host_v1.py"),
        browser_tree_manifest_path: repo_root
            .join("tests/fixtures/camoufox/browser-tree-manifest-windows.json"),
        browser_tree_manifest_raw_sha256: tree_raw_sha256.to_owned(),
        probe_port,
        expected_release: expected_release.to_owned(),
        expected_asset_sha256: expected_asset_sha256.to_owned(),
        asset_lock_path: None,
        browser_root_path: None,
    }
}

fn new_runtime(app_root: &Path, adapter: M3WiRealHostAdapter) -> RuntimeManager {
    let mut runtime = RuntimeManager::open(app_root);
    runtime.set_test_engine_adapter(Box::new(adapter));
    runtime
}

fn managed_profiles(silo: &Silo) -> Vec<PathBuf> {
    vec![PathBuf::from(&silo.profile_directory)]
}

fn launch_real(
    app_root: &Path,
    adapter: M3WiRealHostAdapter,
    silo: &Silo,
    deriver: &SentinelDeriver,
) -> Result<RuntimeGuard, LauncherError> {
    let mut runtime = new_runtime(app_root, adapter);
    runtime.launch_with_identity_deriver(
        silo,
        &managed_profiles(silo),
        None,
        None,
        Some(deriver),
    )?;
    Ok(RuntimeGuard::new(runtime, silo.id))
}

fn active_snapshot(runtime: &RuntimeManager, app_root: &Path) -> Value {
    let host_pid = runtime.child.as_ref().expect("owned Host child").id();
    let host = match runtime.engine_runtime.as_ref() {
        Some(EngineRuntimeProtocol::CamoufoxHost(host)) => host,
        _ => panic!("RuntimeManager did not retain the real Host protocol"),
    };
    let session_path = app_root
        .join("camoufox/state")
        .join(&host.session_id)
        .join("session.json");
    let observed_path = session_path.with_file_name("observed.json");
    let session: Value = serde_json::from_slice(
        &fs::read(&session_path).expect("read real Host running session state"),
    )
    .expect("parse real Host running session state");
    let observed: Value =
        serde_json::from_slice(&fs::read(&observed_path).expect("read real browser observation"))
            .expect("parse real browser observation");
    let wire = String::from_utf8_lossy(&host.transport.wire_snapshot.concat()).into_owned();
    json!({
        "hostPid": host_pid,
        "sessionId": host.session_id,
        "profileId": host.binding.profile_id,
        "artifactId": host.binding.artifact_id,
        "artifactFileSha256": host.binding.artifact_file_sha256,
        "browserProxyServer": host.binding.browser_proxy_server,
        "observedWebsiteDigest": host.observed_website_digest,
        "evidenceClass": host.evidence_class,
        "launchSurface": host
            .launch_surface
            .clone()
            .expect("real M3-WI Host launch surface receipt"),
        "wire": wire,
        "activation": serde_json::to_value(
            runtime.activation.as_ref().expect("active RuntimeManager activation")
        ).unwrap(),
        "runtimeRecord": serde_json::to_value(&runtime.record).unwrap(),
        "session": session,
        "observed": observed,
        "stageDiagnostics": stage_diagnostics(app_root),
    })
}

fn session_after_stop(app_root: &Path, session_id: &str) -> Value {
    let path = app_root
        .join("camoufox/state")
        .join(session_id)
        .join("session.json");
    serde_json::from_slice(&fs::read(path).expect("read stopped Host session state"))
        .expect("parse stopped Host session state")
}

fn stage_diagnostics(app_root: &Path) -> Vec<Value> {
    let path = app_root.join("camoufox/state/host-stderr.log");
    let entries = fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.strip_prefix("stage-diagnostic "))
        .filter_map(|payload| serde_json::from_str(payload).ok())
        .collect::<Vec<Value>>();
    let start = entries
        .iter()
        .rposition(|entry| {
            entry.get("stage").and_then(Value::as_str) == Some("response write")
                && entry.get("event").and_then(Value::as_str) == Some("start")
        })
        .unwrap_or(0);
    entries.into_iter().skip(start).collect()
}

fn assert_stage_diagnostics(diagnostics: &Value) {
    let entries = diagnostics
        .as_array()
        .expect("Host stage diagnostics array");
    for stage in [
        "browser/context",
        "page",
        "probe",
        "observed collection",
        "response write",
        "close",
    ] {
        assert!(
            entries.iter().any(|entry| {
                entry.get("stage").and_then(Value::as_str) == Some(stage)
                    && entry.get("event").and_then(Value::as_str) == Some("success")
            }),
            "missing successful Host stage diagnostic for {stage}: {entries:?}"
        );
    }
    assert!(
        entries.iter().all(|entry| {
            entry.get("kind").and_then(Value::as_str) == Some("camoufox-host-stage")
                && serde_json::to_string(entry)
                    .map(|encoded| encoded.len() <= 512)
                    .unwrap_or(false)
        }),
        "Host stage diagnostics must remain bounded and typed"
    );
}

fn json_u32_array(value: &Value, pointer: &str) -> Vec<u32> {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("missing PID array {pointer}"))
        .iter()
        .map(|pid| pid.as_u64().expect("PID integer") as u32)
        .collect()
}

fn wait_for_pids_dead(pids: &[u32], timeout: Duration) -> Vec<u32> {
    let deadline = Instant::now() + timeout;
    loop {
        let alive = pids
            .iter()
            .copied()
            .filter(|pid| process_is_alive(*pid))
            .collect::<Vec<_>>();
        if alive.is_empty() || Instant::now() >= deadline {
            return alive;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn wait_exact_child_exit(runtime: &mut RuntimeManager, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        if runtime
            .child
            .as_mut()
            .expect("exact Host child retained")
            .try_wait()
            .expect("query exact Host child")
            .is_some()
        {
            return;
        }
        assert!(Instant::now() < deadline, "exact Host child did not exit");
        thread::sleep(Duration::from_millis(100));
    }
}

fn assert_running_evidence(snapshot: &Value) {
    assert_eq!(
        snapshot
            .pointer("/activation/state")
            .and_then(Value::as_str),
        Some("running")
    );
    assert_eq!(
        snapshot
            .pointer("/activation/engineEvidence/packageVerification")
            .and_then(Value::as_str),
        Some("not_requested")
    );
    assert_eq!(
        snapshot
            .pointer("/activation/engineEvidence/hostLaunch")
            .and_then(Value::as_str),
        Some("observed")
    );
    assert!(snapshot
        .pointer("/activation/engineEvidence/verifiedAdapter")
        .is_none_or(Value::is_null));
    assert_eq!(
        snapshot.get("evidenceClass").and_then(Value::as_str),
        Some("observed-on-this-host")
    );
    assert_eq!(
        snapshot.pointer("/session/state").and_then(Value::as_str),
        Some("running")
    );
    assert_eq!(
        snapshot
            .pointer("/observed/mediaDeviceReadiness/matched")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        snapshot
            .pointer("/launchSurface/integrationPath")
            .and_then(Value::as_str),
        Some("test-only-real-host")
    );
    assert_eq!(
        snapshot
            .pointer("/launchSurface/adapterVersion")
            .and_then(Value::as_str),
        Some(M3_WI_REAL_HOST_ADAPTER_VERSION)
    );
    let expected_python = required_env("VERISILO_M3_WI_PYTHON_PATH");
    assert_eq!(
        snapshot
            .pointer("/launchSurface/executablePath")
            .and_then(Value::as_str),
        Some(expected_python.as_str())
    );
    assert_eq!(
        snapshot
            .pointer("/launchSurface/packageVerification")
            .and_then(Value::as_str),
        None
    );
    assert_eq!(
        snapshot
            .pointer("/launchSurface/shell")
            .and_then(Value::as_bool),
        Some(false)
    );
    let arguments = snapshot
        .pointer("/launchSurface/arguments")
        .and_then(Value::as_array)
        .expect("real M3-WI typed Host argv receipt");
    assert_eq!(arguments.first().and_then(Value::as_str), Some("-u"));
    let expected_host = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../camoufox-host/host_v1.py")
        .canonicalize()
        .expect("canonical real Host source")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        arguments.get(1).and_then(Value::as_str),
        Some(expected_host.as_str())
    );
}

fn assert_clean_stop(stopped: &Value, host_pid: u32, managed_pids: &[u32]) {
    assert_eq!(stopped.get("state").and_then(Value::as_str), Some("exited"));
    assert_eq!(
        stopped
            .pointer("/processTreeExit/exited")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        stopped
            .pointer("/processTreeExit/job/activeProcessCount")
            .and_then(Value::as_u64),
        Some(0)
    );
    let mut pids = managed_pids.to_vec();
    pids.push(host_pid);
    assert!(
        wait_for_pids_dead(&pids, Duration::from_secs(30)).is_empty(),
        "clean stop left an owned Host/browser/supervisor process"
    );
}

fn assert_r2_clean_close(stopped: &Value) {
    assert_eq!(stopped.get("state").and_then(Value::as_str), Some("exited"));
    assert_eq!(
        stopped
            .pointer("/closeOutcome/status")
            .and_then(Value::as_str),
        Some("success")
    );
    assert_eq!(
        stopped
            .pointer("/closeOutcome/contextClose/ctx/status")
            .and_then(Value::as_str),
        Some("success")
    );
    assert!(matches!(
        stopped
            .pointer("/closeOutcome/contextClose/page/status")
            .and_then(Value::as_str),
        Some("success") | Some("not_present")
    ));
    assert_eq!(
        stopped
            .pointer("/closeOutcome/gracefulProcessExit/status")
            .and_then(Value::as_str),
        Some("success")
    );
    assert_eq!(
        stopped
            .pointer("/closeOutcome/forcedJobCleanup/status")
            .and_then(Value::as_str),
        Some("not_needed")
    );
    assert_eq!(
        stopped
            .pointer("/processTreeExit/exited")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        stopped
            .pointer("/processTreeExit/sigkill")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        stopped
            .pointer("/processTreeExit/job/terminateJobObject")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        stopped
            .pointer("/processTreeExit/job/activeProcessCount")
            .and_then(Value::as_u64),
        Some(0)
    );
}

fn assert_r2_diagnostics(diagnostics: &Value) {
    assert_stage_diagnostics(diagnostics);
    let entries = diagnostics.as_array().expect("R2 diagnostics array");
    for (phase, expected_event, expected_outcome) in [
        ("ctx.close", "result", "success"),
        ("graceful-process-exit", "result", "success"),
        ("forced-job-cleanup", "result", "not_needed"),
        ("sqlite-evidence", "result", "available"),
    ] {
        assert!(
            entries.iter().any(|entry| {
                entry.get("stage").and_then(Value::as_str) == Some("close")
                    && entry.get("phase").and_then(Value::as_str) == Some(phase)
                    && entry.get("event").and_then(Value::as_str) == Some(expected_event)
                    && entry.get("outcome").and_then(Value::as_str) == Some(expected_outcome)
            }),
            "missing R2 close diagnostic {phase}: {entries:?}"
        );
    }
}

fn scan_secret_surfaces(values: &[&Value], extra: &[String]) -> Value {
    let artifact: Value = serde_json::from_slice(
        &fs::read(required_env("VERISILO_M3_WI_ARTIFACT_PATH"))
            .expect("read tracked Artifact for seed sentinel extraction"),
    )
    .expect("parse tracked Artifact for seed sentinel extraction");
    let mut sentinels = vec![
        VAULT_TOKEN_SENTINEL.to_owned(),
        PROXY_USERNAME_SENTINEL.to_owned(),
        PROXY_PASSWORD_SENTINEL.to_owned(),
    ];
    for pointer in [
        "/resolvedConfig/fonts:spacing_seed",
        "/resolvedConfig/audio:seed",
        "/resolvedConfig/canvas:seed",
    ] {
        sentinels.push(
            artifact
                .pointer(pointer)
                .and_then(Value::as_u64)
                .unwrap_or_else(|| panic!("missing Artifact seed {pointer}"))
                .to_string(),
        );
    }
    let mut matches = Vec::new();
    for (surface_index, value) in values.iter().enumerate() {
        let encoded = serde_json::to_string(value).expect("serialize secret-scan surface");
        for (sentinel_index, sentinel) in sentinels.iter().enumerate() {
            if encoded.contains(sentinel) {
                matches.push(format!("json-{surface_index}:sentinel-{sentinel_index}"));
            }
        }
    }
    for (surface_index, value) in extra.iter().enumerate() {
        for (sentinel_index, sentinel) in sentinels.iter().enumerate() {
            if value.contains(sentinel) {
                matches.push(format!("text-{surface_index}:sentinel-{sentinel_index}"));
            }
        }
    }
    json!({
        "patternsChecked": [
            "vaultDeriverToken", "proxyUsername", "proxyPassword",
            "artifactFontSpacingSeed", "artifactAudioSeed", "artifactCanvasSeed"
        ],
        "matches": matches,
    })
}

fn scenario_root(run_dir: &Path, name: &str, repo_root: &Path) -> PathBuf {
    let root = run_dir.join(name).join("app");
    copy_artifact(repo_root, &root);
    root
}

fn write_new_json(path: &Path, value: &Value) {
    let mut raw = serde_json::to_vec_pretty(value).expect("serialize immutable FP3 evidence");
    raw.push(b'\n');
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut file| file.write_all(&raw))
        .unwrap_or_else(|error| panic!("write immutable FP3 evidence {}: {error}", path.display()));
}

#[derive(Clone)]
struct FP3FailureContext {
    host_pid: u32,
    managed_pids: Vec<u32>,
    relay_port: u16,
    session_path: PathBuf,
}

fn fp3_failure_cleanup(context: &Mutex<Option<FP3FailureContext>>) -> Value {
    let Some(context) = context.lock().expect("FP3 failure context").clone() else {
        return json!({
            "status": "unavailable",
            "reason": "panic before exact Host/relay ownership was captured"
        });
    };
    let stopped = fs::read(&context.session_path)
        .ok()
        .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
        .unwrap_or(Value::Null);
    let mut owned_pids = context.managed_pids;
    if owned_pids.is_empty() {
        owned_pids.extend(
            stopped
                .pointer("/managedPids")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_u64)
                .map(|pid| pid as u32),
        );
    }
    owned_pids.push(context.host_pid);
    owned_pids.sort_unstable();
    owned_pids.dedup();
    let alive_owned_pids = wait_for_pids_dead(&owned_pids, Duration::from_secs(30));
    let relay_closed = TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], context.relay_port)),
        Duration::from_millis(250),
    )
    .is_err();
    let clean = alive_owned_pids.is_empty()
        && relay_closed
        && stopped.get("state").and_then(Value::as_str) == Some("exited")
        && stopped
            .pointer("/closeOutcome/status")
            .and_then(Value::as_str)
            == Some("success")
        && stopped
            .pointer("/processTreeExit/job/activeProcessCount")
            .and_then(Value::as_u64)
            == Some(0);
    json!({
        "status": if clean { "observed_clean" } else { "failed" },
        "ownedPids": owned_pids,
        "aliveOwnedPids": alive_owned_pids,
        "relayPort": context.relay_port,
        "relayPortClosed": relay_closed,
        "stopped": stopped
    })
}

#[test]
#[ignore = "requires authorized native Windows browser and external FP3 observation services"]
fn fp3_1b_native_windows_required_fixed_proxy_discriminator() {
    let evidence_path = PathBuf::from(required_env("VERISILO_FP3_NATIVE_EVIDENCE_PATH"));
    assert!(
        !evidence_path.exists(),
        "FP3 native evidence already exists"
    );
    let failure_context = Mutex::new(None);
    let result = catch_unwind(AssertUnwindSafe(|| {
        run_fp3_1b(&evidence_path, &failure_context)
    }));
    if let Err(payload) = result {
        if !evidence_path.exists() {
            fs::create_dir_all(evidence_path.parent().expect("FP3 evidence parent"))
                .expect("create FP3 failed-evidence parent");
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| {
                    payload
                        .downcast_ref::<&str>()
                        .map(|value| (*value).to_owned())
                })
                .unwrap_or_else(|| "non-string Rust panic".to_owned());
            write_new_json(
                &evidence_path,
                &json!({
                    "schema": "verisilo-camoufox-fp3-1b-native-evidence/v1",
                    "status": "failed",
                    "evidenceClass": "attempted-on-this-native-windows-host",
                    "verified": false,
                    "failure": { "type": "RustPanic", "message": message },
                    "cleanup": fp3_failure_cleanup(&failure_context),
                    "dns": { "actualPath": "unavailable" }
                }),
            );
        }
        resume_unwind(payload);
    }
}

fn run_fp3_1b(evidence_path: &Path, failure_context: &Mutex<Option<FP3FailureContext>>) {
    let run_dir = evidence_path.parent().expect("FP3 evidence parent");
    let root = run_dir.join("runtime").join("app");
    assert!(!root.exists(), "FP3 runtime root already exists");
    let artifact_root = root.join("camoufox/artifacts");
    fs::create_dir_all(&artifact_root).expect("create FP3 Artifact root");
    fs::create_dir_all(root.join("camoufox/profiles")).expect("create FP3 Profile root");
    fs::create_dir_all(root.join("camoufox/state")).expect("create FP3 state root");

    let artifact_id = required_env("VERISILO_FP3_ARTIFACT_ID");
    let artifact_sha256 = required_env("VERISILO_FP3_ARTIFACT_SHA256");
    let artifact_source = PathBuf::from(required_env("VERISILO_FP3_ARTIFACT_PATH"));
    let expected_artifact_name = format!("{artifact_id}.json");
    assert_eq!(
        artifact_source.file_name().and_then(|name| name.to_str()),
        Some(expected_artifact_name.as_str()),
        "FP3 Artifact filename must match its immutable ID"
    );
    let source_sidecar = artifact_source.with_file_name(format!("{expected_artifact_name}.sha256"));
    assert_eq!(
        fs::read_to_string(&source_sidecar).expect("read FP3 Artifact sidecar"),
        format!("{artifact_sha256}  {expected_artifact_name}\n")
    );
    let artifact_raw = fs::read(&artifact_source).expect("read FP3 Artifact");
    let artifact_document: Value =
        serde_json::from_slice(&artifact_raw).expect("parse FP3 Artifact");
    fs::write(artifact_root.join(&expected_artifact_name), artifact_raw)
        .expect("copy FP3 Artifact into run-owned root");
    fs::copy(
        &source_sidecar,
        artifact_root.join(format!("{expected_artifact_name}.sha256")),
    )
    .expect("copy FP3 Artifact sidecar into run-owned root");

    let proxy_host = required_env("VERISILO_FP3_PROXY_HOST");
    let proxy_port = required_env("VERISILO_FP3_PROXY_PORT")
        .parse::<u16>()
        .expect("FP3 proxy port");
    let direct_ip = required_env("VERISILO_FP3_DIRECT_PUBLIC_IP");
    let artifact_network_identity = artifact_document
        .get("networkIdentity")
        .expect("FP3 Artifact networkIdentity");
    let expected_ip = artifact_network_identity
        .get("expectedPublicAddress")
        .and_then(Value::as_str)
        .expect("FP3 Artifact public address")
        .to_owned();
    let expected_country = artifact_network_identity
        .get("countryCode")
        .and_then(Value::as_str)
        .expect("FP3 Artifact country code")
        .to_owned();
    let expected_timezone = artifact_network_identity
        .get("timezone")
        .and_then(Value::as_str)
        .expect("FP3 Artifact timezone")
        .to_owned();
    let expected_locale = artifact_network_identity
        .get("locale")
        .and_then(Value::as_str)
        .expect("FP3 Artifact locale")
        .to_owned();
    let expected_latitude = artifact_network_identity
        .get("latitude")
        .and_then(Value::as_f64)
        .expect("FP3 Artifact latitude");
    let expected_longitude = artifact_network_identity
        .get("longitude")
        .and_then(Value::as_f64)
        .expect("FP3 Artifact longitude");
    let stun_url = required_env("VERISILO_FP3_STUN_URL");

    let control_profile = root.join("silo-control-profile");
    fs::create_dir_all(&control_profile).expect("create FP3 Silo control Profile");
    let placeholder_browser = root.join("not-used-by-camoufox.exe");
    fs::write(&placeholder_browser, []).expect("create FP3 browser descriptor placeholder");
    let identity: IdentityTemplate = serde_json::from_value(json!({
        "schemaVersion": 1,
        "templateId": "73333333-3333-4333-8333-333333333333",
        "os": { "family": "windows", "version": "11", "architecture": "x64" },
        "browser": {
            "family": "firefox",
            "majorVersion": 152,
            "userAgent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:152.0) Gecko/20100101 Firefox/152.0",
            "uaCh": null
        },
        "languages": { "primary": expected_locale, "accepted": [expected_locale] },
        "timezone": expected_timezone,
        "screen": {
            "width": 1920, "height": 1080,
            "availableWidth": 1920, "availableHeight": 1040,
            "devicePixelRatio": 1.0, "colorDepth": 24
        },
        "render": { "canvas": "native", "webGlVendor": null, "webGlRenderer": null },
        "fonts": { "families": ["Segoe UI"] },
        "media": { "microphones": 1, "cameras": 1, "speakers": 1, "labelsExposed": true },
        "network": {
            "proxyRequired": true,
            "countryCode": expected_country,
            "timezone": expected_timezone,
            "locale": expected_locale,
            "desiredQuic": "browser_default"
        }
    }))
    .expect("FP3 IdentityTemplate");
    let silo = Silo {
        id: Uuid::parse_str("74444444-4444-4444-8444-444444444444").unwrap(),
        schema_version: SCHEMA_VERSION,
        name: "fp3-1b-required-fixed-proxy".to_owned(),
        color: "#4f46e5".to_owned(),
        browser: BrowserDescriptor {
            kind: BrowserKind::Chrome,
            executable_path: placeholder_browser.to_string_lossy().into_owned(),
            version: None,
        },
        execution_target: SiloExecutionTarget::Local,
        profile_directory: control_profile.to_string_lossy().into_owned(),
        network_profile: NetworkProfile::FixedProxy {
            proxy_required: true,
            scheme: ProxyScheme::Socks5,
            host: proxy_host.clone(),
            port: proxy_port,
            bypass_list: Vec::new(),
            credential_reference: None,
            external_mihomo: None,
        },
        engine: SiloEngineConfig::Camoufox {
            identity_template: identity,
            fallback_rules: Vec::new(),
            artifact_binding: Some(CamoufoxArtifactBindingV1 {
                artifact_id: artifact_id.clone(),
                artifact_file_sha256: artifact_sha256.clone(),
                schema: CAMOUFOX_ARTIFACT_SCHEMA_V6.to_owned(),
            }),
        },
        seed_reference: Uuid::parse_str("75555555-5555-4555-8555-555555555555").unwrap(),
        created_at: Utc::now(),
        identity_locked_at: None,
        archived_at: None,
    };

    let cache_root = run_dir.join("runtime/cache");
    fs::create_dir_all(&cache_root).expect("create FP3 run-owned Camoufox cache");
    let _cache_environment = EnvironmentGuard::set("VERISILO_CAMOUFOX_CACHE_DIR", &cache_root);
    let reservation = TcpListener::bind("127.0.0.1:0").expect("reserve FP3 probe port");
    let probe_port = reservation.local_addr().expect("FP3 probe address").port();
    drop(reservation);

    let mut adapter = adapter_for(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .expect("FP3 repository root")
            .as_path(),
        probe_port,
        BROWSER_RELEASE,
        &required_env("VERISILO_FP3_EXPECTED_ASSET_SHA256"),
        &required_env("VERISILO_FP3_TREE_SHA256"),
    )
    .with_host_script(PathBuf::from(required_env("VERISILO_FP3_HOST_SCRIPT")));
    adapter.browser_tree_manifest_path = PathBuf::from(required_env("VERISILO_FP3_TREE_MANIFEST"));
    adapter.asset_lock_path = Some(PathBuf::from(required_env("VERISILO_FP3_ASSET_LOCK")));
    adapter.browser_root_path = Some(PathBuf::from(required_env("VERISILO_FP3_BROWSER_ROOT")));

    let deriver_called = Arc::new(AtomicBool::new(false));
    let deriver = SentinelDeriver {
        called: Arc::clone(&deriver_called),
    };
    let mut guard = launch_real(&root, adapter, &silo, &deriver)
        .unwrap_or_else(|error| panic!("launch FP3 native discriminator: {error}"));
    let relay_port = guard
        .runtime
        .proxy_relay
        .as_ref()
        .expect("FP3 exact runtime relay")
        .endpoint()
        .port;
    let relay_uri = format!("socks5://127.0.0.1:{relay_port}");
    let upstream_endpoint = format!("{proxy_host}:{proxy_port}");
    let host_pid = guard.runtime.child.as_ref().expect("FP3 Host child").id();
    let session_id = match guard.runtime.engine_runtime.as_ref() {
        Some(EngineRuntimeProtocol::CamoufoxHost(host)) => host.session_id.clone(),
        _ => panic!("FP3 RuntimeManager did not retain the Host protocol"),
    };
    *failure_context.lock().expect("set FP3 failure context") = Some(FP3FailureContext {
        host_pid,
        managed_pids: Vec::new(),
        relay_port,
        session_path: root
            .join("camoufox/state")
            .join(&session_id)
            .join("session.json"),
    });
    let running = active_snapshot(&guard.runtime, &root);
    assert_eq!(
        running["hostPid"].as_u64().expect("FP3 Host PID") as u32,
        host_pid
    );
    let managed_pids = json_u32_array(&running, "/session/managedPids");
    assert_eq!(
        running["sessionId"].as_str().expect("FP3 session ID"),
        session_id
    );
    failure_context
        .lock()
        .expect("update FP3 failure context")
        .as_mut()
        .expect("FP3 failure context present")
        .managed_pids = managed_pids.clone();
    let wire = running["wire"].as_str().expect("FP3 Host wire");
    let arguments = running
        .pointer("/launchSurface/arguments")
        .and_then(Value::as_array)
        .expect("FP3 typed Host argv")
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join("\n");
    let persisted_session =
        serde_json::to_string(&running["session"]).expect("serialize FP3 persisted Host session");
    let observation = running
        .pointer("/observed/fp3NetworkObservation")
        .expect("FP3 browser network observation");
    let ice_candidates = observation
        .pointer("/ice/candidates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let ice_addresses = ice_candidates
        .iter()
        .filter_map(|candidate| candidate.get("address").and_then(Value::as_str))
        .collect::<Vec<_>>();

    let route_applied = running.pointer("/activation/state").and_then(Value::as_str)
        == Some("running")
        && running
            .pointer("/activation/networkEvidence/endpoint")
            .and_then(Value::as_str)
            == Some("reachable")
        && running
            .pointer("/activation/networkEvidence/browserRouting")
            .and_then(Value::as_str)
            == Some("applied");
    let route_binding_exact = running.get("browserProxyServer").and_then(Value::as_str)
        == Some(relay_uri.as_str())
        && wire.matches(&relay_uri).count() == 1;
    let upstream_absent_from_host = !arguments.contains(&upstream_endpoint)
        && !wire.contains(&upstream_endpoint)
        && !persisted_session.contains(&upstream_endpoint);
    let exit_matches = observation
        .pointer("/publicExit/success")
        .and_then(Value::as_bool)
        == Some(true)
        && observation
            .pointer("/publicExit/ip")
            .and_then(Value::as_str)
            == Some(expected_ip.as_str())
        && expected_ip != direct_ip
        && observation
            .pointer("/publicExit/countryCode")
            .and_then(Value::as_str)
            == Some(expected_country.as_str());
    let timezone_matches =
        observation.get("timezone").and_then(Value::as_str) == Some(expected_timezone.as_str());
    let locale_matches =
        observation.get("locale").and_then(Value::as_str) == Some(expected_locale.as_str());
    let geolocation_matches = observation
        .pointer("/geolocationPermission/status")
        .and_then(Value::as_str)
        == Some("granted")
        && observation
            .pointer("/geolocation/status")
            .and_then(Value::as_str)
            == Some("observed")
        && observation
            .pointer("/geolocation/latitude")
            .and_then(Value::as_f64)
            .is_some_and(|value| (value - expected_latitude).abs() <= 1e-6)
        && observation
            .pointer("/geolocation/longitude")
            .and_then(Value::as_f64)
            .is_some_and(|value| (value - expected_longitude).abs() <= 1e-6);
    let ice_matches = observation.get("stunUrl").and_then(Value::as_str) == Some(stun_url.as_str())
        && observation
            .pointer("/ice/completed")
            .and_then(Value::as_bool)
            == Some(true)
        && observation
            .pointer("/ice/timedOut")
            .and_then(Value::as_bool)
            == Some(false)
        && !ice_candidates.is_empty()
        && ice_addresses.contains(&expected_ip.as_str())
        && !ice_addresses.contains(&direct_ip.as_str());
    let artifact_binding_exact = running.get("artifactFileSha256").and_then(Value::as_str)
        == Some(artifact_sha256.as_str())
        && running.get("artifactId").and_then(Value::as_str) == Some(artifact_id.as_str());

    let stopped_activation = guard
        .runtime
        .stop_managed_camoufox(silo.id)
        .expect("cleanly stop FP3 native discriminator");
    let exact_child_success = guard.runtime.child.is_none();
    let stopped = session_after_stop(&root, &session_id);
    let mut owned_pids = managed_pids.clone();
    owned_pids.push(host_pid);
    let residual_owned = wait_for_pids_dead(&owned_pids, Duration::from_secs(30));
    let relay_closed = TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], relay_port)),
        Duration::from_millis(250),
    )
    .is_err();
    let clean_close = stopped_activation.state == RuntimeState::Stopped
        && guard.runtime.profile_lease.is_none()
        && guard.runtime.proxy_relay.is_none()
        && relay_closed
        && exact_child_success
        && residual_owned.is_empty()
        && stopped.get("state").and_then(Value::as_str) == Some("exited")
        && stopped.get("exitStatus").and_then(Value::as_i64) == Some(0)
        && stopped.get("exitFileObserved").and_then(Value::as_bool) == Some(true)
        && stopped.get("quarantine").is_none_or(Value::is_null)
        && stopped
            .pointer("/closeOutcome/status")
            .and_then(Value::as_str)
            == Some("success")
        && stopped
            .pointer("/closeOutcome/contextClose/ctx/status")
            .and_then(Value::as_str)
            == Some("success")
        && matches!(
            stopped
                .pointer("/closeOutcome/contextClose/page/status")
                .and_then(Value::as_str),
            Some("success") | Some("not_present")
        )
        && stopped
            .pointer("/closeOutcome/gracefulProcessExit/status")
            .and_then(Value::as_str)
            == Some("success")
        && stopped
            .pointer("/closeOutcome/forcedJobCleanup/status")
            .and_then(Value::as_str)
            == Some("not_needed")
        && stopped
            .pointer("/processTreeExit/exited")
            .and_then(Value::as_bool)
            == Some(true)
        && stopped
            .pointer("/processTreeExit/sigkill")
            .and_then(Value::as_bool)
            == Some(false)
        && stopped
            .pointer("/processTreeExit/job/terminateJobObject")
            .and_then(Value::as_bool)
            == Some(false)
        && stopped
            .pointer("/processTreeExit/job/activeProcessCount")
            .and_then(Value::as_u64)
            == Some(0);

    let checks = [
        ("routeApplied", route_applied),
        ("routeBindingExactThroughStatus", route_binding_exact),
        ("upstreamAbsentFromHostSurfaces", upstream_absent_from_host),
        ("artifactBindingExact", artifact_binding_exact),
        (
            "identityDeriverNotCalled",
            !deriver_called.load(Ordering::SeqCst),
        ),
        ("browserExitMatchesArtifactAndNotDirect", exit_matches),
        ("browserTimezoneMatchesArtifact", timezone_matches),
        ("browserLocaleMatchesArtifact", locale_matches),
        ("browserGeolocationMatchesArtifact", geolocation_matches),
        ("iceContainsArtifactAndNotDirect", ice_matches),
        ("cleanClose", clean_close),
    ];
    let passed = checks.iter().all(|(_, value)| *value);
    let checks = checks
        .into_iter()
        .map(|(name, value)| (name.to_owned(), Value::Bool(value)))
        .collect::<serde_json::Map<_, _>>();
    let evidence = json!({
        "schema": "verisilo-camoufox-fp3-1b-native-evidence/v1",
        "status": if passed { "passed" } else { "failed" },
        "evidenceClass": "observed-on-this-native-windows-host",
        "verified": false,
        "fixedInputs": {
            "artifactId": artifact_id,
            "artifactFileSha256": artifact_sha256,
            "artifactNetworkIdentity": artifact_network_identity,
            "requiredProxy": { "scheme": "socks5", "host": proxy_host, "port": proxy_port, "authentication": "none" },
            "expectedPublicAddress": expected_ip,
            "directPublicAddress": direct_ip,
            "countryCode": expected_country,
            "timezone": expected_timezone,
            "locale": expected_locale,
            "latitude": expected_latitude,
            "longitude": expected_longitude,
            "stunUrl": stun_url,
            "probePort": probe_port
        },
        "checks": checks,
        "route": { "relayUri": relay_uri, "relayPortClosed": relay_closed },
        "running": running,
        "stopped": stopped,
        "exactHostChildExitConfirmed": exact_child_success,
        "residualProcessCheck": { "ownedPids": owned_pids, "aliveOwnedPids": residual_owned },
        "dns": { "actualPath": "unavailable" }
    });
    fs::create_dir_all(run_dir).expect("create FP3 evidence parent");
    write_new_json(evidence_path, &evidence);
    assert!(passed, "FP3-1b native discriminator failed: {checks:?}");
    guard.armed = false;
}

#[test]
#[ignore = "requires native Windows interactive desktop and pinned real Camoufox asset"]
fn m3_wi_windows_r1_runtime_manager_five_cycle_soak() {
    assert_eq!(required_env("VERISILO_M3_WI_ALLOW_REAL_BROWSER"), "1");
    let run_id = required_env("VERISILO_M3_WI_RUN_ID");
    let code_revision = required_env("VERISILO_M3_WI_CODE_REVISION");
    let code_tree = required_env("VERISILO_M3_WI_CODE_TREE");
    let branch = required_env("VERISILO_M3_WI_BRANCH");
    let tree_raw_sha256 = required_env("VERISILO_M3_WI_TREE_RAW_SHA256");
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonical repository root");
    let run_dir = repo_root
        .join("artifacts/camoufox-m3-wi-windows-gate/runs")
        .join(&run_id);
    assert!(!run_dir.exists(), "M3-WI R1 run-id already exists");
    fs::create_dir_all(&run_dir).expect("create unique M3-WI R1 run root");
    let cache_root = run_dir.join("cache");
    fs::create_dir_all(&cache_root).expect("create run-owned Camoufox cache root");
    let _cache_environment = EnvironmentGuard::set("VERISILO_CAMOUFOX_CACHE_DIR", &cache_root);

    let reservation = TcpListener::bind("127.0.0.1:0").expect("reserve bounded probe port");
    let probe_port = reservation
        .local_addr()
        .expect("reserved probe address")
        .port();
    drop(reservation);

    let deriver_called = Arc::new(AtomicBool::new(false));
    let deriver = SentinelDeriver {
        called: Arc::clone(&deriver_called),
    };
    let mut unrelated = ExactChildGuard::start();
    let unrelated_pid = unrelated.pid();
    let root = scenario_root(&run_dir, "r1-reliability-soak", &repo_root);
    let silo = silo_for(&root, ARTIFACT_SHA256);
    let adapter = adapter_for(
        &repo_root,
        probe_port,
        BROWSER_RELEASE,
        BROWSER_ASSET_SHA256,
        &tree_raw_sha256,
    );
    let mut cycles = Vec::new();
    let mut owned_pids = BTreeSet::new();
    let mut secret_values = Vec::new();
    let mut secret_text = Vec::new();
    let mut stable_digest: Option<Value> = None;
    let mut stable_profile: Option<Value> = None;

    for cycle in 1..=5_u64 {
        let mut guard = launch_real(&root, adapter.clone(), &silo, &deriver)
            .unwrap_or_else(|error| panic!("launch R1 reliability cycle {cycle}: {error}"));
        let running = active_snapshot(&guard.runtime, &root);
        assert_running_evidence(&running);
        assert_eq!(
            running
                .pointer("/session/bootCountBefore")
                .and_then(Value::as_u64),
            Some(cycle - 1)
        );
        assert_eq!(
            running
                .pointer("/session/bootCountAfter")
                .and_then(Value::as_u64),
            Some(cycle)
        );
        for pointer in [
            "/session/cookieEvidence/cookieInApi",
            "/session/cookieEvidence/cookieOnPage",
            "/session/cookieEvidence/cookieValueLooksManaged",
        ] {
            assert_eq!(
                running.pointer(pointer).and_then(Value::as_bool),
                Some(true),
                "R1 cycle {cycle} missing cookie evidence at {pointer}"
            );
        }
        if let Some(expected) = stable_digest.as_ref() {
            assert_eq!(
                running.get("observedWebsiteDigest"),
                Some(expected),
                "ObservedWebsiteDigest drifted in R1 cycle {cycle}"
            );
        } else {
            stable_digest = running.get("observedWebsiteDigest").cloned();
        }
        if let Some(expected) = stable_profile.as_ref() {
            assert_eq!(running.get("profileId"), Some(expected));
        } else {
            stable_profile = running.get("profileId").cloned();
        }

        let host_pid = running["hostPid"].as_u64().expect("R1 Host PID") as u32;
        let managed = json_u32_array(&running, "/session/managedPids");
        let session_id = running["sessionId"]
            .as_str()
            .expect("R1 Host session ID")
            .to_owned();
        owned_pids.insert(host_pid);
        owned_pids.extend(managed.iter().copied());

        let stopped_activation = guard
            .runtime
            .stop_managed_camoufox(silo.id)
            .unwrap_or_else(|error| panic!("close R1 reliability cycle {cycle}: {error}"));
        assert_eq!(stopped_activation.state, RuntimeState::Stopped);
        assert!(guard.runtime.profile_lease.is_none());
        let stopped = session_after_stop(&root, &session_id);
        assert_clean_stop(&stopped, host_pid, &managed);
        assert_eq!(stopped.get("exitStatus").and_then(Value::as_i64), Some(0));
        assert_eq!(
            stopped.get("exitFileObserved").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            stopped
                .pointer("/processTreeExit/job/activeProcessCount")
                .and_then(Value::as_u64),
            Some(0)
        );
        for pointer in [
            "/cookieSqlite/fileExists",
            "/cookieSqlite/cookieNamePresent",
            "/cookieSqlite/valuesManaged",
        ] {
            assert_eq!(
                stopped.pointer(pointer).and_then(Value::as_bool),
                Some(true),
                "R1 cycle {cycle} missing SQLite evidence at {pointer}"
            );
        }
        assert_eq!(
            stopped
                .pointer("/cookieSqlite/sqliteRetryExhausted")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert!(stopped.pointer("/cookieSqlite/sqliteReadError").is_none());

        let diagnostics = json!(stage_diagnostics(&root));
        assert_stage_diagnostics(&diagnostics);
        secret_values.push(running.clone());
        secret_values.push(stopped.clone());
        secret_text.push(serde_json::to_string(&diagnostics).expect("serialize R1 diagnostics"));
        cycles.push(json!({
            "cycle": cycle,
            "running": running,
            "closeReceipt": stopped,
            "profileId": running["profileId"],
            "sessionId": session_id,
            "hostPid": host_pid,
            "managedPids": managed,
            "bootCountBefore": running.pointer("/session/bootCountBefore"),
            "bootCountAfter": running.pointer("/session/bootCountAfter"),
            "observedWebsiteDigest": running["observedWebsiteDigest"],
            "cookieApi": running["session"]["cookieEvidence"],
            "pageCookie": running["session"]["cookieEvidence"]["cookieOnPage"],
            "sqlite": stopped["cookieSqlite"],
            "close": {
                "exitStatus": stopped["exitStatus"],
                "exitFile": stopped["exitFileObserved"],
                "jobActiveProcessCount": stopped["processTreeExit"]["job"]["activeProcessCount"],
                "processTreeExited": stopped["processTreeExit"]["exited"],
            },
            "stageDiagnostics": diagnostics,
            "verified": false,
            "evidenceClass": "observed-on-this-windows-host",
        }));
        unrelated.assert_alive();
    }

    assert!(!deriver_called.load(Ordering::SeqCst));
    for value in &cycles {
        secret_values.push(value.clone());
    }
    let secret_refs = secret_values.iter().collect::<Vec<_>>();
    let secret_scan = scan_secret_surfaces(&secret_refs, &secret_text);
    assert_eq!(
        secret_scan
            .get("matches")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(0)
    );

    let residual_owned = wait_for_pids_dead(
        &owned_pids.iter().copied().collect::<Vec<_>>(),
        Duration::from_secs(30),
    );
    assert!(residual_owned.is_empty(), "R1 left owned processes alive");
    unrelated.assert_alive();

    let report = json!({
        "schema": "verisilo-camoufox-m3-wi-r1-windows-runtime-evidence/v1",
        "status": "passed",
        "runId": run_id,
        "codeGitRevision": code_revision,
        "codeTreeHash": code_tree,
        "branch": branch,
        "integrationPath": "test-only-real-host",
        "productionPackageVerified": false,
        "shipped": false,
        "verified": false,
        "evidenceClass": "observed-on-this-windows-host",
        "fixedInputs": {
            "artifactId": ARTIFACT_ID,
            "artifactFileSha256": ARTIFACT_SHA256,
            "browserRelease": BROWSER_RELEASE,
            "browserAssetSha256": BROWSER_ASSET_SHA256,
            "browserTreeManifestRawSha256": tree_raw_sha256,
            "browserTreeManifestCanonicalSha256": TREE_CANONICAL_SHA256,
            "probePort": probe_port,
        },
        "sameProfile": true,
        "cycleCount": 5,
        "launchTimeoutSeconds": 120,
        "closeTimeoutSeconds": 10,
        "cycles": cycles,
        "observedWebsiteDigestStable": true,
        "secretScan": secret_scan,
        "unrelatedSentinel": {
            "pid": unrelated_pid,
            "survivedAllLifecycleOperations": true,
        },
        "residualProcessCheck": {
            "ownedPids": owned_pids,
            "aliveOwnedPids": residual_owned,
        },
        "semanticBoundary": {
            "launchPath": "RuntimeManager.launch_with_identity_deriver -> test-only adapter plan -> spawn_camoufox_host -> CamoufoxHostJsonlV1",
            "closePath": "RuntimeManager.stop_managed_camoufox -> Host close -> Host shutdown -> exact child wait",
            "launchExecutable": "uv-resolved-locked-python-interpreter",
            "hostEntrypoint": "apps/camoufox-host/host_v1.py",
            "typedHostArgvRecorded": true,
            "argvContainsProxyArguments": false,
            "argvContainsSecrets": false,
            "verifiedAdapter": null,
            "productionPackageVerified": false,
        },
    });
    let report_path = run_dir.join("r1-runtime-evidence.json");
    fs::write(
        &report_path,
        serde_json::to_vec_pretty(&report).expect("serialize M3-WI R1 runtime evidence"),
    )
    .expect("write M3-WI R1 runtime evidence");
    println!("m3-wi-r1-run-id={run_id}");
    println!("m3-wi-r1-runtime-evidence={}", report_path.display());
}

#[test]
#[ignore = "requires native Windows interactive desktop and pinned real Camoufox asset"]
fn m3_wi_windows_r2_runtime_manager_ten_cycle_clean_close_soak() {
    assert_eq!(required_env("VERISILO_M3_WI_ALLOW_REAL_BROWSER"), "1");
    let run_id = required_env("VERISILO_M3_WI_RUN_ID");
    let code_revision = required_env("VERISILO_M3_WI_CODE_REVISION");
    let code_tree = required_env("VERISILO_M3_WI_CODE_TREE");
    let branch = required_env("VERISILO_M3_WI_BRANCH");
    let tree_raw_sha256 = required_env("VERISILO_M3_WI_TREE_RAW_SHA256");
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonical repository root");
    let run_dir = repo_root
        .join("artifacts/camoufox-m3-wi-windows-gate/runs")
        .join(&run_id);
    assert!(!run_dir.exists(), "M3-WI R2 run-id already exists");
    fs::create_dir_all(&run_dir).expect("create unique M3-WI R2 run root");
    let cache_root = run_dir.join("cache");
    fs::create_dir_all(&cache_root).expect("create run-owned Camoufox cache root");
    let _cache_environment = EnvironmentGuard::set("VERISILO_CAMOUFOX_CACHE_DIR", &cache_root);

    let reservation = TcpListener::bind("127.0.0.1:0").expect("reserve bounded probe port");
    let probe_port = reservation
        .local_addr()
        .expect("reserved probe address")
        .port();
    drop(reservation);

    let deriver_called = Arc::new(AtomicBool::new(false));
    let deriver = SentinelDeriver {
        called: Arc::clone(&deriver_called),
    };
    let mut unrelated = ExactChildGuard::start();
    let unrelated_pid = unrelated.pid();
    let root = scenario_root(&run_dir, "r2-reliability-soak", &repo_root);
    let silo = silo_for(&root, ARTIFACT_SHA256);
    let adapter = adapter_for(
        &repo_root,
        probe_port,
        BROWSER_RELEASE,
        BROWSER_ASSET_SHA256,
        &tree_raw_sha256,
    );
    let mut cycles = Vec::new();
    let mut owned_pids = BTreeSet::new();
    let mut secret_values = Vec::new();
    let mut secret_text = Vec::new();
    let mut stable_digest: Option<Value> = None;
    let mut stable_profile: Option<Value> = None;

    for cycle in 1..=10_u64 {
        let mut guard = launch_real(&root, adapter.clone(), &silo, &deriver)
            .unwrap_or_else(|error| panic!("launch R2 reliability cycle {cycle}: {error}"));
        let running = active_snapshot(&guard.runtime, &root);
        assert_running_evidence(&running);
        assert_eq!(
            running
                .pointer("/session/bootCountBefore")
                .and_then(Value::as_u64),
            Some(cycle - 1)
        );
        assert_eq!(
            running
                .pointer("/session/bootCountAfter")
                .and_then(Value::as_u64),
            Some(cycle)
        );
        for pointer in [
            "/session/cookieEvidence/cookieInApi",
            "/session/cookieEvidence/cookieOnPage",
            "/session/cookieEvidence/cookieValueLooksManaged",
        ] {
            assert_eq!(
                running.pointer(pointer).and_then(Value::as_bool),
                Some(true),
                "R2 cycle {cycle} missing cookie evidence at {pointer}"
            );
        }
        if let Some(expected) = stable_digest.as_ref() {
            assert_eq!(
                running.get("observedWebsiteDigest"),
                Some(expected),
                "ObservedWebsiteDigest drifted in R2 cycle {cycle}"
            );
        } else {
            stable_digest = running.get("observedWebsiteDigest").cloned();
        }
        if let Some(expected) = stable_profile.as_ref() {
            assert_eq!(running.get("profileId"), Some(expected));
        } else {
            stable_profile = running.get("profileId").cloned();
        }

        let host_pid = running["hostPid"].as_u64().expect("R2 Host PID") as u32;
        let managed = json_u32_array(&running, "/session/managedPids");
        let session_id = running["sessionId"]
            .as_str()
            .expect("R2 Host session ID")
            .to_owned();
        owned_pids.insert(host_pid);
        owned_pids.extend(managed.iter().copied());

        let stopped_activation = guard
            .runtime
            .stop_managed_camoufox(silo.id)
            .unwrap_or_else(|error| panic!("close R2 reliability cycle {cycle}: {error}"));
        assert_eq!(stopped_activation.state, RuntimeState::Stopped);
        assert!(guard.runtime.profile_lease.is_none());
        let stopped = session_after_stop(&root, &session_id);
        assert_clean_stop(&stopped, host_pid, &managed);
        assert_r2_clean_close(&stopped);
        assert_eq!(stopped.get("exitStatus").and_then(Value::as_i64), Some(0));
        assert_eq!(
            stopped.get("exitFileObserved").and_then(Value::as_bool),
            Some(true)
        );
        for pointer in [
            "/cookieSqlite/fileExists",
            "/cookieSqlite/cookieNamePresent",
            "/cookieSqlite/valuesManaged",
        ] {
            assert_eq!(
                stopped.pointer(pointer).and_then(Value::as_bool),
                Some(true),
                "R2 cycle {cycle} missing SQLite evidence at {pointer}"
            );
        }
        assert_eq!(
            stopped
                .pointer("/cookieSqlite/sqliteRetryExhausted")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert!(stopped.pointer("/cookieSqlite/sqliteReadError").is_none());

        let diagnostics = json!(stage_diagnostics(&root));
        assert_r2_diagnostics(&diagnostics);
        secret_values.push(running.clone());
        secret_values.push(stopped.clone());
        secret_text.push(serde_json::to_string(&diagnostics).expect("serialize R2 diagnostics"));
        cycles.push(json!({
            "cycle": cycle,
            "running": running,
            "closeReceipt": stopped,
            "profileId": running["profileId"],
            "sessionId": session_id,
            "hostPid": host_pid,
            "managedPids": managed,
            "bootCountBefore": cycle - 1,
            "bootCountAfter": cycle,
            "observedWebsiteDigest": running["observedWebsiteDigest"],
            "cookieApi": running["session"]["cookieEvidence"],
            "pageCookie": running["session"]["cookieEvidence"]["cookieOnPage"],
            "sqlite": stopped["cookieSqlite"],
            "close": {
                "exitStatus": stopped["exitStatus"],
                "exitFile": stopped["exitFileObserved"],
                "jobActiveProcessCount": stopped["processTreeExit"]["job"]["activeProcessCount"],
                "processTreeExited": stopped["processTreeExit"]["exited"],
                "sigkill": stopped["processTreeExit"]["sigkill"],
                "terminateJobObject": stopped["processTreeExit"]["job"]["terminateJobObject"],
                "closeOutcome": stopped["closeOutcome"],
            },
            "stageDiagnostics": diagnostics,
            "verified": false,
            "evidenceClass": "observed-on-this-windows-host",
        }));
        unrelated.assert_alive();
    }

    assert!(!deriver_called.load(Ordering::SeqCst));
    for value in &cycles {
        secret_values.push(value.clone());
    }
    let secret_refs = secret_values.iter().collect::<Vec<_>>();
    let secret_scan = scan_secret_surfaces(&secret_refs, &secret_text);
    assert_eq!(
        secret_scan
            .get("matches")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(0)
    );

    let residual_owned = wait_for_pids_dead(
        &owned_pids.iter().copied().collect::<Vec<_>>(),
        Duration::from_secs(30),
    );
    assert!(residual_owned.is_empty(), "R2 left owned processes alive");
    unrelated.assert_alive();

    let report = json!({
        "schema": "verisilo-camoufox-m3-wi-r2-windows-runtime-evidence/v1",
        "status": "passed",
        "runId": run_id,
        "codeGitRevision": code_revision,
        "codeTreeHash": code_tree,
        "branch": branch,
        "integrationPath": "test-only-real-host",
        "productionPackageVerified": false,
        "shipped": false,
        "verified": false,
        "evidenceClass": "observed-on-this-windows-host",
        "fixedInputs": {
            "artifactId": ARTIFACT_ID,
            "artifactFileSha256": ARTIFACT_SHA256,
            "browserRelease": BROWSER_RELEASE,
            "browserAssetSha256": BROWSER_ASSET_SHA256,
            "browserTreeManifestRawSha256": tree_raw_sha256,
            "browserTreeManifestCanonicalSha256": TREE_CANONICAL_SHA256,
            "probePort": probe_port,
        },
        "sameProfile": true,
        "cycleCount": 10,
        "launchTimeoutSeconds": 120,
        "closeTimeoutSeconds": 10,
        "forcedCleanupObserved": false,
        "closeLifecycle": {
            "allContextCloses": true,
            "allGracefulProcessExits": true,
            "forcedCleanupCount": 0,
            "sigkillCount": 0,
            "terminateJobObjectCount": 0,
        },
        "cycles": cycles,
        "observedWebsiteDigestStable": true,
        "secretScan": secret_scan,
        "unrelatedSentinel": {
            "pid": unrelated_pid,
            "survivedAllLifecycleOperations": true,
        },
        "residualProcessCheck": {
            "ownedPids": owned_pids,
            "aliveOwnedPids": residual_owned,
        },
        "semanticBoundary": {
            "launchPath": "RuntimeManager.launch_with_identity_deriver -> test-only adapter plan -> spawn_camoufox_host -> CamoufoxHostJsonlV1",
            "closePath": "RuntimeManager.stop_managed_camoufox -> Host close(page -> ctx) -> Host shutdown -> exact child wait",
            "launchExecutable": "uv-resolved-locked-python-interpreter",
            "hostEntrypoint": "apps/camoufox-host/host_v1.py",
            "typedHostArgvRecorded": true,
            "argvContainsProxyArguments": false,
            "argvContainsSecrets": false,
            "verifiedAdapter": null,
            "productionPackageVerified": false,
        },
    });
    let report_path = run_dir.join("r2-runtime-evidence.json");
    fs::write(
        &report_path,
        serde_json::to_vec_pretty(&report).expect("serialize M3-WI R2 runtime evidence"),
    )
    .expect("write M3-WI R2 runtime evidence");
    println!("m3-wi-r2-run-id={run_id}");
    println!("m3-wi-r2-runtime-evidence={}", report_path.display());
}

#[test]
#[ignore = "requires native Windows interactive desktop and pinned real Camoufox asset"]
fn m3_wi_windows_real_host_runtime_manager_gate() {
    assert_eq!(required_env("VERISILO_M3_WI_ALLOW_REAL_BROWSER"), "1");
    let run_id = required_env("VERISILO_M3_WI_RUN_ID");
    let code_revision = required_env("VERISILO_M3_WI_CODE_REVISION");
    let code_tree = required_env("VERISILO_M3_WI_CODE_TREE");
    let branch = required_env("VERISILO_M3_WI_BRANCH");
    let tree_raw_sha256 = required_env("VERISILO_M3_WI_TREE_RAW_SHA256");
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonical repository root");
    let run_dir = repo_root
        .join("artifacts/camoufox-m3-wi-windows-gate/runs")
        .join(&run_id);
    assert!(!run_dir.exists(), "M3-WI run-id already exists");
    fs::create_dir_all(&run_dir).expect("create unique M3-WI run root");
    let cache_root = run_dir.join("cache");
    fs::create_dir_all(&cache_root).expect("create run-owned Camoufox cache root");
    let _cache_environment = EnvironmentGuard::set("VERISILO_CAMOUFOX_CACHE_DIR", &cache_root);

    let reservation = TcpListener::bind("127.0.0.1:0").expect("reserve bounded probe port");
    let probe_port = reservation
        .local_addr()
        .expect("reserved probe address")
        .port();
    drop(reservation);

    let deriver_called = Arc::new(AtomicBool::new(false));
    let deriver = SentinelDeriver {
        called: Arc::clone(&deriver_called),
    };
    let mut unrelated = ExactChildGuard::start();
    let unrelated_pid = unrelated.pid();
    let mut owned_pids = BTreeSet::new();
    let mut secret_values = Vec::new();
    let mut secret_text = Vec::new();

    // Persistence cycle 1 + concurrent Profile rejection + clean stop.
    let persistence_root = scenario_root(&run_dir, "persistence", &repo_root);
    let persistence_silo = silo_for(&persistence_root, ARTIFACT_SHA256);
    let adapter = adapter_for(
        &repo_root,
        probe_port,
        BROWSER_RELEASE,
        BROWSER_ASSET_SHA256,
        &tree_raw_sha256,
    );
    let mut first = launch_real(
        &persistence_root,
        adapter.clone(),
        &persistence_silo,
        &deriver,
    )
    .expect("launch first real Host/browser persistence cycle");
    let first_running = active_snapshot(&first.runtime, &persistence_root);
    assert_running_evidence(&first_running);
    let first_host_pid = first_running["hostPid"].as_u64().unwrap() as u32;
    let first_managed = json_u32_array(&first_running, "/session/managedPids");
    owned_pids.insert(first_host_pid);
    owned_pids.extend(first_managed.iter().copied());
    assert_eq!(
        first_running
            .pointer("/session/bootCountBefore")
            .and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(
        first_running
            .pointer("/session/bootCountAfter")
            .and_then(Value::as_u64),
        Some(1)
    );

    let mut concurrent = RuntimeManager {
        record_path: Some(persistence_root.join("runtime/concurrent-browser-session.json")),
        ..RuntimeManager::default()
    };
    concurrent.set_test_engine_adapter(Box::new(adapter.clone()));
    let concurrent_error = concurrent
        .launch(
            &persistence_silo,
            &managed_profiles(&persistence_silo),
            None,
            None,
        )
        .expect_err("concurrent same-Profile desktop launch must fail closed");
    assert!(matches!(concurrent_error, LauncherError::ProfileInUse));
    assert!(concurrent.child.is_none());
    unrelated.assert_alive();

    let first_session_id = first_running["sessionId"].as_str().unwrap().to_owned();
    let first_stopped_activation = first
        .runtime
        .stop_managed_camoufox(persistence_silo.id)
        .expect("cleanly stop first real Host/browser cycle");
    assert_eq!(first_stopped_activation.state, RuntimeState::Stopped);
    assert!(first.runtime.profile_lease.is_none());
    let first_stopped = session_after_stop(&persistence_root, &first_session_id);
    assert_clean_stop(&first_stopped, first_host_pid, &first_managed);
    unrelated.assert_alive();

    // Persistence cycle 2 is a new RuntimeManager and a distinct Host child,
    // but uses the exact same Silo/Profile/Artifact and fixed probe origin.
    let mut second = launch_real(
        &persistence_root,
        adapter.clone(),
        &persistence_silo,
        &deriver,
    )
    .expect("launch second real Host/browser persistence cycle");
    let second_running = active_snapshot(&second.runtime, &persistence_root);
    assert_running_evidence(&second_running);
    let second_host_pid = second_running["hostPid"].as_u64().unwrap() as u32;
    let second_managed = json_u32_array(&second_running, "/session/managedPids");
    owned_pids.insert(second_host_pid);
    owned_pids.extend(second_managed.iter().copied());
    assert_ne!(first_host_pid, second_host_pid);
    assert_eq!(
        second_running
            .pointer("/session/bootCountBefore")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        second_running
            .pointer("/session/bootCountAfter")
            .and_then(Value::as_u64),
        Some(2)
    );
    for pointer in [
        "/session/cookieEvidence/cookieInApi",
        "/session/cookieEvidence/cookieOnPage",
        "/session/cookieEvidence/cookieValueLooksManaged",
    ] {
        assert_eq!(
            second_running.pointer(pointer).and_then(Value::as_bool),
            Some(true)
        );
    }
    assert_eq!(
        first_running["observedWebsiteDigest"],
        second_running["observedWebsiteDigest"]
    );
    let second_session_id = second_running["sessionId"].as_str().unwrap().to_owned();
    let second_stopped_activation = second
        .runtime
        .stop_managed_camoufox(persistence_silo.id)
        .expect("cleanly stop second real Host/browser cycle");
    assert_eq!(second_stopped_activation.state, RuntimeState::Stopped);
    let second_stopped = session_after_stop(&persistence_root, &second_session_id);
    assert_clean_stop(&second_stopped, second_host_pid, &second_managed);
    assert_eq!(
        second_stopped
            .pointer("/cookieSqlite/fileExists")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        second_stopped
            .pointer("/cookieSqlite/cookieNamePresent")
            .and_then(Value::as_bool),
        Some(true)
    );
    unrelated.assert_alive();

    // Active-session stdin EOF: Host cleans the Job tree, while desktop keeps
    // the Profile lease and publishes VerificationFailed (never Stopped).
    let eof_root = scenario_root(&run_dir, "active-eof", &repo_root);
    let eof_silo = silo_for(&eof_root, ARTIFACT_SHA256);
    let mut eof_runtime = launch_real(&eof_root, adapter.clone(), &eof_silo, &deriver)
        .expect("launch real Host/browser for active EOF");
    let eof_running = active_snapshot(&eof_runtime.runtime, &eof_root);
    let eof_host_pid = eof_running["hostPid"].as_u64().unwrap() as u32;
    let eof_managed = json_u32_array(&eof_running, "/session/managedPids");
    let eof_session_id = eof_running["sessionId"].as_str().unwrap().to_owned();
    owned_pids.insert(eof_host_pid);
    owned_pids.extend(eof_managed.iter().copied());
    match eof_runtime.runtime.engine_runtime.as_mut() {
        Some(EngineRuntimeProtocol::CamoufoxHost(host)) => host.transport.close_exact_stdin(),
        _ => panic!("active EOF lost exact Host transport"),
    }
    wait_exact_child_exit(&mut eof_runtime.runtime, Duration::from_secs(120));
    eof_runtime.runtime.refresh();
    assert_eq!(
        eof_runtime.runtime.activation().state,
        RuntimeState::VerificationFailed
    );
    assert!(eof_runtime.runtime.profile_lease.is_some());
    let eof_stopped = session_after_stop(&eof_root, &eof_session_id);
    assert_clean_stop(&eof_stopped, eof_host_pid, &eof_managed);
    unrelated.assert_alive();

    // Exact Host crash: desktop cannot accept a close receipt, so it retains
    // the lease and fails closed.  The Windows supervisor/Job still reaps only
    // the owned browser tree.
    let crash_root = scenario_root(&run_dir, "host-crash", &repo_root);
    let crash_silo = silo_for(&crash_root, ARTIFACT_SHA256);
    let mut crash_runtime = launch_real(&crash_root, adapter.clone(), &crash_silo, &deriver)
        .expect("launch real Host/browser for exact Host crash");
    let crash_running = active_snapshot(&crash_runtime.runtime, &crash_root);
    let crash_host_pid = crash_running["hostPid"].as_u64().unwrap() as u32;
    let crash_managed = json_u32_array(&crash_running, "/session/managedPids");
    owned_pids.insert(crash_host_pid);
    owned_pids.extend(crash_managed.iter().copied());
    crash_runtime
        .runtime
        .child
        .as_mut()
        .expect("exact crash Host child")
        .kill()
        .expect("kill only exact M3-WI Host child");
    wait_exact_child_exit(&mut crash_runtime.runtime, Duration::from_secs(30));
    crash_runtime.runtime.refresh();
    assert_eq!(
        crash_runtime.runtime.activation().state,
        RuntimeState::VerificationFailed
    );
    assert!(crash_runtime.runtime.profile_lease.is_some());
    let mut crash_pids = crash_managed.clone();
    crash_pids.push(crash_host_pid);
    assert!(wait_for_pids_dead(&crash_pids, Duration::from_secs(30)).is_empty());
    unrelated.assert_alive();

    // Desktop-control drop closes the exact stdin pipe.  A subsequent desktop
    // instance reads the still-Running record as RecoveryRequired; it never
    // silently publishes Stopped or reuses the Profile.
    let desktop_root = scenario_root(&run_dir, "desktop-drop", &repo_root);
    let desktop_silo = silo_for(&desktop_root, ARTIFACT_SHA256);
    let desktop_runtime = launch_real(&desktop_root, adapter.clone(), &desktop_silo, &deriver)
        .expect("launch real Host/browser for desktop-control drop");
    let desktop_running = active_snapshot(&desktop_runtime.runtime, &desktop_root);
    let desktop_host_pid = desktop_running["hostPid"].as_u64().unwrap() as u32;
    let desktop_managed = json_u32_array(&desktop_running, "/session/managedPids");
    let desktop_session_id = desktop_running["sessionId"].as_str().unwrap().to_owned();
    owned_pids.insert(desktop_host_pid);
    owned_pids.extend(desktop_managed.iter().copied());
    let desktop_owned = desktop_runtime.into_desktop_drop();
    drop(desktop_owned);
    let mut desktop_pids = desktop_managed.clone();
    desktop_pids.push(desktop_host_pid);
    assert!(wait_for_pids_dead(&desktop_pids, Duration::from_secs(120)).is_empty());
    let mut recovered = RuntimeManager::open(&desktop_root);
    assert_eq!(
        recovered
            .activation
            .as_ref()
            .map(|activation| &activation.state),
        Some(&RuntimeState::RecoveryRequired)
    );
    assert_eq!(
        recovered
            .activation
            .as_ref()
            .and_then(|activation| activation.active_silo_id),
        Some(desktop_silo.id)
    );
    let recovery_error = recovered
        .launch(&desktop_silo, &managed_profiles(&desktop_silo), None, None)
        .expect_err("RecoveryRequired desktop record must block Profile reuse");
    assert!(matches!(recovery_error, LauncherError::AnotherSiloRunning));
    let desktop_stopped = session_after_stop(&desktop_root, &desktop_session_id);
    assert_clean_stop(&desktop_stopped, desktop_host_pid, &desktop_managed);
    unrelated.assert_alive();

    // Real Host, pre-browser fail-closed binding negatives.
    let mut negatives = Vec::new();
    for (name, artifact_sha, release, asset_sha, tree_sha, expected_fragment) in [
        (
            "artifact-raw-sha",
            "0".repeat(64),
            BROWSER_RELEASE.to_owned(),
            BROWSER_ASSET_SHA256.to_owned(),
            tree_raw_sha256.clone(),
            "artifact file sha256 mismatch",
        ),
        (
            "browser-tree-sha",
            ARTIFACT_SHA256.to_owned(),
            BROWSER_RELEASE.to_owned(),
            BROWSER_ASSET_SHA256.to_owned(),
            "0".repeat(64),
            "hello did not match",
        ),
        (
            "host-release-binding",
            ARTIFACT_SHA256.to_owned(),
            "v152.0.4-beta.27".to_owned(),
            BROWSER_ASSET_SHA256.to_owned(),
            tree_raw_sha256.clone(),
            "hello did not match",
        ),
    ] {
        let root = scenario_root(&run_dir, name, &repo_root);
        let silo = silo_for(&root, &artifact_sha);
        let mut runtime = new_runtime(
            &root,
            adapter_for(&repo_root, probe_port, &release, &asset_sha, &tree_sha),
        );
        let error = runtime
            .launch(&silo, &managed_profiles(&silo), None, None)
            .expect_err("real Host negative must fail before Running");
        let error_text = error.to_string();
        assert!(
            error_text.to_ascii_lowercase().contains(expected_fragment),
            "negative {name} returned unexpected error: {error_text}"
        );
        assert!(runtime.child.is_none());
        assert_ne!(runtime.activation().state, RuntimeState::Running);
        secret_text.push(error_text);
        negatives.push(json!({
            "name": name,
            "state": runtime.activation().state,
            "failedBeforeRunning": true,
            "exactSpawnedChildRetained": false,
        }));
    }

    // Send real proxy secret sentinels through the RuntimeManager input
    // boundary. Camoufox rejects the FixedProxy policy before adapter/spawn;
    // neither the failure nor activation evidence may reflect the values.
    let proxy_root = scenario_root(&run_dir, "fixed-proxy-policy", &repo_root);
    let mut proxy_silo = silo_for(&proxy_root, ARTIFACT_SHA256);
    proxy_silo.network_profile = NetworkProfile::FixedProxy {
        proxy_required: true,
        scheme: ProxyScheme::Socks5,
        host: "127.0.0.1".to_owned(),
        port: 9,
        bypass_list: Vec::new(),
        credential_reference: Some(
            Uuid::parse_str("66666666-6666-4666-8666-666666666666").unwrap(),
        ),
        external_mihomo: None,
    };
    let mut proxy_runtime = new_runtime(&proxy_root, adapter.clone());
    let proxy_error = proxy_runtime
        .launch(
            &proxy_silo,
            &managed_profiles(&proxy_silo),
            Some(ProxyAuthentication::new(
                PROXY_USERNAME_SENTINEL.to_owned(),
                PROXY_PASSWORD_SENTINEL.to_owned(),
            )),
            None,
        )
        .expect_err("Camoufox FixedProxy sentinel input must fail before spawn");
    assert!(proxy_error.to_string().contains("only permits a Direct"));
    assert!(proxy_runtime.child.is_none());
    assert_eq!(proxy_runtime.activation().state, RuntimeState::Failed);
    let proxy_activation =
        serde_json::to_value(proxy_runtime.activation()).expect("proxy rejection activation JSON");
    secret_text.push(proxy_error.to_string());
    secret_values.push(proxy_activation);
    negatives.push(json!({
        "name": "fixed-proxy-secret-input",
        "state": proxy_runtime.activation().state,
        "failedBeforeRunning": true,
        "exactSpawnedChildRetained": false,
    }));
    unrelated.assert_alive();

    assert!(!deriver_called.load(Ordering::SeqCst));
    for value in [
        &first_running,
        &first_stopped,
        &second_running,
        &second_stopped,
        &eof_running,
        &eof_stopped,
        &crash_running,
        &desktop_running,
        &desktop_stopped,
    ] {
        secret_values.push(value.clone());
    }
    for scenario in [
        (&persistence_root, &first_session_id),
        (&persistence_root, &second_session_id),
        (&eof_root, &eof_session_id),
        (&desktop_root, &desktop_session_id),
    ] {
        let log_path = scenario
            .0
            .join("camoufox/state")
            .join(scenario.1)
            .join("browser.log");
        secret_text
            .push(String::from_utf8_lossy(&fs::read(log_path).unwrap_or_default()).into_owned());
    }
    let controlled_environment = json!({
        "VERISILO_M3_WI_RUN_ID": run_id,
        "VERISILO_M3_WI_CODE_REVISION": code_revision,
        "VERISILO_M3_WI_CODE_TREE": code_tree,
        "VERISILO_M3_WI_BRANCH": branch,
        "VERISILO_M3_WI_PYTHON_PATH": required_env("VERISILO_M3_WI_PYTHON_PATH"),
        "VERISILO_M3_WI_TREE_RAW_SHA256": tree_raw_sha256,
        "VERISILO_M3_WI_ARTIFACT_PATH": required_env("VERISILO_M3_WI_ARTIFACT_PATH"),
    });
    secret_values.push(controlled_environment.clone());
    let secret_refs = secret_values.iter().collect::<Vec<_>>();
    let secret_scan = scan_secret_surfaces(&secret_refs, &secret_text);
    assert_eq!(
        secret_scan
            .get("matches")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(0)
    );

    let residual_owned = wait_for_pids_dead(
        &owned_pids.iter().copied().collect::<Vec<_>>(),
        Duration::from_secs(30),
    );
    assert!(
        residual_owned.is_empty(),
        "M3-WI left owned processes alive"
    );
    unrelated.assert_alive();

    let report = json!({
        "schema": "verisilo-camoufox-m3-wi-windows-runtime-evidence/v1",
        "status": "passed",
        "runId": run_id,
        "codeGitRevision": code_revision,
        "codeTreeHash": code_tree,
        "branch": branch,
        "integrationPath": "test-only-real-host",
        "productionPackageVerified": false,
        "shipped": false,
        "verified": false,
        "evidenceClass": "observed-on-this-windows-host",
        "fixedInputs": {
            "artifactId": ARTIFACT_ID,
            "artifactFileSha256": ARTIFACT_SHA256,
            "browserRelease": BROWSER_RELEASE,
            "browserAssetSha256": BROWSER_ASSET_SHA256,
            "browserTreeManifestRawSha256": tree_raw_sha256,
            "browserTreeManifestCanonicalSha256": TREE_CANONICAL_SHA256,
            "probePort": probe_port,
        },
        "persistence": {
            "cycle1": first_running,
            "cycle1Close": first_stopped,
            "cycle2": second_running,
            "cycle2Close": second_stopped,
            "hostPidsDistinct": first_host_pid != second_host_pid,
            "bootCounts": [1, 2],
            "profileConcurrentRejected": true,
        },
        "activeEof": {
            "running": eof_running,
            "closedSession": eof_stopped,
            "desktopState": eof_runtime.runtime.activation(),
            "profileLeaseRetained": eof_runtime.runtime.profile_lease.is_some(),
        },
        "hostCrash": {
            "running": crash_running,
            "desktopState": crash_runtime.runtime.activation(),
            "profileLeaseRetained": crash_runtime.runtime.profile_lease.is_some(),
            "ownedPidsDead": true,
        },
        "desktopDrop": {
            "running": desktop_running,
            "closedSession": desktop_stopped,
            "recoveredState": recovered.activation,
            "ownedPidsDead": true,
        },
        "negativeMatrix": negatives,
        "controlledEnvironment": controlled_environment,
        "secretScan": secret_scan,
        "unrelatedSentinel": {
            "pid": unrelated_pid,
            "survivedAllLifecycleOperations": true,
        },
        "residualProcessCheck": {
            "ownedPids": owned_pids,
            "aliveOwnedPids": residual_owned,
        },
        "semanticBoundary": {
            "launchExecutable": "uv-resolved-locked-python-interpreter",
            "hostEntrypoint": "apps/camoufox-host/host_v1.py",
            "typedHostArgvRecorded": true,
            "argvContainsProxyArguments": false,
            "argvContainsSecrets": false,
            "hostLaunch": "observed",
            "bootstrapDelivery": "not_applicable",
            "runtimeReceipts": "not_applicable",
            "verifiedAdapter": null,
            "productionPackageVerified": false,
        },
    });
    let report_path = run_dir.join("runtime-evidence.json");
    fs::write(
        &report_path,
        serde_json::to_vec_pretty(&report).expect("serialize M3-WI runtime evidence"),
    )
    .expect("write M3-WI runtime evidence");
    println!("m3-wi-run-id={run_id}");
    println!("m3-wi-runtime-evidence={}", report_path.display());
}
