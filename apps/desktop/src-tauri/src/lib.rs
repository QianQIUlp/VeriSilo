use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use chrono::Utc;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime, State, WindowEvent,
};
use uuid::Uuid;
use verisilo_remote_backend::transport::PinnedHttpsTransport;
use verisilo_remote_backend::{
    agent::{AutomationScope as RemoteAutomationScope, InputEvent as RemoteInputEvent},
    AgentInteractionReceipt as RemoteInteractionReceipt,
    CapabilityAvailability as RemoteCapabilityAvailability, InteractivePrincipal,
    MemoryBindingStore, OperationResult as RemoteOperationResult, PairingApproval,
    RemoteCapability, RemoteEndpoint, RemoteEnvironmentBackend, RemoteNetworkPolicy,
    RemoteOperation, RemoteOrphanReceipt, SiloBinding as RemoteSiloBinding, SystemClock,
    PROTOCOL_VERSION as REMOTE_PROTOCOL_VERSION,
};

pub mod domain;
pub mod engine;
pub mod environment;
pub mod launcher;
pub mod mihomo;
pub mod native_host;
pub mod proxy_relay;
mod runtime_watchdog;
pub mod vault;

const TRAY_OPEN_ID: &str = "tray-open";
const TRAY_EXIT_ID: &str = "tray-exit";
const ENVIRONMENT_RUNTIME_RECORD_FILE: &str = "environment-runtime.json";
const ENVIRONMENT_RUNTIME_RECORD_SCHEMA_VERSION: u32 = 1;

fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn exit_from_tray<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let local_reservation = match state.local_control.reserve() {
        Ok(reservation) => reservation,
        Err(_) => {
            show_main_window(app);
            return;
        }
    };
    let mut runtime = match state.runtime.lock() {
        Ok(runtime) => runtime,
        Err(_) => {
            show_main_window(app);
            return;
        }
    };
    let can_exit = match runtime.active_managed_camoufox_silo_id() {
        Some(silo_id) => runtime.stop_managed_camoufox(silo_id).is_ok(),
        None => true,
    };
    drop(runtime);
    drop(local_reservation);
    if can_exit {
        let _ = native_host::clear_runtime_status_snapshot(&state.root);
        app.exit(0);
    } else {
        show_main_window(app);
    }
}

fn is_tray_primary_activation(button: MouseButton, button_state: MouseButtonState) -> bool {
    button == MouseButton::Left && button_state == MouseButtonState::Up
}

use domain::{
    app_data_root, discover_browsers as discover_installed_browsers, BrowserCandidate,
    BrowserVerification, CreateManagedSiloInput, CreateSiloInput, ManagedIdentityPreset,
    NetworkProfile, ProxyCredentialsInput, ProxyScheme as SiloProxyScheme, RuntimeActivation,
    RuntimeState, Silo, SiloExecutionTarget, SiloStorageUsage, UpdateSiloEngineInput,
    UpdateSiloInput, UpdateSiloNetworkInput, VaultLockState, VaultStatus,
};
use engine::{
    EngineAdapter, EngineAdapterId, EngineCapabilityId, EngineDescriptor, EngineHealth,
    EngineMaintenanceReceipt, EngineNegotiation, EnginePackageRequest,
    ExternalPackageEngineAdapter, SiloEngineConfig, StockChromiumAdapter,
    VaultSeedIdentityTokenDeriver,
};
use environment::backend::{
    EnvironmentActionReceipt, EnvironmentBackendId, EnvironmentBackendStatus,
    EnvironmentNetworkProfile, EnvironmentOperation, OperationAvailability,
    ProxyScheme as EnvironmentProxyScheme,
};
use environment::{EnvironmentManager, EnvironmentOperationRequest, WslStatus};
use launcher::{
    managed_profiles_are_quiescent_for_vault_restore, profile_in_use, LauncherError, RuntimeManager,
};
use mihomo::{MihomoControllerInput, MihomoSnapshot};
use proxy_relay::ProxyRelay;
use runtime_watchdog::RuntimeWatchdog;
use vault::{
    ProxyAuthentication, RemoteVaultState, StoredIdentityArtifact, VaultBackupReceipt, VaultError,
    VaultRuntime,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LegacyEnvironmentArtifact {
    silo_id: Uuid,
    backend: EnvironmentBackendId,
    cleanup_available: bool,
    message: String,
}

#[derive(Default)]
struct LocalEnvironmentControl {
    reservation: Mutex<()>,
}

impl LocalEnvironmentControl {
    fn reserve(&self) -> Result<MutexGuard<'_, ()>, String> {
        self.reservation
            .lock()
            .map_err(|_| "VeriSilo local environment reservation is unavailable.".to_owned())
    }
}

struct EnvironmentRuntimeState {
    activation: RuntimeActivation,
    wsl_distribution: Option<String>,
    reconciled: bool,
    recovery_blocked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EnvironmentRuntimeRecord {
    schema_version: u32,
    silo_id: Uuid,
    distribution: String,
}

fn environment_runtime_record_is_valid(record: &EnvironmentRuntimeRecord) -> bool {
    record.schema_version == ENVIRONMENT_RUNTIME_RECORD_SCHEMA_VERSION
        && record.silo_id != Uuid::nil()
        && !record.distribution.trim().is_empty()
        && record.distribution.len() <= 128
        && !record.distribution.chars().any(char::is_control)
}

impl Default for EnvironmentRuntimeState {
    fn default() -> Self {
        Self {
            activation: RuntimeActivation::idle(),
            wsl_distribution: None,
            reconciled: false,
            recovery_blocked: false,
        }
    }
}

impl EnvironmentRuntimeState {
    fn load(root: &std::path::Path) -> Self {
        let path = root.join(ENVIRONMENT_RUNTIME_RECORD_FILE);
        match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<EnvironmentRuntimeRecord>(&bytes) {
                Ok(record) if environment_runtime_record_is_valid(&record) => {
                    Self {
                        activation: RuntimeActivation {
                            active_silo_id: Some(record.silo_id),
                            state: RuntimeState::RecoveryRequired,
                            updated_at: Utc::now(),
                            message: Some(
                                "VeriSilo found an interrupted Linux browser session and will verify it after the Vault is unlocked."
                                    .to_owned(),
                            ),
                            browser_verification: None,
                            engine_evidence: None,
                            network_evidence: None,
                        },
                        wsl_distribution: Some(record.distribution),
                        reconciled: false,
                        recovery_blocked: true,
                    }
                }
                _ => Self {
                    activation: RuntimeActivation {
                        active_silo_id: None,
                        state: RuntimeState::RecoveryRequired,
                        updated_at: Utc::now(),
                        message: Some(
                            "The saved Linux runtime record is invalid. New launches are blocked until recovery is completed."
                                .to_owned(),
                        ),
                        browser_verification: None,
                        engine_evidence: None,
                        network_evidence: None,
                    },
                    wsl_distribution: None,
                    reconciled: false,
                    recovery_blocked: true,
                },
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(_) => Self {
                activation: RuntimeActivation {
                    active_silo_id: None,
                    state: RuntimeState::RecoveryRequired,
                    updated_at: Utc::now(),
                    message: Some(
                        "The saved Linux runtime record cannot be read. New launches are blocked."
                            .to_owned(),
                    ),
                    browser_verification: None,
                    engine_evidence: None,
                    network_evidence: None,
                },
                wsl_distribution: None,
                reconciled: false,
                recovery_blocked: true,
            },
        }
    }

    fn is_active(&self, silo_id: Uuid) -> bool {
        self.activation.active_silo_id == Some(silo_id)
            && matches!(
                self.activation.state,
                RuntimeState::Preflight
                    | RuntimeState::Launching
                    | RuntimeState::Running
                    | RuntimeState::RecoveryRequired
            )
    }

    fn has_active_silo(&self) -> bool {
        self.recovery_blocked
            || (self.activation.active_silo_id.is_some()
                && matches!(
                    self.activation.state,
                    RuntimeState::Preflight
                        | RuntimeState::Launching
                        | RuntimeState::Running
                        | RuntimeState::RecoveryRequired
                ))
    }
}

fn persist_environment_runtime_record(
    root: &std::path::Path,
    record: &EnvironmentRuntimeRecord,
) -> Result<(), String> {
    let path = root.join(ENVIRONMENT_RUNTIME_RECORD_FILE);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err("The Linux runtime record path is not a regular file.".to_owned())
        }
        Ok(_) => {
            return Err(
                "A Linux runtime record already exists and must be reconciled before launch."
                    .to_owned(),
            )
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Could not inspect the Linux runtime record path: {error}"
            ))
        }
    }
    let bytes = serde_json::to_vec(record)
        .map_err(|error| format!("Could not serialize the Linux runtime record: {error}"))?;
    let temporary_path = root.join(format!(".environment-runtime-{}.tmp", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)
        .map_err(|error| format!("Could not reserve the Linux runtime record: {error}"))?;
    let persist_result = file
        .write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("Could not persist the Linux runtime record: {error}"));
    if persist_result.is_err() {
        drop(file);
        let _ = fs::remove_file(&temporary_path);
        return persist_result;
    }
    drop(file);
    let link_result = fs::hard_link(&temporary_path, &path)
        .and_then(|_| OpenOptions::new().read(true).open(&path)?.sync_all())
        .map_err(|error| format!("Could not commit the Linux runtime record: {error}"));
    let _ = fs::remove_file(&temporary_path);
    link_result
}

fn environment_runtime_record_exists(root: &std::path::Path) -> Result<bool, String> {
    let path = root.join(ENVIRONMENT_RUNTIME_RECORD_FILE);
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "Could not inspect the Linux runtime record path: {error}"
        )),
    }
}

fn clear_environment_runtime_record(
    root: &std::path::Path,
    expected: &EnvironmentRuntimeRecord,
) -> Result<(), String> {
    let path = root.join(ENVIRONMENT_RUNTIME_RECORD_FILE);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("Could not read the Linux runtime record: {error}")),
    };
    let actual: EnvironmentRuntimeRecord = serde_json::from_slice(&bytes)
        .map_err(|_| "The Linux runtime record is invalid; refusing to clear it.".to_owned())?;
    if actual != *expected {
        return Err(
            "The Linux runtime record belongs to a different Silo; refusing to clear it."
                .to_owned(),
        );
    }
    fs::remove_file(path)
        .map_err(|error| format!("Could not clear the Linux runtime record: {error}"))
}

fn quarantine_invalid_environment_runtime_record(root: &std::path::Path) -> Result<(), String> {
    let path = root.join(ENVIRONMENT_RUNTIME_RECORD_FILE);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("Could not inspect the invalid Linux runtime record: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(
            "The invalid Linux runtime record path is not a regular file; automatic recovery is unsafe."
                .to_owned(),
        );
    }
    let bytes = fs::read(&path)
        .map_err(|error| format!("Could not read the invalid Linux runtime record: {error}"))?;
    if serde_json::from_slice::<EnvironmentRuntimeRecord>(&bytes)
        .is_ok_and(|record| environment_runtime_record_is_valid(&record))
    {
        return Err(
            "The Linux runtime record became valid during recovery; refusing to quarantine it."
                .to_owned(),
        );
    }
    let quarantine = root.join(format!(
        "environment-runtime.invalid-{}.json",
        Uuid::new_v4()
    ));
    fs::rename(&path, quarantine)
        .map_err(|error| format!("Could not quarantine the invalid Linux runtime record: {error}"))
}

fn recover_invalid_environment_runtime_record(
    state: &AppState,
    silos: &[Silo],
) -> Result<(), String> {
    for silo in silos {
        let SiloExecutionTarget::Wsl { distribution } = &silo.execution_target else {
            continue;
        };
        let artifacts = environment::backend::local_environment_artifacts(
            &state.root.join("environments"),
            silo.id,
        )
        .map_err(|error| {
            format!(
                "Could not verify Linux environment ownership for Silo {}: {error}",
                silo.id
            )
        })?;
        if !artifacts.contains(&EnvironmentBackendId::WslChromium) {
            // A Silo that was never configured, or was already detached, has
            // no host binding and therefore cannot own a guest process that
            // VeriSilo started. Do not let it block recovery of later bound
            // Silos. Any partial WSL artifact still reaches exact Stop below,
            // whose binding check fails closed.
            continue;
        }
        let mut environments = state
            .environments
            .lock()
            .map_err(|_| "VeriSilo environment provider state is unavailable.".to_owned())?;
        let status = environments
            .select_wsl_distribution(distribution.clone())
            .map_err(|error| error.to_string())?;
        if !environment_operation_available(&status, EnvironmentOperation::Stop) {
            return Err(format!(
                "The Linux environment {distribution} is not ready to stop an interrupted Silo."
            ));
        }
        environments
            .execute(EnvironmentOperationRequest::Stop {
                backend: EnvironmentBackendId::WslChromium,
                environment_id: silo.id,
            })
            .map_err(|error| {
                format!(
                    "Could not stop Silo {} in Linux environment {distribution}: {error}",
                    silo.id
                )
            })?;
    }
    quarantine_invalid_environment_runtime_record(&state.root)
}

pub struct AppState {
    root: PathBuf,
    resource_root: PathBuf,
    // Local stock/provider lifecycle operations first reserve local_control,
    // then acquire Vault → Runtime → Environments when each is needed. A
    // command may drop an inner guard but must never reacquire an earlier one.
    // The reservation stays held across slow provider/launcher completion.
    local_control: LocalEnvironmentControl,
    vault: Mutex<VaultRuntime>,
    runtime: Arc<Mutex<RuntimeManager>>,
    runtime_watchdog: RuntimeWatchdog,
    environments: Mutex<EnvironmentManager>,
    environment_runtime: Mutex<EnvironmentRuntimeState>,
    // Remote lifecycle exchanges are serialized. Commands hold this guard
    // before Vault so a user lock/revocation linearizes after any in-flight
    // request and no later request can reuse the dropped credential.
    remote_control: Mutex<()>,
}

impl AppState {
    fn new(resource_root: PathBuf) -> Self {
        let root =
            app_data_root().expect("VeriSilo needs a writable local application data directory");
        let runtime = Arc::new(Mutex::new(RuntimeManager::open(&root)));
        let runtime_watchdog =
            RuntimeWatchdog::start(&runtime).expect("VeriSilo needs a native runtime watchdog");
        let environments = EnvironmentManager::new(root.clone(), resource_root.clone())
            .expect("VeriSilo needs valid fixed environment provider roots");
        let environment_runtime = EnvironmentRuntimeState::load(&root);
        Self {
            root,
            resource_root,
            local_control: LocalEnvironmentControl::default(),
            vault: Mutex::new(VaultRuntime::default()),
            runtime,
            runtime_watchdog,
            environments: Mutex::new(environments),
            environment_runtime: Mutex::new(environment_runtime),
            remote_control: Mutex::new(()),
        }
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        self.runtime_watchdog.shutdown();
        let _ = native_host::clear_runtime_status_snapshot(&self.root);
    }
}

fn publish_runtime_status(state: &AppState, activation: &RuntimeActivation, vault: &VaultStatus) {
    let _ = native_host::write_runtime_status_snapshot(&state.root, activation, vault);
}

fn reconcile_runtime_if_possible(
    vault: &mut VaultRuntime,
    runtime: &mut RuntimeManager,
) -> RuntimeActivation {
    if runtime.needs_reconciliation() {
        if let Some(silo_id) = runtime.recorded_silo_id() {
            if let Ok(silo) = vault.get_silo(silo_id) {
                let mihomo_authentication = vault
                    .mihomo_controller_authentication_for_silo(silo_id)
                    .ok()
                    .flatten();
                return runtime.reconcile_persisted(&silo, mihomo_authentication);
            }
        }
    }
    runtime.activation()
}

fn environment_operation_available(
    status: &EnvironmentBackendStatus,
    operation: EnvironmentOperation,
) -> bool {
    status.capabilities.iter().any(|capability| {
        capability.operation == operation
            && matches!(&capability.availability, OperationAvailability::Available)
    })
}

fn prepare_wsl_distribution(
    state: &AppState,
    distribution: &str,
    required_operations: &[EnvironmentOperation],
) -> Result<EnvironmentBackendStatus, String> {
    {
        let environment_runtime = state
            .environment_runtime
            .lock()
            .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())?;
        if environment_runtime.has_active_silo()
            && environment_runtime.wsl_distribution.as_deref() != Some(distribution)
        {
            return Err(
                "Stop or recover the active Linux Silo before selecting a different distribution."
                    .to_owned(),
            );
        }
    }
    let mut environments = state
        .environments
        .lock()
        .map_err(|_| "VeriSilo environment provider state is unavailable.".to_owned())?;
    let status = environments
        .select_wsl_distribution(distribution.to_owned())
        .map_err(|error| error.to_string())?;
    for &operation in required_operations {
        if !environment_operation_available(&status, operation) {
            return Err(format!(
                "The selected Linux environment is not ready for {operation:?}. Complete its setup in Runtime components first."
            ));
        }
    }
    Ok(status)
}

fn wsl_network_profile(silo: &Silo) -> Result<EnvironmentNetworkProfile, String> {
    match &silo.network_profile {
        NetworkProfile::Direct { proxy_required } if !*proxy_required => {
            Ok(EnvironmentNetworkProfile::Direct)
        }
        NetworkProfile::FixedProxy {
            proxy_required,
            scheme: SiloProxyScheme::Socks5,
            host,
            port,
            bypass_list,
            credential_reference,
            external_mihomo,
            ..
        } if host == "127.0.0.1"
            && bypass_list.is_empty()
            && credential_reference.is_none()
            && external_mihomo.is_none() =>
        {
            Ok(EnvironmentNetworkProfile::FixedProxy {
                proxy_required: *proxy_required,
                scheme: EnvironmentProxyScheme::Socks5,
                host: host.clone(),
                port: *port,
            })
        }
        _ => Err(
            "This Linux environment currently accepts direct access or a credential-free SOCKS5 proxy running inside that Linux environment. Change the Silo network before launching; VeriSilo will not fall back to the Windows connection."
                .to_owned(),
        ),
    }
}

fn environment_runtime_is_active(state: &AppState, silo_id: Uuid) -> Result<bool, String> {
    state
        .environment_runtime
        .lock()
        .map(|runtime| runtime.is_active(silo_id))
        .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())
}

fn environment_runtime_has_active_silo(state: &AppState) -> Result<bool, String> {
    state
        .environment_runtime
        .lock()
        .map(|runtime| runtime.has_active_silo())
        .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())
}

fn effective_runtime_activation(
    state: &AppState,
    vault_status: &VaultStatus,
    local_activation: RuntimeActivation,
) -> Result<RuntimeActivation, String> {
    if !matches!(vault_status.state, VaultLockState::Unlocked) {
        return Ok(local_activation);
    }
    let environment = state
        .environment_runtime
        .lock()
        .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())?;
    if environment.has_active_silo() {
        Ok(environment.activation.clone())
    } else {
        Ok(local_activation)
    }
}

fn reconcile_environment_runtime_if_needed(state: &AppState, silos: &[Silo]) -> Result<(), String> {
    let (needs_reconciliation, recovery_blocked, silo_id, distribution) = {
        let runtime = state
            .environment_runtime
            .lock()
            .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())?;
        (
            !runtime.reconciled || runtime.has_active_silo(),
            runtime.recovery_blocked,
            runtime.activation.active_silo_id,
            runtime.wsl_distribution.clone(),
        )
    };
    if !needs_reconciliation {
        return Ok(());
    }
    let (Some(silo_id), Some(distribution)) = (silo_id, distribution) else {
        if recovery_blocked {
            let recovery = recover_invalid_environment_runtime_record(state, silos);
            let mut runtime = state
                .environment_runtime
                .lock()
                .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())?;
            runtime.reconciled = true;
            match recovery {
                Ok(()) => {
                    runtime.activation = RuntimeActivation {
                        active_silo_id: None,
                        state: RuntimeState::Stopped,
                        updated_at: Utc::now(),
                        message: Some(
                            "Stopped every known Linux Silo and quarantined an invalid recovery record."
                                .to_owned(),
                        ),
                        browser_verification: None,
                        engine_evidence: None,
                        network_evidence: None,
                    };
                    runtime.wsl_distribution = None;
                    runtime.recovery_blocked = false;
                }
                Err(error) => {
                    runtime.activation.state = RuntimeState::RecoveryRequired;
                    runtime.activation.updated_at = Utc::now();
                    runtime.activation.message = Some(format!(
                        "The invalid Linux runtime record could not be recovered safely: {error}"
                    ));
                    runtime.recovery_blocked = true;
                }
            }
            return Ok(());
        }
        let mut runtime = state
            .environment_runtime
            .lock()
            .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())?;
        runtime.reconciled = true;
        if !recovery_blocked {
            runtime.activation = RuntimeActivation::idle();
        }
        return Ok(());
    };
    let Some(silo) = silos.iter().find(|silo| silo.id == silo_id) else {
        let mut runtime = state
            .environment_runtime
            .lock()
            .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())?;
        runtime.reconciled = true;
        runtime.recovery_blocked = true;
        runtime.activation.state = RuntimeState::RecoveryRequired;
        runtime.activation.message = Some(
            "The interrupted Linux runtime record no longer matches an active Silo. New launches remain blocked."
                .to_owned(),
        );
        return Ok(());
    };
    if !matches!(
        &silo.execution_target,
        SiloExecutionTarget::Wsl {
            distribution: saved
        } if saved == &distribution
    ) {
        let mut runtime = state
            .environment_runtime
            .lock()
            .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())?;
        runtime.reconciled = true;
        runtime.recovery_blocked = true;
        runtime.activation.state = RuntimeState::RecoveryRequired;
        runtime.activation.message = Some(
            "The interrupted Linux runtime does not match the Silo's saved run location. New launches remain blocked."
                .to_owned(),
        );
        return Ok(());
    }

    let health_result = (|| -> Result<(), String> {
        prepare_wsl_distribution(state, &distribution, &[EnvironmentOperation::Health])?;
        let mut environments = state
            .environments
            .lock()
            .map_err(|_| "VeriSilo environment provider state is unavailable.".to_owned())?;
        environments
            .execute(EnvironmentOperationRequest::Health {
                backend: EnvironmentBackendId::WslChromium,
                environment_id: silo_id,
            })
            .map(|_| ())
            .map_err(|error| error.to_string())
    })();

    if health_result.is_ok() {
        if silo.identity_locked_at.is_none() {
            let mut vault = state
                .vault
                .lock()
                .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
            if let Err(error) = vault.mark_silo_identity_locked(&state.root, silo_id) {
                let mut runtime = state
                    .environment_runtime
                    .lock()
                    .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())?;
                runtime.reconciled = true;
                runtime.recovery_blocked = true;
                runtime.activation.state = RuntimeState::RecoveryRequired;
                runtime.activation.message = Some(format!(
                    "The Linux browser is running, but the Silo identity lock could not be persisted: {error}"
                ));
                return Ok(());
            }
        }
        let mut runtime = state
            .environment_runtime
            .lock()
            .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())?;
        runtime.activation = RuntimeActivation {
            active_silo_id: Some(silo_id),
            state: RuntimeState::Running,
            updated_at: Utc::now(),
            message: Some(format!(
                "Recovered the isolated Linux browser in {distribution}."
            )),
            browser_verification: None,
            engine_evidence: None,
            network_evidence: None,
        };
        runtime.wsl_distribution = Some(distribution);
        runtime.reconciled = true;
        runtime.recovery_blocked = false;
        return Ok(());
    }

    let stop_result = (|| -> Result<(), String> {
        prepare_wsl_distribution(state, &distribution, &[EnvironmentOperation::Stop])?;
        let mut environments = state
            .environments
            .lock()
            .map_err(|_| "VeriSilo environment provider state is unavailable.".to_owned())?;
        environments
            .execute(EnvironmentOperationRequest::Stop {
                backend: EnvironmentBackendId::WslChromium,
                environment_id: silo_id,
            })
            .map(|_| ())
            .map_err(|error| error.to_string())
    })();
    let record = EnvironmentRuntimeRecord {
        schema_version: ENVIRONMENT_RUNTIME_RECORD_SCHEMA_VERSION,
        silo_id,
        distribution: distribution.clone(),
    };
    if stop_result.is_ok() && clear_environment_runtime_record(&state.root, &record).is_ok() {
        let mut runtime = state
            .environment_runtime
            .lock()
            .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())?;
        runtime.activation = RuntimeActivation {
            active_silo_id: None,
            state: RuntimeState::Stopped,
            updated_at: Utc::now(),
            message: Some(
                "Cleared an interrupted Linux runtime that was no longer running.".to_owned(),
            ),
            browser_verification: None,
            engine_evidence: None,
            network_evidence: None,
        };
        runtime.wsl_distribution = None;
        runtime.reconciled = true;
        runtime.recovery_blocked = false;
    } else {
        let mut runtime = state
            .environment_runtime
            .lock()
            .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())?;
        runtime.reconciled = true;
        runtime.recovery_blocked = true;
        runtime.activation.state = RuntimeState::RecoveryRequired;
        runtime.activation.message = Some(format!(
            "The interrupted Linux browser could not be verified or stopped safely: {}",
            health_result.expect_err("failed health result")
        ));
    }
    Ok(())
}

fn stop_environment_runtime_for_vault_lock(state: &AppState) {
    let (silo_id, distribution) = match state.environment_runtime.lock() {
        Ok(runtime) if runtime.has_active_silo() => (
            runtime.activation.active_silo_id,
            runtime.wsl_distribution.clone(),
        ),
        _ => return,
    };
    let (Some(silo_id), Some(distribution)) = (silo_id, distribution) else {
        return;
    };
    let stop_result = (|| -> Result<(), String> {
        prepare_wsl_distribution(state, &distribution, &[EnvironmentOperation::Stop])?;
        let mut environments = state
            .environments
            .lock()
            .map_err(|_| "VeriSilo environment provider state is unavailable.".to_owned())?;
        environments
            .execute(EnvironmentOperationRequest::Stop {
                backend: EnvironmentBackendId::WslChromium,
                environment_id: silo_id,
            })
            .map(|_| ())
            .map_err(|error| error.to_string())
    })();
    let record = EnvironmentRuntimeRecord {
        schema_version: ENVIRONMENT_RUNTIME_RECORD_SCHEMA_VERSION,
        silo_id,
        distribution,
    };
    let mut runtime = match state.environment_runtime.lock() {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    if stop_result.is_ok() && clear_environment_runtime_record(&state.root, &record).is_ok() {
        runtime.activation = RuntimeActivation {
            active_silo_id: None,
            state: RuntimeState::Stopped,
            updated_at: Utc::now(),
            message: Some("Stopped the Linux browser before locking the Vault.".to_owned()),
            browser_verification: None,
            engine_evidence: None,
            network_evidence: None,
        };
        runtime.wsl_distribution = None;
        runtime.recovery_blocked = false;
    } else {
        runtime.activation.state = RuntimeState::RecoveryRequired;
        runtime.activation.updated_at = Utc::now();
        runtime.activation.message = Some(
            "The Vault was locked, but the Linux browser could not be confirmed stopped. New launches remain blocked after unlock."
                .to_owned(),
        );
        runtime.recovery_blocked = true;
    }
    runtime.reconciled = false;
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopStatus {
    vault: VaultStatus,
    activation: RuntimeActivation,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineAdapterStatus {
    descriptor: EngineDescriptor,
    negotiation: EngineNegotiation,
    health: EngineHealth,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteEnvironmentStatus {
    protocol_version: u16,
    state: &'static str,
    transport_available: bool,
    durable_binding_store_available: bool,
    self_hosted_agent_available: bool,
    capabilities: Vec<RemoteCapability>,
    message: String,
    endpoint: Option<RemoteEndpoint>,
    pairing: Option<RemotePairingStatus>,
    bindings: Vec<RemoteSiloBinding>,
    last_results: Vec<RemoteOperationResult>,
    pairing_revoked_at: Option<chrono::DateTime<Utc>>,
    orphan_receipts: Vec<RemoteOrphanReceipt>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemotePairingStatus {
    server_id: Uuid,
    client_credential_id: Uuid,
    node: verisilo_remote_backend::agent::NodeDisclosure,
    credential_expires_at_unix_ms: u64,
    expired: bool,
}

fn summarize_engine(adapter: &dyn EngineAdapter) -> EngineAdapterStatus {
    EngineAdapterStatus {
        descriptor: adapter.descriptor(),
        negotiation: adapter.negotiate(&EngineCapabilityId::ALL),
        health: adapter.health(),
    }
}

#[tauri::command]
fn list_engine_adapters(state: State<'_, AppState>) -> Result<Vec<EngineAdapterStatus>, String> {
    let mut statuses = discover_installed_browsers()
        .into_iter()
        .map(|candidate| {
            let adapter = StockChromiumAdapter::new(domain::BrowserDescriptor {
                kind: candidate.kind,
                executable_path: candidate.executable_path,
                version: candidate.version,
            });
            summarize_engine(&adapter)
        })
        .collect::<Vec<_>>();

    for id in [
        engine::EngineAdapterId::ControlledChromium,
        engine::EngineAdapterId::Camoufox,
    ] {
        let mut adapter = ExternalPackageEngineAdapter::production_prototype(id)
            .map_err(|error| error.to_string())?;
        let mut status = summarize_engine(&adapter);
        if id == EngineAdapterId::Camoufox {
            if let Err(error) =
                adapter.ensure_builtin_package(&managed_browser_package_root(&state))
            {
                status.health.state = engine::EngineHealthState::Unavailable;
                status.health.message = format!(
                    "The bundled Camoufox RC1 package is unavailable or failed verification: {error}"
                );
                status.health.checked_at = Utc::now();
            } else {
                status = summarize_engine(&adapter);
            }
        }
        statuses.push(status);
    }
    Ok(statuses)
}

fn production_external_engine(
    adapter_id: EngineAdapterId,
) -> Result<ExternalPackageEngineAdapter, String> {
    ExternalPackageEngineAdapter::production_prototype(adapter_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn install_engine_package(
    state: State<'_, AppState>,
    adapter_id: EngineAdapterId,
    request: EnginePackageRequest,
) -> Result<EngineMaintenanceReceipt, String> {
    if adapter_id == EngineAdapterId::Camoufox {
        let mut adapter = production_external_engine(adapter_id)?;
        return adapter
            .ensure_builtin_package(&managed_browser_package_root(&state))
            .map_err(|error| error.to_string());
    }
    production_external_engine(adapter_id)?
        .install(&request)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn update_engine_package(
    state: State<'_, AppState>,
    adapter_id: EngineAdapterId,
    request: EnginePackageRequest,
) -> Result<EngineMaintenanceReceipt, String> {
    if adapter_id == EngineAdapterId::Camoufox {
        let mut adapter = production_external_engine(adapter_id)?;
        return adapter
            .ensure_builtin_package(&managed_browser_package_root(&state))
            .map_err(|error| error.to_string());
    }
    production_external_engine(adapter_id)?
        .update(&request)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn rollback_engine_package(
    adapter_id: EngineAdapterId,
) -> Result<EngineMaintenanceReceipt, String> {
    production_external_engine(adapter_id)?
        .rollback()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_engine_emergency_disabled(
    adapter_id: EngineAdapterId,
    disabled: bool,
    reason: Option<String>,
) -> Result<EngineAdapterStatus, String> {
    let mut adapter = production_external_engine(adapter_id)?;
    adapter
        .set_emergency_disabled(disabled, reason)
        .map_err(|error| error.to_string())?;
    Ok(summarize_engine(&adapter))
}

fn unavailable_remote_capabilities(reason: &str) -> Vec<RemoteCapability> {
    RemoteOperation::ALL
        .into_iter()
        .map(|operation| RemoteCapability {
            operation,
            availability: RemoteCapabilityAvailability::Unavailable {
                reason: reason.to_owned(),
            },
        })
        .collect()
}

fn remote_environment_status_from(
    vault_state: VaultLockState,
    remote: Option<&RemoteVaultState>,
) -> RemoteEnvironmentStatus {
    let now_ms = u64::try_from(Utc::now().timestamp_millis()).unwrap_or(0);
    let Some(remote) = remote else {
        let (state, reason, message) = match vault_state {
            VaultLockState::Unlocked => (
                "not_configured",
                "No self-hosted endpoint is paired.",
                "Enter a user-operated HTTPS origin, verify its pin and explicitly approve pairing. No default endpoint exists.",
            ),
            VaultLockState::Locked => (
                "vault_locked",
                "Unlock the Vault to use encrypted remote credentials.",
                "Remote credentials and bindings are unavailable while the Vault is locked; lifecycle requests are refused.",
            ),
            VaultLockState::Uninitialized => (
                "vault_uninitialized",
                "Initialize and unlock the Vault before pairing.",
                "Remote endpoint and pairing material are stored only inside an initialized encrypted Vault.",
            ),
        };
        return RemoteEnvironmentStatus {
            protocol_version: REMOTE_PROTOCOL_VERSION,
            state,
            transport_available: true,
            durable_binding_store_available: true,
            self_hosted_agent_available: false,
            capabilities: unavailable_remote_capabilities(reason),
            message: message.to_owned(),
            endpoint: None,
            pairing: None,
            bindings: Vec::new(),
            last_results: Vec::new(),
            pairing_revoked_at: None,
            orphan_receipts: Vec::new(),
        };
    };

    let pairing = remote.backend.pairing.as_ref();
    let expired = pairing.is_some_and(|pairing| pairing.credential_expires_at_unix_ms <= now_ms);
    let (state, capabilities, message) = match pairing {
        Some(pairing) if !expired => (
            "paired",
            pairing.capabilities.clone(),
            "A pinned self-hosted endpoint and unexpired application credential are stored. Availability still depends on the user-operated Agent at request time.".to_owned(),
        ),
        Some(_) => (
            "credential_expired",
            unavailable_remote_capabilities(
                "The stored remote credential expired; revoke and explicitly pair again.",
            ),
            "The endpoint and stable bindings remain encrypted, but lifecycle requests are refused because the pairing credential expired.".to_owned(),
        ),
        None if remote.pairing_revoked_at.is_some() => (
            "revoked",
            unavailable_remote_capabilities(
                "The local pairing credential was explicitly revoked.",
            ),
            "The local credential was erased. Existing binding metadata is retained to prevent accidental recreation or endpoint substitution.".to_owned(),
        ),
        None => (
            "not_paired",
            unavailable_remote_capabilities("The configured endpoint is not paired."),
            "The endpoint was entered during an explicit pairing attempt, but no usable credential is stored.".to_owned(),
        ),
    };
    let mut bindings = remote.backend.bindings.clone();
    bindings.sort_by_key(|binding| binding.silo_id);
    let mut last_results = remote.last_results.values().cloned().collect::<Vec<_>>();
    last_results.sort_by_key(|result| result.silo_id);
    let mut orphan_receipts = remote.orphan_receipts.clone();
    orphan_receipts.sort_by_key(|receipt| std::cmp::Reverse(receipt.detached_at_unix_ms));
    RemoteEnvironmentStatus {
        protocol_version: REMOTE_PROTOCOL_VERSION,
        state,
        transport_available: true,
        durable_binding_store_available: true,
        self_hosted_agent_available: pairing.is_some() && !expired,
        capabilities,
        message,
        endpoint: remote.endpoint.clone(),
        pairing: pairing.map(|pairing| RemotePairingStatus {
            server_id: pairing.server_id,
            client_credential_id: pairing.client_credential_id,
            node: pairing.node.clone(),
            credential_expires_at_unix_ms: pairing.credential_expires_at_unix_ms,
            expired,
        }),
        bindings,
        last_results,
        pairing_revoked_at: remote.pairing_revoked_at,
        orphan_receipts,
    }
}

#[tauri::command]
fn remote_environment_status(
    state: State<'_, AppState>,
) -> Result<RemoteEnvironmentStatus, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let vault_status = vault.status(&state.root);
    let remote = if matches!(vault_status.state, VaultLockState::Unlocked) {
        Some(
            vault
                .remote_control_plane()
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    Ok(remote_environment_status_from(
        vault_status.state,
        remote.as_ref(),
    ))
}

#[tauri::command]
fn validate_remote_environment_endpoint(
    endpoint: RemoteEndpoint,
) -> Result<RemoteEndpoint, String> {
    endpoint.validate().map_err(|error| error.to_string())?;
    Ok(endpoint)
}

type ProductionRemoteBackend =
    RemoteEnvironmentBackend<PinnedHttpsTransport, MemoryBindingStore, SystemClock>;

fn production_remote_backend(remote: &RemoteVaultState) -> Result<ProductionRemoteBackend, String> {
    let endpoint = remote
        .endpoint
        .clone()
        .ok_or_else(|| "No self-hosted remote endpoint is configured.".to_owned())?;
    let transport = PinnedHttpsTransport::new().map_err(|error| error.to_string())?;
    RemoteEnvironmentBackend::from_snapshot(
        endpoint,
        transport,
        SystemClock,
        remote.backend.clone(),
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn pair_remote_environment(
    state: State<'_, AppState>,
    endpoint: RemoteEndpoint,
    approval: PairingApproval,
) -> Result<RemoteEnvironmentStatus, String> {
    // Pairing is the first production network action. It is reached only from
    // this explicit command, never from startup, status, endpoint validation or
    // Silo selection.
    let _remote_guard = state
        .remote_control
        .lock()
        .map_err(|_| "VeriSilo remote control state is unavailable.".to_owned())?;
    endpoint.validate().map_err(|error| error.to_string())?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    let mut remote = vault
        .remote_control_plane()
        .map_err(|error| error.to_string())?;
    if remote.backend.pairing.is_some() {
        return Err(
            "A remote credential is already stored. Explicitly revoke it before pairing again."
                .to_owned(),
        );
    }
    if remote
        .backend
        .bindings
        .iter()
        .any(|binding| binding.endpoint != endpoint)
    {
        return Err(
            "Existing Silo bindings belong to a different endpoint. Re-pair that exact endpoint; endpoint substitution is refused."
                .to_owned(),
        );
    }
    remote.endpoint = Some(endpoint);
    let mut backend = production_remote_backend(&remote)?;
    let pairing_result = backend.pair(approval);
    remote.backend = backend.export_snapshot();
    if pairing_result.is_ok() {
        remote.pairing_revoked_at = None;
    }
    // A token is single-use locally even when TLS, pinning or the Agent rejects
    // the attempt. Commit that replay ledger before surfacing the network error.
    vault
        .persist_remote_control_plane(&state.root, remote.clone())
        .map_err(|error| error.to_string())?;
    pairing_result.map_err(|error| error.to_string())?;
    Ok(remote_environment_status_from(
        VaultLockState::Unlocked,
        Some(&remote),
    ))
}

#[tauri::command]
fn rotate_remote_environment_tls_pin(
    state: State<'_, AppState>,
    endpoint: RemoteEndpoint,
    approval: PairingApproval,
    confirm_rotation: bool,
) -> Result<RemoteEnvironmentStatus, String> {
    if !confirm_rotation {
        return Err("TLS pin rotation requires explicit user confirmation.".to_owned());
    }
    let _remote_guard = state
        .remote_control
        .lock()
        .map_err(|_| "VeriSilo remote control state is unavailable.".to_owned())?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    let original = vault
        .remote_control_plane()
        .map_err(|error| error.to_string())?;
    if original.backend.pairing.is_none() {
        return Err(
            "TLS pin rotation requires an existing unrevoked pairing credential.".to_owned(),
        );
    }

    // Phase 1 is intentionally a replay-ledger-only commit. The old endpoint,
    // pairing and every binding remain current while the candidate pin is
    // contacted. This also makes the token single-use after transport errors,
    // wrong-server responses, rejected candidates, or a failed final commit.
    let mut reservation_backend = production_remote_backend(&original)?;
    reservation_backend
        .reserve_pairing_token(&approval)
        .map_err(|error| error.to_string())?;
    let mut ledger_state = original.clone();
    ledger_state.backend.used_pairing_token_ids = reservation_backend.used_pairing_token_ids();
    vault
        .persist_remote_control_plane(&state.root, ledger_state.clone())
        .map_err(|error| error.to_string())?;

    // Build the network attempt from the pre-reservation snapshot so the
    // backend itself consumes exactly the same ID while producing a complete
    // candidate snapshot. Nothing from this candidate is observable until the
    // single final Vault replacement succeeds.
    let mut candidate_backend = production_remote_backend(&original)?;
    let rotation_claim = match candidate_backend.begin_tls_pin_rotation(&endpoint, &approval) {
        Ok(rotation_claim) => rotation_claim,
        Err(error) => {
            // The endpoint, credential identity and bindings are still the
            // old ones, but an authenticated old-pin request may have
            // consumed client and server sequences. Persist those monotonic
            // counters alongside the already-reserved token so the old
            // credential remains usable.
            let mut failed = ledger_state;
            failed.backend = candidate_backend.export_snapshot();
            vault
                .persist_remote_control_plane(&state.root, failed)
                .map_err(|persist_error| persist_error.to_string())?;
            return Err(error.to_string());
        }
    };

    // Durably checkpoint the consumed old-credential sequences before the new
    // pin is contacted. If the new pairing or final commit fails, this remains
    // a complete old-endpoint state with the burned token and usable counters.
    let mut authorized = ledger_state;
    authorized.backend = candidate_backend.export_snapshot();
    vault
        .persist_remote_control_plane(&state.root, authorized.clone())
        .map_err(|error| error.to_string())?;

    if let Err(error) =
        candidate_backend.finish_tls_pin_rotation(endpoint.clone(), &approval, rotation_claim)
    {
        return Err(error.to_string());
    }
    let mut rotated = authorized;
    rotated.endpoint = Some(endpoint);
    rotated.backend = candidate_backend.export_snapshot();
    rotated.pairing_revoked_at = None;
    vault
        .persist_remote_control_plane(&state.root, rotated.clone())
        .map_err(|error| error.to_string())?;

    Ok(remote_environment_status_from(
        VaultLockState::Unlocked,
        Some(&rotated),
    ))
}

#[tauri::command]
fn revoke_remote_pairing(
    state: State<'_, AppState>,
    confirm_revoke: bool,
) -> Result<RemoteEnvironmentStatus, String> {
    if !confirm_revoke {
        return Err("Remote credential revocation requires explicit confirmation.".to_owned());
    }
    let _remote_guard = state
        .remote_control
        .lock()
        .map_err(|_| "VeriSilo remote control state is unavailable.".to_owned())?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    let mut remote = vault
        .remote_control_plane()
        .map_err(|error| error.to_string())?;
    if remote.backend.pairing.take().is_none() {
        return Err("No remote pairing credential is stored.".to_owned());
    }
    // Authorization IDs are capabilities too. Erase every locally usable
    // interactive grant/channel alongside the bearer credential so a later
    // re-pair cannot accidentally reuse an authorization issued to the old
    // application credential. Stable environment bindings and audit receipts
    // remain for recovery and deletion.
    for binding in &mut remote.backend.bindings {
        binding.human_session = None;
        binding.automation_authorizations.clear();
        binding.last_screen_channel = None;
    }
    remote.pairing_revoked_at = Some(Utc::now());
    vault
        .persist_remote_control_plane(&state.root, remote.clone())
        .map_err(|error| error.to_string())?;
    Ok(remote_environment_status_from(
        VaultLockState::Unlocked,
        Some(&remote),
    ))
}

#[tauri::command]
fn force_detach_remote_environment(
    state: State<'_, AppState>,
    silo_id: Uuid,
    confirm_local_detach: bool,
    acknowledge_remote_orphan_risk: bool,
) -> Result<RemoteEnvironmentStatus, String> {
    if !confirm_local_detach || !acknowledge_remote_orphan_risk {
        return Err(
            "Force detach requires both local removal and continuing remote cost confirmations."
                .to_owned(),
        );
    }
    let _remote_guard = state
        .remote_control
        .lock()
        .map_err(|_| "VeriSilo remote control state is unavailable.".to_owned())?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    vault.get_silo(silo_id).map_err(|error| error.to_string())?;
    let mut remote = vault
        .remote_control_plane()
        .map_err(|error| error.to_string())?;
    let mut backend = production_remote_backend(&remote)?;
    let receipt = backend
        .force_detach_binding(
            silo_id,
            confirm_local_detach,
            acknowledge_remote_orphan_risk,
        )
        .map_err(|error| error.to_string())?;
    remote.backend = backend.export_snapshot();
    remote.last_results.remove(&silo_id);
    remote.orphan_receipts.push(receipt);
    vault
        .persist_remote_control_plane(&state.root, remote.clone())
        .map_err(|error| error.to_string())?;

    Ok(remote_environment_status_from(
        VaultLockState::Unlocked,
        Some(&remote),
    ))
}

fn execute_remote_operation(
    state: &AppState,
    silo_id: Uuid,
    operation: impl FnOnce(
        &mut ProductionRemoteBackend,
    )
        -> Result<RemoteOperationResult, verisilo_remote_backend::RemoteBackendError>,
) -> Result<RemoteOperationResult, String> {
    let _remote_guard = state
        .remote_control
        .lock()
        .map_err(|_| "VeriSilo remote control state is unavailable.".to_owned())?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    // This both validates the Silo identity and refuses every request while the
    // Vault is locked. The Vault guard remains held through the bounded HTTPS
    // exchange, so a concurrent lock linearizes strictly after this request.
    vault.get_silo(silo_id).map_err(|error| error.to_string())?;
    let mut remote = vault
        .remote_control_plane()
        .map_err(|error| error.to_string())?;
    let mut backend = production_remote_backend(&remote)?;
    let result = operation(&mut backend);
    remote.backend = backend.export_snapshot();
    if let Ok(result) = &result {
        remote.last_results.insert(silo_id, result.clone());
    }
    // Persist advancing response sequences and partial binding/evidence state
    // even when a required-proxy check intentionally fails closed.
    vault
        .persist_remote_control_plane(&state.root, remote)
        .map_err(|error| error.to_string())?;
    result.map_err(|error| error.to_string())
}

fn reject_unavailable_remote_runtime<T>(state: &AppState, silo_id: Uuid) -> Result<T, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    let silo = vault.get_silo(silo_id).map_err(|error| error.to_string())?;
    let location = match silo.execution_target {
        SiloExecutionTarget::Local => "this Windows computer".to_owned(),
        SiloExecutionTarget::Wsl { distribution } => {
            format!("the Linux environment {distribution}")
        }
        SiloExecutionTarget::Remote { endpoint_origin } => endpoint_origin,
    };
    Err(format!(
        "This Silo is bound to {location}. Remote activation and interaction are unavailable until a remote-bound browser identity, global lifecycle lock, and durable identity journal are verified end to end."
    ))
}

#[tauri::command]
fn remote_environment_create(
    state: State<'_, AppState>,
    silo_id: Uuid,
    network: RemoteNetworkPolicy,
    ttl_seconds: u64,
    cost_acknowledged: bool,
) -> Result<RemoteOperationResult, String> {
    let _ = (network, ttl_seconds, cost_acknowledged);
    reject_unavailable_remote_runtime(&state, silo_id)
}

#[tauri::command]
fn remote_environment_start(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    reject_unavailable_remote_runtime(&state, silo_id)
}

#[tauri::command]
fn remote_environment_stop(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    execute_remote_operation(&state, silo_id, |backend| backend.stop(silo_id))
}

#[tauri::command]
fn remote_environment_pause(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    reject_unavailable_remote_runtime(&state, silo_id)
}

#[tauri::command]
fn remote_environment_snapshot(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    reject_unavailable_remote_runtime(&state, silo_id)
}

#[tauri::command]
fn remote_environment_destroy(
    state: State<'_, AppState>,
    silo_id: Uuid,
    confirm_destroy: bool,
) -> Result<RemoteOperationResult, String> {
    execute_remote_operation(&state, silo_id, |backend| {
        backend.destroy(silo_id, confirm_destroy)
    })
}

#[tauri::command]
fn remote_environment_configure_network(
    state: State<'_, AppState>,
    silo_id: Uuid,
    network: RemoteNetworkPolicy,
) -> Result<RemoteOperationResult, String> {
    let _ = network;
    reject_unavailable_remote_runtime(&state, silo_id)
}

#[tauri::command]
fn remote_environment_health(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    execute_remote_operation(&state, silo_id, |backend| backend.health(silo_id))
}

#[tauri::command]
fn remote_environment_logs(
    state: State<'_, AppState>,
    silo_id: Uuid,
    cursor: Option<Uuid>,
    limit: u16,
) -> Result<RemoteOperationResult, String> {
    execute_remote_operation(&state, silo_id, |backend| {
        backend.logs(silo_id, cursor, limit)
    })
}

#[tauri::command]
fn remote_environment_open_human_session(
    state: State<'_, AppState>,
    silo_id: Uuid,
    lifetime_seconds: u64,
) -> Result<RemoteInteractionReceipt, String> {
    let _ = lifetime_seconds;
    reject_unavailable_remote_runtime(&state, silo_id)
}

#[tauri::command]
fn remote_environment_close_human_session(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RemoteInteractionReceipt, String> {
    reject_unavailable_remote_runtime(&state, silo_id)
}

#[tauri::command]
fn remote_environment_grant_automation(
    state: State<'_, AppState>,
    silo_id: Uuid,
    lifetime_seconds: u64,
    scopes: Vec<RemoteAutomationScope>,
    approved_by_user: bool,
) -> Result<RemoteInteractionReceipt, String> {
    let _ = (lifetime_seconds, scopes, approved_by_user);
    reject_unavailable_remote_runtime(&state, silo_id)
}

#[tauri::command]
fn remote_environment_revoke_automation(
    state: State<'_, AppState>,
    silo_id: Uuid,
    authorization_id: Uuid,
) -> Result<RemoteInteractionReceipt, String> {
    let _ = authorization_id;
    reject_unavailable_remote_runtime(&state, silo_id)
}

#[tauri::command]
fn remote_environment_open_screen(
    state: State<'_, AppState>,
    silo_id: Uuid,
    principal: InteractivePrincipal,
) -> Result<RemoteInteractionReceipt, String> {
    let _ = principal;
    reject_unavailable_remote_runtime(&state, silo_id)
}

#[tauri::command]
fn remote_environment_send_input(
    state: State<'_, AppState>,
    silo_id: Uuid,
    principal: InteractivePrincipal,
    events: Vec<RemoteInputEvent>,
) -> Result<RemoteInteractionReceipt, String> {
    let _ = (principal, events);
    reject_unavailable_remote_runtime(&state, silo_id)
}

#[tauri::command]
fn desktop_status(state: State<'_, AppState>) -> Result<DesktopStatus, String> {
    let _local_reservation = state.local_control.reserve()?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let mut vault_status = vault.status(&state.root);
    let mut activation;
    if matches!(vault_status.state, VaultLockState::Unlocked) {
        activation = reconcile_runtime_if_possible(&mut vault, &mut runtime);
        // Reconciliation reads never renew activity, and may itself observe a
        // deadline crossed after the first status snapshot.
        vault_status = vault.status(&state.root);
        if !matches!(vault_status.state, VaultLockState::Unlocked) {
            activation = runtime.revoke_secrets_for_vault_lock();
        }
    } else {
        activation = runtime.revoke_secrets_for_vault_lock();
    }
    let environment_silos = if matches!(vault_status.state, VaultLockState::Unlocked) {
        vault
            .list_active_silos()
            .map_err(|error| error.to_string())?
    } else {
        Vec::new()
    };
    drop(runtime);
    drop(vault);
    if matches!(vault_status.state, VaultLockState::Unlocked) {
        reconcile_environment_runtime_if_needed(&state, &environment_silos)?;
    } else {
        stop_environment_runtime_for_vault_lock(&state);
        let mut environment_runtime = state
            .environment_runtime
            .lock()
            .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())?;
        environment_runtime.reconciled = false;
    }
    activation = effective_runtime_activation(&state, &vault_status, activation)?;
    publish_runtime_status(&state, &activation, &vault_status);

    // Companion is optional. An unreadable inbox must not prevent the desktop
    // core from reporting status or launching an otherwise valid Silo.
    let inbox = native_host::read_network_evidence_inbox(&state.root).unwrap_or_default();
    if !inbox.is_empty() {
        // Preserve the global Vault → Runtime lock order while atomically
        // importing encrypted history and updating the in-memory activation.
        let mut vault = state
            .vault
            .lock()
            .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
        if vault
            .import_network_evidence(&state.root, inbox.clone())
            .is_ok()
        {
            let mut runtime = state
                .runtime
                .lock()
                .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
            vault_status = vault.status(&state.root);
            if matches!(vault_status.state, VaultLockState::Unlocked) {
                for entry in &inbox {
                    activation = runtime.apply_network_evidence(entry);
                }
            } else {
                activation = runtime.revoke_secrets_for_vault_lock();
            }
            drop(runtime);
            drop(vault);
            // Delete transport files only after the encrypted Vault commit (or
            // a successful duplicate/no-longer-relevant decision).
            let _ = native_host::acknowledge_network_evidence_inbox(&state.root, &inbox);
            publish_runtime_status(&state, &activation, &vault_status);
        } else {
            vault_status = vault.status(&state.root);
            if !matches!(vault_status.state, VaultLockState::Unlocked) {
                let mut runtime = state
                    .runtime
                    .lock()
                    .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
                activation = runtime.revoke_secrets_for_vault_lock();
                drop(runtime);
                drop(vault);
                publish_runtime_status(&state, &activation, &vault_status);
            } else {
                drop(vault);
            }
        }
    }
    if !matches!(vault_status.state, VaultLockState::Unlocked) {
        stop_environment_runtime_for_vault_lock(&state);
        let mut environment_runtime = state
            .environment_runtime
            .lock()
            .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())?;
        environment_runtime.reconciled = false;
    }
    activation = effective_runtime_activation(&state, &vault_status, activation)?;
    publish_runtime_status(&state, &activation, &vault_status);
    Ok(DesktopStatus {
        vault: vault_status,
        activation,
    })
}

#[tauri::command]
fn initialize_vault(state: State<'_, AppState>, passphrase: String) -> Result<VaultStatus, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault
        .initialize(&state.root, &passphrase)
        .map_err(|error| error.to_string())?;
    let vault_status = vault.status(&state.root);
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let activation = reconcile_runtime_if_possible(&mut vault, &mut runtime);
    drop(runtime);
    drop(vault);
    publish_runtime_status(&state, &activation, &vault_status);
    Ok(vault_status)
}

#[tauri::command]
fn unlock_vault(state: State<'_, AppState>, passphrase: String) -> Result<VaultStatus, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault
        .unlock(&state.root, &passphrase)
        .map_err(|error| error.to_string())?;
    let vault_status = vault.status(&state.root);
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let activation = reconcile_runtime_if_possible(&mut vault, &mut runtime);
    drop(runtime);
    drop(vault);
    publish_runtime_status(&state, &activation, &vault_status);
    Ok(vault_status)
}

#[tauri::command]
fn lock_vault(state: State<'_, AppState>) -> Result<VaultStatus, String> {
    let _local_reservation = state.local_control.reserve()?;
    let _remote_guard = state
        .remote_control
        .lock()
        .map_err(|_| "VeriSilo remote control state is unavailable.".to_owned())?;
    stop_environment_runtime_for_vault_lock(&state);
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.lock();
    let vault_status = vault.status(&state.root);
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let activation = runtime.revoke_secrets_for_vault_lock();
    drop(runtime);
    drop(vault);
    publish_runtime_status(&state, &activation, &vault_status);
    Ok(vault_status)
}

#[tauri::command]
fn change_vault_passphrase(
    state: State<'_, AppState>,
    current_passphrase: String,
    new_passphrase: String,
) -> Result<VaultStatus, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault
        .change_passphrase(&state.root, &current_passphrase, &new_passphrase)
        .map_err(|error| error.to_string())?;
    let vault_status = vault.status(&state.root);
    drop(vault);
    let activation = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?
        .activation();
    publish_runtime_status(&state, &activation, &vault_status);
    Ok(vault_status)
}

#[tauri::command]
fn backup_vault(
    state: State<'_, AppState>,
    destination_path: String,
) -> Result<VaultBackupReceipt, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault
        .backup(&state.root, &PathBuf::from(destination_path))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn restore_vault(
    state: State<'_, AppState>,
    source_path: String,
    passphrase: String,
    confirm_overwrite: bool,
) -> Result<VaultStatus, String> {
    // Restore is a global ownership transition. Reserve local and remote
    // lifecycle planes before Vault → Runtime → Environments so all in-flight
    // provider work finishes and no new work can race the replacement.
    let _local_reservation = state.local_control.reserve()?;
    let _remote_guard = state
        .remote_control
        .lock()
        .map_err(|_| "VeriSilo remote control state is unavailable.".to_owned())?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let managed_profile_directories = match vault.status(&state.root).state {
        VaultLockState::Uninitialized => Vec::new(),
        VaultLockState::Unlocked => vault
            .managed_profile_directories()
            .map_err(|error| error.to_string())?,
        VaultLockState::Locked => {
            return Err("Unlock the current Vault before restoring a Vault backup.".to_owned())
        }
    };
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let runtime_preparation = runtime
        .prepare_for_vault_restore()
        .ok_or_else(|| {
            "Resolve or close every running, failed, or recovery-required Silo runtime before restoring a Vault backup."
                .to_owned()
        })?;
    let environments = state
        .environments
        .lock()
        .map_err(|_| "VeriSilo environment provider state is unavailable.".to_owned())?;
    environments
        .ensure_no_local_environment_artifacts_for_restore()
        .map_err(|error| error.to_string())?;
    if !managed_profiles_are_quiescent_for_vault_restore(&managed_profile_directories) {
        return Err(
            "Close every browser using a managed Silo Profile before restoring a Vault backup."
                .to_owned(),
        );
    }
    vault
        .restore(
            &state.root,
            &PathBuf::from(source_path),
            &passphrase,
            confirm_overwrite,
        )
        .map_err(|error| error.to_string())?;
    let vault_status = vault.status(&state.root);
    let activation = runtime.complete_successful_vault_restore(runtime_preparation);
    drop(environments);
    drop(runtime);
    drop(vault);
    publish_runtime_status(&state, &activation, &vault_status);
    Ok(vault_status)
}

#[tauri::command]
fn discover_browsers() -> Vec<BrowserCandidate> {
    discover_installed_browsers()
}

#[tauri::command]
fn detect_wsl() -> WslStatus {
    environment::detect_wsl()
}

#[tauri::command]
fn environment_backend_statuses(
    state: State<'_, AppState>,
) -> Result<Vec<EnvironmentBackendStatus>, String> {
    let environments = state
        .environments
        .lock()
        .map_err(|_| "VeriSilo environment provider state is unavailable.".to_owned())?;
    Ok(environments.statuses())
}

#[tauri::command]
fn select_wsl_environment_distribution(
    state: State<'_, AppState>,
    distribution: String,
) -> Result<EnvironmentBackendStatus, String> {
    let _local_reservation = state.local_control.reserve()?;
    {
        let environment_runtime = state
            .environment_runtime
            .lock()
            .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())?;
        if environment_runtime.has_active_silo()
            && environment_runtime.wsl_distribution.as_deref() != Some(distribution.as_str())
        {
            return Err(
                "Stop the active Linux Silo before checking a different distribution.".to_owned(),
            );
        }
    }
    let mut environments = state
        .environments
        .lock()
        .map_err(|_| "VeriSilo environment provider state is unavailable.".to_owned())?;
    environments
        .select_wsl_distribution(distribution)
        .map_err(|error| error.to_string())
}

fn execute_environment_backend(
    state: &AppState,
    request: EnvironmentOperationRequest,
) -> Result<EnvironmentActionReceipt, String> {
    let environment_id = request.environment_id();
    let _local_reservation = state.local_control.reserve()?;
    // Environment operations are authorized against an unlocked, existing
    // Silo. Keep the reservation across authorization and provider completion;
    // inner Vault/Runtime guards may be dropped before the slow fixed process.
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    let silo = vault
        .get_silo(environment_id)
        .map_err(|error| error.to_string())?;
    let saved_target_distribution = match (&silo.execution_target, request.backend()) {
        (SiloExecutionTarget::Wsl { distribution }, EnvironmentBackendId::WslChromium) => {
            Some(distribution.clone())
        }
        _ => None,
    };
    let legacy_cleanup_provider = if saved_target_distribution.is_none()
        && matches!(
            &request,
            EnvironmentOperationRequest::Destroy {
                confirm_destroy: true,
                ..
            }
        ) {
        environment::backend::local_environment_binding_provider(
            &state.root.join("environments"),
            environment_id,
            request.backend(),
        )
        .map_err(|error| error.to_string())?
    } else {
        None
    };
    let target_matches = saved_target_distribution.is_some() || legacy_cleanup_provider.is_some();
    if !target_matches {
        return Err(
            "This environment is not the Silo's saved run location. Choose the run location when creating the Silo; component tools cannot silently move an identity."
                .to_owned(),
        );
    }
    if matches!(
        request.operation(),
        EnvironmentOperation::Start | EnvironmentOperation::Stop
    ) {
        return Err(
            "Start or stop this Silo from Overview so VeriSilo can keep its identity, network, run location, and recovery record together."
                .to_owned(),
        );
    }
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    if runtime.activation().active_silo_id.is_some() {
        return Err(
            "Close the active stock browser Silo before operating a VM, WSL, or Sandbox backend."
                .to_owned(),
        );
    }
    if environment_runtime_has_active_silo(state)?
        && !matches!(
            request.operation(),
            EnvironmentOperation::Stop | EnvironmentOperation::Health | EnvironmentOperation::Logs
        )
    {
        return Err("Stop the active Silo before changing a runtime component.".to_owned());
    }
    drop(runtime);
    drop(vault);
    let target_distribution = saved_target_distribution.or_else(|| {
        (request.backend() == EnvironmentBackendId::WslChromium)
            .then_some(legacy_cleanup_provider)
            .flatten()
    });
    if let Some(distribution) = target_distribution {
        prepare_wsl_distribution(state, &distribution, &[request.operation()])?;
    }
    let mut environments = state
        .environments
        .lock()
        .map_err(|_| "VeriSilo environment provider state is unavailable.".to_owned())?;
    let receipt = environments
        .execute(request)
        .map_err(|error| error.to_string())?;
    if receipt.operation == EnvironmentOperation::Destroy {
        let mut environment_runtime = state
            .environment_runtime
            .lock()
            .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())?;
        if environment_runtime.activation.active_silo_id == Some(environment_id) {
            environment_runtime.activation = RuntimeActivation {
                active_silo_id: None,
                state: RuntimeState::Stopped,
                updated_at: Utc::now(),
                message: Some("The isolated Linux browser was stopped.".to_owned()),
                browser_verification: None,
                engine_evidence: None,
                network_evidence: None,
            };
            environment_runtime.wsl_distribution = None;
        }
    }
    Ok(receipt)
}

#[tauri::command]
fn environment_backend_execute(
    state: State<'_, AppState>,
    request: EnvironmentOperationRequest,
) -> Result<EnvironmentActionReceipt, String> {
    execute_environment_backend(&state, request)
}

fn legacy_environment_artifacts(
    root: &std::path::Path,
    silos: &[Silo],
) -> Result<Vec<LegacyEnvironmentArtifact>, String> {
    let environment_root = root.join("environments");
    let mut result = Vec::new();
    for silo in silos {
        let artifacts =
            environment::backend::local_environment_artifacts(&environment_root, silo.id)
                .map_err(|error| error.to_string())?;
        for backend in artifacts {
            let provider = environment::backend::local_environment_binding_provider(
                &environment_root,
                silo.id,
                backend,
            );
            let saved_wsl_distribution = match (&silo.execution_target, backend) {
                (SiloExecutionTarget::Wsl { distribution }, EnvironmentBackendId::WslChromium) => {
                    Some(distribution)
                }
                _ => None,
            };
            match (saved_wsl_distribution, provider) {
                (Some(saved), Ok(Some(bound))) if saved == &bound => {
                    // This is the Silo's current, expected Linux owner rather
                    // than an orphan left by a schema without run locations.
                }
                (Some(_), _) => result.push(LegacyEnvironmentArtifact {
                    silo_id: silo.id,
                    backend,
                    cleanup_available: false,
                    message: "The saved Linux run location and its ownership record disagree. VeriSilo will not delete either side automatically."
                        .to_owned(),
                }),
                (None, Ok(Some(_))) => result.push(LegacyEnvironmentArtifact {
                    silo_id: silo.id,
                    backend,
                    cleanup_available: true,
                    message: "An older run environment is still bound to this Silo but is not its current run location."
                        .to_owned(),
                }),
                (None, Ok(None)) => result.push(LegacyEnvironmentArtifact {
                    silo_id: silo.id,
                    backend,
                    cleanup_available: false,
                    message: "An older run environment path exists without a complete ownership record. VeriSilo will not delete it automatically."
                        .to_owned(),
                }),
                (None, Err(_)) => result.push(LegacyEnvironmentArtifact {
                    silo_id: silo.id,
                    backend,
                    cleanup_available: false,
                    message: "An older run environment has incomplete ownership information. VeriSilo will not delete it automatically."
                        .to_owned(),
                }),
            }
        }
    }
    result.sort_by_key(|artifact| {
        (
            artifact.silo_id,
            match artifact.backend {
                EnvironmentBackendId::WslChromium => 0_u8,
                EnvironmentBackendId::WindowsSandbox => 1,
                EnvironmentBackendId::HyperV => 2,
            },
        )
    });
    Ok(result)
}

#[tauri::command]
fn list_legacy_environment_artifacts(
    state: State<'_, AppState>,
) -> Result<Vec<LegacyEnvironmentArtifact>, String> {
    let _local_reservation = state.local_control.reserve()?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let silos = vault.list_silos().map_err(|error| error.to_string())?;
    legacy_environment_artifacts(&state.root, &silos)
}

#[tauri::command]
fn cleanup_legacy_environment_artifact(
    state: State<'_, AppState>,
    silo_id: Uuid,
    backend: EnvironmentBackendId,
    confirm_cleanup: bool,
) -> Result<EnvironmentActionReceipt, String> {
    if !confirm_cleanup {
        return Err("Cleaning an older run environment requires explicit confirmation.".to_owned());
    }
    {
        let mut vault = state
            .vault
            .lock()
            .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
        let silo = vault.get_silo(silo_id).map_err(|error| error.to_string())?;
        if matches!(
            (&silo.execution_target, backend),
            (
                SiloExecutionTarget::Wsl { .. },
                EnvironmentBackendId::WslChromium
            )
        ) {
            return Err(
                "The Linux environment is this Silo's current run location and cannot be removed as an older component."
                    .to_owned(),
            );
        }
    }
    execute_environment_backend(
        &state,
        EnvironmentOperationRequest::Destroy {
            backend,
            environment_id: silo_id,
            confirm_destroy: true,
        },
    )
}

#[tauri::command]
fn inspect_mihomo_controller(input: MihomoControllerInput) -> Result<MihomoSnapshot, String> {
    mihomo::inspect_controller(&input).map_err(|error| error.to_string())
}

#[tauri::command]
fn list_silos(state: State<'_, AppState>) -> Result<Vec<Silo>, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.list_silos().map_err(|error| error.to_string())
}

#[tauri::command]
fn list_active_silos(state: State<'_, AppState>) -> Result<Vec<Silo>, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.list_active_silos().map_err(|error| error.to_string())
}

#[tauri::command]
fn list_archived_silos(state: State<'_, AppState>) -> Result<Vec<Silo>, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault
        .list_archived_silos()
        .map_err(|error| error.to_string())
}

fn managed_browser_package_root(state: &AppState) -> PathBuf {
    state
        .resource_root
        .join("managed-browser")
        .join("engine-package")
}

fn managed_vault_error(error: VaultError) -> String {
    match error {
        VaultError::SiloRunning => "managed_another_silo_running",
        VaultError::SiloProfileInUse => "managed_profile_in_use",
        VaultError::InvalidData | VaultError::SiloNotFound | VaultError::UnmanagedProfile => {
            "managed_artifact_unavailable"
        }
        VaultError::Filesystem(_) => "managed_runtime_recovery_required",
        _ => "managed_create_failed",
    }
    .to_owned()
}

fn managed_launcher_error(error: LauncherError) -> String {
    match error {
        LauncherError::AnotherSiloRunning => "managed_another_silo_running",
        LauncherError::ProfileInUse => "managed_profile_in_use",
        LauncherError::ProxyPreflight(_)
        | LauncherError::ProxyRelay(_)
        | LauncherError::InvalidNetwork(_)
        | LauncherError::Mihomo(_) => "managed_network_mismatch",
        LauncherError::RuntimeReceipt(_) | LauncherError::Bootstrap(_) => {
            "managed_runtime_recovery_required"
        }
        LauncherError::BrowserVerification(_)
        | LauncherError::BrowserStartup(_)
        | LauncherError::Engine(_)
        | LauncherError::Spawn(_) => "managed_engine_unavailable",
    }
    .to_owned()
}

fn managed_provision_roots(
    root: &std::path::Path,
    provision_id: Uuid,
) -> engine::CamoufoxHostRoots {
    let managed_root = root.join("silos").join(provision_id.to_string());
    engine::CamoufoxHostRoots {
        artifact_root: managed_root.join("identity"),
        profile_root: managed_root.join("profiles"),
        state_root: managed_root.join("engine-state"),
    }
}

fn provision_managed_artifact(
    root: &std::path::Path,
    package_root: &std::path::Path,
    relay_silo_id: Uuid,
    preset: ManagedIdentityPreset,
    network_profile: &NetworkProfile,
    proxy_credentials: Option<&ProxyCredentialsInput>,
    seed: &[u8; 32],
) -> Result<engine::CamoufoxProvisionResult, String> {
    let provision_id = Uuid::new_v4();
    let managed_root = root.join("silos").join(provision_id.to_string());
    let roots = managed_provision_roots(root, provision_id);
    let relay = if matches!(network_profile, NetworkProfile::FixedProxy { .. }) {
        let authentication = proxy_credentials.map(|credentials| {
            ProxyAuthentication::new(credentials.username.clone(), credentials.password.clone())
        });
        let relay = ProxyRelay::start(
            network_profile,
            relay_silo_id,
            Uuid::new_v4(),
            authentication,
        )
        .map_err(|_| "managed_network_mismatch".to_owned())?;
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        relay
            .verify_upstream_until(Instant::now() + Duration::from_secs(10), &cancelled)
            .map_err(|_| "managed_network_mismatch".to_owned())?;
        Some(relay)
    } else {
        None
    };
    let proxy_server = relay.as_ref().map(|relay| {
        format!(
            "socks5://{}:{}",
            relay.endpoint().host,
            relay.endpoint().port
        )
    });
    let result = (|| -> Result<engine::CamoufoxProvisionResult, String> {
        let mut adapter =
            ExternalPackageEngineAdapter::production_prototype(EngineAdapterId::Camoufox)
                .map_err(|_| "managed_engine_unavailable".to_owned())?;
        adapter
            .ensure_builtin_package(package_root)
            .map_err(|_| "managed_engine_unavailable".to_owned())?;
        adapter
            .provision_camoufox_artifact(&roots, preset.as_str(), seed, proxy_server.as_deref())
            .map_err(|_| "managed_identity_generation_failed".to_owned())
    })();
    let cleanup = match fs::remove_dir_all(&managed_root) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("managed_runtime_recovery_required".to_owned()),
    };
    match result {
        Ok(result) => {
            cleanup?;
            Ok(result)
        }
        Err(error) => match cleanup {
            Ok(()) => Err(error),
            Err(cleanup_error) => Err(cleanup_error),
        },
    }
}

#[tauri::command]
fn create_managed_silo(
    state: State<'_, AppState>,
    input: CreateManagedSiloInput,
) -> Result<Silo, String> {
    let _local_reservation = state.local_control.reserve()?;
    input
        .validate()
        .map_err(|_| "managed_create_failed".to_owned())?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    let package_root = managed_browser_package_root(&state);
    let mut seed = [0_u8; 32];
    OsRng.fill_bytes(&mut seed);
    let result = provision_managed_artifact(
        &state.root,
        &package_root,
        Uuid::new_v4(),
        input.identity_preset,
        &input.network_profile,
        input.proxy_credentials.as_ref(),
        &seed,
    )?;
    let artifact = StoredIdentityArtifact {
        artifact_id: result.artifact_id,
        schema: result.schema,
        raw_json: result.raw_json,
        raw_sha256: result.artifact_file_sha256,
    };
    vault
        .create_managed_silo(&state.root, input, artifact, &seed)
        .map_err(managed_vault_error)
}

#[tauri::command]
fn create_silo(state: State<'_, AppState>, input: CreateSiloInput) -> Result<Silo, String> {
    let _local_reservation = state.local_control.reserve()?;
    match &input.execution_target {
        SiloExecutionTarget::Local => {}
        SiloExecutionTarget::Wsl { distribution } => {
            prepare_wsl_distribution(
                &state,
                distribution,
                &[
                    EnvironmentOperation::ConfigureNetwork,
                    EnvironmentOperation::Start,
                    EnvironmentOperation::Stop,
                    EnvironmentOperation::Health,
                ],
            )?;
        }
        SiloExecutionTarget::Remote { .. } => {
            return Err(
                "Remote nodes are not yet selectable for a Silo because this build cannot verify that a browser and its device identity were applied there. Pairing a server alone is not enough, and VeriSilo will not pretend it is."
                    .to_owned(),
            );
        }
    }
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault
        .create_silo(&state.root, input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn update_silo(
    state: State<'_, AppState>,
    silo_id: Uuid,
    input: UpdateSiloInput,
) -> Result<Silo, String> {
    let _local_reservation = state.local_control.reserve()?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let is_active = runtime.is_active(silo_id) || environment_runtime_is_active(&state, silo_id)?;
    vault
        .update_silo(&state.root, silo_id, input, is_active)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn update_silo_configuration(
    state: State<'_, AppState>,
    silo_id: Uuid,
    input: UpdateSiloInput,
    network_input: Option<UpdateSiloNetworkInput>,
    engine_input: Option<UpdateSiloEngineInput>,
) -> Result<Silo, String> {
    let _local_reservation = state.local_control.reserve()?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let is_active = runtime.is_active(silo_id) || environment_runtime_is_active(&state, silo_id)?;
    drop(runtime);
    let current = vault.get_silo(silo_id).map_err(|error| error.to_string())?;
    if !current.engine.is_stock() && network_input.is_some() {
        if engine_input.is_some() {
            return Err("managed_artifact_unavailable".to_owned());
        }
        input
            .validate_managed_metadata()
            .map_err(|_| "managed_create_failed".to_owned())?;
        if is_active {
            return Err("managed_another_silo_running".to_owned());
        }
        let network_input = network_input.expect("checked managed network input");
        network_input
            .validate_for_execution_target(&current.execution_target)
            .map_err(|_| "managed_network_mismatch".to_owned())?;
        if !matches!(
            &network_input.network_profile,
            NetworkProfile::Direct {
                proxy_required: false
            } | NetworkProfile::FixedProxy {
                proxy_required: true,
                scheme: SiloProxyScheme::Http | SiloProxyScheme::Socks5,
                external_mihomo: None,
                ..
            }
        ) {
            return Err("managed_network_mismatch".to_owned());
        }
        let current_preset = vault
            .managed_identity_preset_for_silo(silo_id)
            .map_err(managed_vault_error)?;
        let preset = if network_input.network_profile.requires_proxy() {
            ManagedIdentityPreset::MatchFixedProxy
        } else {
            match current_preset {
                ManagedIdentityPreset::MatchFixedProxy => ManagedIdentityPreset::BalancedEnUs,
                preset => preset,
            }
        };
        let seed = vault
            .identity_seed_for_silo(silo_id)
            .map_err(managed_vault_error)?;
        let result = provision_managed_artifact(
            &state.root,
            &managed_browser_package_root(&state),
            silo_id,
            preset,
            &network_input.network_profile,
            network_input.proxy_credentials.as_ref(),
            &seed,
        )?;
        let artifact = StoredIdentityArtifact {
            artifact_id: result.artifact_id,
            schema: result.schema,
            raw_json: result.raw_json,
            raw_sha256: result.artifact_file_sha256,
        };
        return vault
            .rebind_managed_silo_configuration(
                &state.root,
                silo_id,
                Some(input),
                network_input,
                artifact,
            )
            .map_err(managed_vault_error);
    }
    vault
        .update_silo_configuration(
            &state.root,
            silo_id,
            input,
            network_input,
            engine_input,
            is_active,
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn rename_silo(state: State<'_, AppState>, silo_id: Uuid, name: String) -> Result<Silo, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let is_active = runtime.is_active(silo_id) || environment_runtime_is_active(&state, silo_id)?;
    vault
        .rename_silo(&state.root, silo_id, &name, is_active)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn update_silo_network(
    state: State<'_, AppState>,
    silo_id: Uuid,
    input: UpdateSiloNetworkInput,
) -> Result<Silo, String> {
    let _local_reservation = state.local_control.reserve()?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let is_active = runtime.is_active(silo_id) || environment_runtime_is_active(&state, silo_id)?;
    drop(runtime);
    let current = vault.get_silo(silo_id).map_err(|error| error.to_string())?;
    if !current.engine.is_stock() {
        input
            .validate_for_execution_target(&current.execution_target)
            .map_err(|_| "managed_network_mismatch".to_owned())?;
        if !matches!(
            &input.network_profile,
            NetworkProfile::Direct {
                proxy_required: false
            } | NetworkProfile::FixedProxy {
                proxy_required: true,
                scheme: SiloProxyScheme::Http | SiloProxyScheme::Socks5,
                external_mihomo: None,
                ..
            }
        ) {
            return Err("managed_network_mismatch".to_owned());
        }
        if is_active {
            return Err("managed_another_silo_running".to_owned());
        }
        let current_preset = vault
            .managed_identity_preset_for_silo(silo_id)
            .map_err(managed_vault_error)?;
        let preset = if input.network_profile.requires_proxy() {
            ManagedIdentityPreset::MatchFixedProxy
        } else {
            match current_preset {
                ManagedIdentityPreset::MatchFixedProxy => ManagedIdentityPreset::BalancedEnUs,
                preset => preset,
            }
        };
        let seed = vault
            .identity_seed_for_silo(silo_id)
            .map_err(managed_vault_error)?;
        let result = provision_managed_artifact(
            &state.root,
            &managed_browser_package_root(&state),
            silo_id,
            preset,
            &input.network_profile,
            input.proxy_credentials.as_ref(),
            &seed,
        )?;
        let artifact = StoredIdentityArtifact {
            artifact_id: result.artifact_id,
            schema: result.schema,
            raw_json: result.raw_json,
            raw_sha256: result.artifact_file_sha256,
        };
        return vault
            .rebind_managed_silo_network(&state.root, silo_id, input, artifact)
            .map_err(managed_vault_error);
    }
    vault
        .update_silo_network(&state.root, silo_id, input, is_active)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn update_silo_engine(
    state: State<'_, AppState>,
    silo_id: Uuid,
    input: UpdateSiloEngineInput,
) -> Result<Silo, String> {
    let _local_reservation = state.local_control.reserve()?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let is_active = runtime.is_active(silo_id) || environment_runtime_is_active(&state, silo_id)?;
    vault
        .update_silo_engine(&state.root, silo_id, input, is_active)
        .map_err(|error| error.to_string())
}

fn verified_current_wsl_artifact(
    root: &std::path::Path,
    silo: &Silo,
) -> Result<Option<String>, String> {
    let environment_root = root.join("environments");
    let artifacts = environment::backend::local_environment_artifacts(
        &environment_root,
        silo.id,
    )
    .map_err(|_| {
        "VeriSilo could not verify the saved run environment. It will not change or delete it automatically."
            .to_owned()
    })?;
    if artifacts.is_empty() {
        return Ok(None);
    }
    let SiloExecutionTarget::Wsl { distribution } = &silo.execution_target else {
        return Err(
            "Clean the older run environment shown in Overview before archiving or deleting this Silo."
                .to_owned(),
        );
    };
    if artifacts.as_slice() != [EnvironmentBackendId::WslChromium] {
        return Err(
            "This Silo has an additional older run environment. Clean it from Overview before continuing."
                .to_owned(),
        );
    }
    let bound_distribution = environment::backend::local_environment_binding_provider(
        &environment_root,
        silo.id,
        EnvironmentBackendId::WslChromium,
    )
    .map_err(|_| {
        "The Linux run environment ownership record is incomplete. VeriSilo will not change or delete it automatically."
            .to_owned()
    })?
    .ok_or_else(|| {
        "The Linux run environment ownership record is missing. VeriSilo will not change or delete it automatically."
            .to_owned()
    })?;
    if &bound_distribution != distribution {
        return Err(
            "The saved Linux run location and its ownership record disagree. VeriSilo will not change either side automatically."
                .to_owned(),
        );
    }
    Ok(Some(bound_distribution))
}

#[tauri::command]
fn archive_silo(state: State<'_, AppState>, silo_id: Uuid) -> Result<(), String> {
    let _local_reservation = state.local_control.reserve()?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let is_active = runtime.is_active(silo_id) || environment_runtime_is_active(&state, silo_id)?;
    let silo = vault.get_silo(silo_id).map_err(|error| error.to_string())?;
    verified_current_wsl_artifact(&state.root, &silo)?;
    let profile_directory = vault
        .silo_profile_directory(silo_id)
        .map_err(|error| error.to_string())?;
    vault
        .archive_silo(
            &state.root,
            silo_id,
            is_active || profile_in_use(&profile_directory),
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn restore_archived_silo(state: State<'_, AppState>, silo_id: Uuid) -> Result<Silo, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault
        .restore_archived_silo(&state.root, silo_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_silo(
    state: State<'_, AppState>,
    silo_id: Uuid,
    confirm_permanent: bool,
) -> Result<(), String> {
    if !confirm_permanent {
        return Err("Permanent deletion requires explicit confirmation.".to_owned());
    }
    let _local_reservation = state.local_control.reserve()?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let is_active = runtime.is_active(silo_id) || environment_runtime_is_active(&state, silo_id)?;
    if is_active {
        return Err("Stop this Silo before permanently deleting it.".to_owned());
    }
    let silo = vault.get_silo(silo_id).map_err(|error| error.to_string())?;
    if vault
        .remote_control_plane()
        .map_err(|error| error.to_string())?
        .backend
        .bindings
        .iter()
        .any(|binding| binding.silo_id == silo_id)
    {
        return Err(
            "Destroy or force-detach the remote environment before permanently deleting this Silo."
                .to_owned(),
        );
    }
    let profile_directory = vault
        .silo_profile_directory(silo_id)
        .map_err(|error| error.to_string())?;
    if profile_in_use(&profile_directory) {
        return Err("The Silo browser profile is still in use.".to_owned());
    }
    let wsl_distribution = verified_current_wsl_artifact(&state.root, &silo)?;
    drop(runtime);
    drop(vault);

    if let Some(distribution) = wsl_distribution {
        prepare_wsl_distribution(&state, &distribution, &[EnvironmentOperation::Destroy])?;
        let mut environments = state
            .environments
            .lock()
            .map_err(|_| "VeriSilo environment provider state is unavailable.".to_owned())?;
        environments
            .execute(EnvironmentOperationRequest::Destroy {
                backend: EnvironmentBackendId::WslChromium,
                environment_id: silo_id,
                confirm_destroy: true,
            })
            .map_err(|error| error.to_string())?;
    }

    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let is_active = runtime.is_active(silo_id) || environment_runtime_is_active(&state, silo_id)?;
    vault
        .delete_silo(&state.root, silo_id, is_active, confirm_permanent)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn silo_storage_usage(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<SiloStorageUsage, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault
        .silo_storage_usage(&state.root, silo_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_network_evidence(
    state: State<'_, AppState>,
    silo_id: Option<Uuid>,
) -> Result<Vec<native_host::NativeNetworkEvidenceInboxEntry>, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault
        .list_network_evidence(silo_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn clear_network_evidence(
    state: State<'_, AppState>,
    silo_id: Uuid,
    confirm_clear: bool,
) -> Result<usize, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault
        .clear_network_evidence(&state.root, silo_id, confirm_clear)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn recheck_silo_browser(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<BrowserVerification, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let is_active = runtime.is_active(silo_id) || environment_runtime_is_active(&state, silo_id)?;
    vault
        .recheck_silo_browser(&state.root, silo_id, is_active)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn recheck_silo_runtime(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RuntimeActivation, String> {
    let _local_reservation = state.local_control.reserve()?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    let silo = vault.get_silo(silo_id).map_err(|error| error.to_string())?;
    let vault_status = vault.status(&state.root);
    match &silo.execution_target {
        SiloExecutionTarget::Local => {
            let proxy_authentication = vault
                .proxy_authentication_for_silo(silo_id)
                .map_err(|error| error.to_string())?;
            let mihomo_authentication = vault
                .mihomo_controller_authentication_for_silo(silo_id)
                .map_err(|error| error.to_string())?;
            let mut runtime = state
                .runtime
                .lock()
                .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
            drop(vault);
            let activation = runtime
                .recheck_active(
                    &silo,
                    proxy_authentication.as_ref(),
                    mihomo_authentication.as_ref(),
                )
                .map_err(|error| error.to_string())?;
            publish_runtime_status(&state, &activation, &vault_status);
            Ok(activation)
        }
        SiloExecutionTarget::Wsl { distribution } => {
            let distribution = distribution.clone();
            {
                let environment_runtime = state.environment_runtime.lock().map_err(|_| {
                    "VeriSilo environment runtime state is unavailable.".to_owned()
                })?;
                if environment_runtime.has_active_silo()
                    && environment_runtime.activation.active_silo_id != Some(silo_id)
                {
                    return Err(
                        "A different Silo is active; its runtime state cannot be replaced by this health check."
                            .to_owned(),
                    );
                }
                if !environment_runtime.is_active(silo_id) {
                    return Ok(environment_runtime.activation.clone());
                }
            }
            drop(vault);
            let health = (|| -> Result<(), String> {
                prepare_wsl_distribution(
                    &state,
                    &distribution,
                    &[EnvironmentOperation::Health],
                )?;
                let mut environments = state.environments.lock().map_err(|_| {
                    "VeriSilo environment provider state is unavailable.".to_owned()
                })?;
                environments
                    .execute(EnvironmentOperationRequest::Health {
                        backend: EnvironmentBackendId::WslChromium,
                        environment_id: silo_id,
                    })
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })();
            let record = EnvironmentRuntimeRecord {
                schema_version: ENVIRONMENT_RUNTIME_RECORD_SCHEMA_VERSION,
                silo_id,
                distribution: distribution.clone(),
            };
            let activation = match health {
                Ok(()) => RuntimeActivation {
                    active_silo_id: Some(silo_id),
                    state: RuntimeState::Running,
                    updated_at: Utc::now(),
                    message: Some(format!(
                        "The isolated Linux browser is healthy in {distribution}."
                    )),
                    browser_verification: None,
                    engine_evidence: None,
                    network_evidence: None,
                },
                Err(error) => {
                    let stopped = (|| -> Result<(), String> {
                        prepare_wsl_distribution(
                            &state,
                            &distribution,
                            &[EnvironmentOperation::Stop],
                        )?;
                        let mut environments = state.environments.lock().map_err(|_| {
                            "VeriSilo environment provider state is unavailable.".to_owned()
                        })?;
                        environments
                            .execute(EnvironmentOperationRequest::Stop {
                                backend: EnvironmentBackendId::WslChromium,
                                environment_id: silo_id,
                            })
                            .map(|_| ())
                            .map_err(|stop_error| stop_error.to_string())
                    })();
                    if stopped.is_ok()
                        && clear_environment_runtime_record(&state.root, &record).is_ok()
                    {
                        RuntimeActivation {
                            active_silo_id: None,
                            state: RuntimeState::Stopped,
                            updated_at: Utc::now(),
                            message: Some(format!(
                                "The Linux browser was no longer healthy and has been stopped safely: {error}"
                            )),
                            browser_verification: None,
                            engine_evidence: None,
                            network_evidence: None,
                        }
                    } else {
                        RuntimeActivation {
                            active_silo_id: Some(silo_id),
                            state: RuntimeState::RecoveryRequired,
                            updated_at: Utc::now(),
                            message: Some(format!(
                                "The Linux browser could not be verified or stopped safely: {error}"
                            )),
                            browser_verification: None,
                            engine_evidence: None,
                            network_evidence: None,
                        }
                    }
                }
            };
            let mut environment_runtime = state.environment_runtime.lock().map_err(|_| {
                "VeriSilo environment runtime state is unavailable.".to_owned()
            })?;
            environment_runtime.activation = activation.clone();
            environment_runtime.wsl_distribution = activation.active_silo_id.map(|_| distribution);
            environment_runtime.reconciled = true;
            environment_runtime.recovery_blocked = matches!(
                activation.state,
                RuntimeState::RecoveryRequired
            );
            publish_runtime_status(&state, &activation, &vault_status);
            Ok(activation)
        }
        SiloExecutionTarget::Remote { .. } => Err(
            "Remote browser health is unavailable because this build cannot verify a remote identity runtime."
                .to_owned(),
        ),
    }
}

#[tauri::command]
fn stop_silo(state: State<'_, AppState>, silo_id: Uuid) -> Result<RuntimeActivation, String> {
    let _local_reservation = state.local_control.reserve()?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    let silo = vault.get_silo(silo_id).map_err(|error| error.to_string())?;
    let vault_status = vault.status(&state.root);
    match silo.execution_target {
        SiloExecutionTarget::Wsl { distribution } => {
            {
                let environment_runtime = state.environment_runtime.lock().map_err(|_| {
                    "VeriSilo environment runtime state is unavailable.".to_owned()
                })?;
                if !environment_runtime.is_active(silo_id) {
                    return Err(
                        "This Silo is not the active Linux runtime; refusing to stop another environment."
                            .to_owned(),
                    );
                }
            }
            drop(vault);
            let stop_result = (|| -> Result<(), String> {
                prepare_wsl_distribution(
                    &state,
                    &distribution,
                    &[EnvironmentOperation::Stop],
                )?;
                let mut environments = state.environments.lock().map_err(|_| {
                    "VeriSilo environment provider state is unavailable.".to_owned()
                })?;
                environments
                    .execute(EnvironmentOperationRequest::Stop {
                        backend: EnvironmentBackendId::WslChromium,
                        environment_id: silo_id,
                    })
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })();
            let record = EnvironmentRuntimeRecord {
                schema_version: ENVIRONMENT_RUNTIME_RECORD_SCHEMA_VERSION,
                silo_id,
                distribution: distribution.clone(),
            };
            if let Err(error) = stop_result {
                let mut environment_runtime = state.environment_runtime.lock().map_err(|_| {
                    "VeriSilo environment runtime state is unavailable.".to_owned()
                })?;
                environment_runtime.activation.state = RuntimeState::RecoveryRequired;
                environment_runtime.activation.updated_at = Utc::now();
                environment_runtime.activation.message = Some(format!(
                    "The Linux browser could not be stopped safely: {error}"
                ));
                environment_runtime.reconciled = true;
                environment_runtime.recovery_blocked = true;
                return Err(error);
            }
            if let Err(error) = clear_environment_runtime_record(&state.root, &record) {
                let mut environment_runtime = state.environment_runtime.lock().map_err(|_| {
                    "VeriSilo environment runtime state is unavailable.".to_owned()
                })?;
                environment_runtime.activation.state = RuntimeState::RecoveryRequired;
                environment_runtime.activation.updated_at = Utc::now();
                environment_runtime.activation.message = Some(error.clone());
                environment_runtime.reconciled = true;
                environment_runtime.recovery_blocked = true;
                return Err(error);
            }
            let activation = RuntimeActivation {
                active_silo_id: None,
                state: RuntimeState::Stopped,
                updated_at: Utc::now(),
                message: Some("The isolated Linux browser was stopped.".to_owned()),
                browser_verification: None,
                engine_evidence: None,
                network_evidence: None,
            };
            let mut environment_runtime = state.environment_runtime.lock().map_err(|_| {
                "VeriSilo environment runtime state is unavailable.".to_owned()
            })?;
            environment_runtime.activation = activation.clone();
            environment_runtime.wsl_distribution = None;
            environment_runtime.reconciled = true;
            environment_runtime.recovery_blocked = false;
            publish_runtime_status(&state, &activation, &vault_status);
            Ok(activation)
        }
        SiloExecutionTarget::Local
            if silo.adapter_id() == EngineAdapterId::Camoufox =>
        {
            drop(vault);
            let mut runtime = state
                .runtime
                .lock()
                .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
            let activation = runtime
                .stop_managed_camoufox(silo_id)
                .map_err(managed_launcher_error)?;
            publish_runtime_status(&state, &activation, &vault_status);
            Ok(activation)
        }
        SiloExecutionTarget::Local => Err(
            "Close the Silo browser window to stop a browser running on this computer. VeriSilo will not terminate unrelated browser processes."
                .to_owned(),
        ),
        SiloExecutionTarget::Remote { .. } => Err(
            "Remote stop is unavailable because this build has no verified remote browser runtime."
                .to_owned(),
        ),
    }
}

#[tauri::command]
fn rebind_silo_mihomo(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RuntimeActivation, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    let silo = vault.get_silo(silo_id).map_err(|error| error.to_string())?;
    if !matches!(silo.execution_target, SiloExecutionTarget::Local) {
        return Err(
            "This network reconnection action is available only for a Silo running on this computer."
                .to_owned(),
        );
    }
    let proxy_authentication = vault
        .proxy_authentication_for_silo(silo_id)
        .map_err(|error| error.to_string())?;
    let mihomo_authentication = vault
        .mihomo_controller_authentication_for_silo(silo_id)
        .map_err(|error| error.to_string())?;
    let vault_status = vault.status(&state.root);
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    drop(vault);
    let activation = runtime
        .rebind_active_mihomo(
            &silo,
            proxy_authentication.as_ref(),
            mihomo_authentication.as_ref(),
        )
        .map_err(|error| error.to_string())?;
    publish_runtime_status(&state, &activation, &vault_status);
    Ok(activation)
}

#[tauri::command]
fn launch_silo(state: State<'_, AppState>, silo_id: Uuid) -> Result<RuntimeActivation, String> {
    let _local_reservation = state.local_control.reserve()?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    let silo = vault.get_silo(silo_id).map_err(|error| error.to_string())?;
    let managed_camoufox = silo.adapter_id() == EngineAdapterId::Camoufox;
    let mut vault_status = vault.status(&state.root);
    match silo.execution_target.clone() {
        SiloExecutionTarget::Local => {
            let managed_profile_directories = vault
                .managed_profile_directories()
                .map_err(|error| error.to_string())?;
            let proxy_authentication = vault
                .proxy_authentication_for_silo(silo_id)
                .map_err(|error| error.to_string())?;
            let mihomo_authentication = vault
                .mihomo_controller_authentication_for_silo(silo_id)
                .map_err(|error| error.to_string())?;
            let identity_seed = matches!(&silo.engine, SiloEngineConfig::ControlledChromium { .. })
                .then(|| vault.identity_seed_for_silo(silo_id))
                .transpose()
                .map_err(|error| error.to_string())?;
            let identity_deriver = identity_seed
                .as_ref()
                .map(|seed| VaultSeedIdentityTokenDeriver::new(seed.as_ref()))
                .transpose()
                .map_err(|error| error.to_string())?;
            let mut runtime = state
                .runtime
                .lock()
                .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
            if runtime.activation().active_silo_id.is_some() {
                return Err(if managed_camoufox {
                    "managed_another_silo_running".to_owned()
                } else {
                    "Close the active browser Silo before starting another Silo.".to_owned()
                });
            }
            {
                let mut environment_runtime = state.environment_runtime.lock().map_err(|_| {
                    "VeriSilo environment runtime state is unavailable.".to_owned()
                })?;
                if environment_runtime.has_active_silo() {
                    return Err(if managed_camoufox {
                        "managed_another_silo_running".to_owned()
                    } else {
                        "Stop the active Silo before starting another run location.".to_owned()
                    });
                }
                environment_runtime.activation = RuntimeActivation::idle();
                environment_runtime.wsl_distribution = None;
                environment_runtime.reconciled = true;
                environment_runtime.recovery_blocked = false;
            }
            // Runtime is now reserved, so no edit command can pass its active
            // check between reading Vault metadata and starting this exact
            // configuration.
            let silo = vault
                .mark_silo_identity_locked(&state.root, silo_id)
                .map_err(|error| error.to_string())?;
            if silo.adapter_id() == EngineAdapterId::Camoufox {
                vault
                    .materialize_identity_artifact(&state.root, silo_id)
                    .map_err(managed_vault_error)?;
            }
            vault_status = vault.status(&state.root);
            drop(vault);
            match runtime.launch_with_identity_deriver(
                &silo,
                &managed_profile_directories,
                proxy_authentication,
                mihomo_authentication,
                identity_deriver
                    .as_ref()
                    .map(|deriver| deriver as &dyn engine::IdentityTokenDeriver),
            ) {
                Ok(activation) => {
                    drop(runtime);
                    publish_runtime_status(&state, &activation, &vault_status);
                    Ok(activation)
                }
                Err(error) => {
                    let activation = runtime.activation();
                    publish_runtime_status(&state, &activation, &vault_status);
                    Err(if managed_camoufox {
                        managed_launcher_error(error)
                    } else {
                        error.to_string()
                    })
                }
            }
        }
        SiloExecutionTarget::Wsl { distribution } => {
            let network = wsl_network_profile(&silo)?;
            let mut runtime = state
                .runtime
                .lock()
                .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
            if runtime.activation().active_silo_id.is_some() {
                return Err(
                    "Close the browser Silo already running on this computer before starting the Linux environment."
                        .to_owned(),
                );
            }
            drop(runtime);
            {
                let mut environment_runtime = state.environment_runtime.lock().map_err(|_| {
                    "VeriSilo environment runtime state is unavailable.".to_owned()
                })?;
                if environment_runtime.has_active_silo() {
                    return Err(
                        "Stop the active Silo before starting another run location.".to_owned(),
                    );
                }
                environment_runtime.activation = RuntimeActivation {
                    active_silo_id: Some(silo_id),
                    state: RuntimeState::Preflight,
                    updated_at: Utc::now(),
                    message: Some(format!(
                        "Checking the saved Linux environment {distribution}."
                    )),
                    browser_verification: None,
                    engine_evidence: None,
                    network_evidence: None,
                };
                environment_runtime.wsl_distribution = Some(distribution.clone());
                environment_runtime.reconciled = true;
                environment_runtime.recovery_blocked = false;
            }
            drop(vault);

            let record = EnvironmentRuntimeRecord {
                schema_version: ENVIRONMENT_RUNTIME_RECORD_SCHEMA_VERSION,
                silo_id,
                distribution: distribution.clone(),
            };
            let mut record_persisted = false;
            let mut start_attempted = false;
            let launch_result = (|| -> Result<RuntimeActivation, String> {
                prepare_wsl_distribution(
                    &state,
                    &distribution,
                    &[
                        EnvironmentOperation::ConfigureNetwork,
                        EnvironmentOperation::Start,
                        EnvironmentOperation::Stop,
                    ],
                )?;
                let mut environments = state.environments.lock().map_err(|_| {
                    "VeriSilo environment provider state is unavailable.".to_owned()
                })?;
                if environments
                    .execute(EnvironmentOperationRequest::Health {
                        backend: EnvironmentBackendId::WslChromium,
                        environment_id: silo_id,
                    })
                    .is_ok()
                {
                    environments
                        .execute(EnvironmentOperationRequest::Stop {
                            backend: EnvironmentBackendId::WslChromium,
                            environment_id: silo_id,
                        })
                        .map_err(|error| {
                            format!(
                                "A previous browser process is still bound to this Silo and could not be stopped safely: {error}"
                            )
                        })?;
                }
                environments
                    .execute(EnvironmentOperationRequest::ConfigureNetwork {
                        backend: EnvironmentBackendId::WslChromium,
                        environment_id: silo_id,
                        network,
                    })
                    .map_err(|error| error.to_string())?;
                drop(environments);

                persist_environment_runtime_record(&state.root, &record)?;
                record_persisted = true;
                {
                    let mut vault = state
                        .vault
                        .lock()
                        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
                    vault
                        .mark_silo_identity_locked(&state.root, silo_id)
                        .map_err(|error| error.to_string())?;
                    vault_status = vault.status(&state.root);
                }
                start_attempted = true;
                let mut environments = state.environments.lock().map_err(|_| {
                    "VeriSilo environment provider state is unavailable.".to_owned()
                })?;
                environments
                    .execute(EnvironmentOperationRequest::Start {
                        backend: EnvironmentBackendId::WslChromium,
                        environment_id: silo_id,
                    })
                    .map_err(|error| error.to_string())?;
                Ok(RuntimeActivation {
                    active_silo_id: Some(silo_id),
                    state: RuntimeState::Running,
                    updated_at: Utc::now(),
                    message: Some(format!(
                        "Silo is running in the saved Linux environment {distribution}."
                    )),
                    browser_verification: None,
                    engine_evidence: None,
                    network_evidence: None,
                })
            })();

            match launch_result {
                Ok(activation) => {
                    let mut environment_runtime = state.environment_runtime.lock().map_err(|_| {
                        "VeriSilo environment runtime state is unavailable.".to_owned()
                    })?;
                    environment_runtime.activation = activation.clone();
                    environment_runtime.wsl_distribution = Some(distribution);
                    environment_runtime.reconciled = true;
                    environment_runtime.recovery_blocked = false;
                    publish_runtime_status(&state, &activation, &vault_status);
                    Ok(activation)
                }
                Err(error) => {
                    let cleanup_result = if !record_persisted {
                        match environment_runtime_record_exists(&state.root) {
                            Ok(false) => Ok(()),
                            Ok(true) => Err(
                                "A Linux runtime record exists after the failed launch; recovery is required before another Silo can start."
                                    .to_owned(),
                            ),
                            Err(inspect_error) => Err(inspect_error),
                        }
                    } else if start_attempted {
                        (|| -> Result<(), String> {
                            prepare_wsl_distribution(
                                &state,
                                &distribution,
                                &[EnvironmentOperation::Stop],
                            )?;
                            let mut environments = state.environments.lock().map_err(|_| {
                                "VeriSilo environment provider state is unavailable.".to_owned()
                            })?;
                            environments
                                .execute(EnvironmentOperationRequest::Stop {
                                    backend: EnvironmentBackendId::WslChromium,
                                    environment_id: silo_id,
                                })
                                .map_err(|stop_error| stop_error.to_string())?;
                            clear_environment_runtime_record(&state.root, &record)
                        })()
                    } else {
                        clear_environment_runtime_record(&state.root, &record)
                    };
                    let cleanup_failed = cleanup_result.is_err();
                    let activation = if cleanup_failed {
                        RuntimeActivation {
                            active_silo_id: Some(silo_id),
                            state: RuntimeState::RecoveryRequired,
                            updated_at: Utc::now(),
                            message: Some(format!(
                                "The Linux browser did not start cleanly and its exact runtime could not be confirmed stopped: {error}. Cleanup also failed: {}",
                                cleanup_result.expect_err("checked cleanup failure")
                            )),
                            browser_verification: None,
                            engine_evidence: None,
                            network_evidence: None,
                        }
                    } else {
                        RuntimeActivation {
                            active_silo_id: None,
                            state: RuntimeState::Failed,
                            updated_at: Utc::now(),
                            message: Some(error.clone()),
                            browser_verification: None,
                            engine_evidence: None,
                            network_evidence: None,
                        }
                    };
                    let mut environment_runtime = state.environment_runtime.lock().map_err(|_| {
                        "VeriSilo environment runtime state is unavailable.".to_owned()
                    })?;
                    environment_runtime.activation = activation.clone();
                    environment_runtime.wsl_distribution =
                        cleanup_failed.then(|| distribution.clone());
                    environment_runtime.reconciled = true;
                    environment_runtime.recovery_blocked = cleanup_failed;
                    publish_runtime_status(&state, &activation, &vault_status);
                    if cleanup_failed {
                        Err(activation
                            .message
                            .clone()
                            .unwrap_or(error))
                    } else {
                        Err(error)
                    }
                }
            }
        }
        SiloExecutionTarget::Remote { .. } => Err(
            "This Silo targets a remote node, but this build has no verified remote browser identity runtime. VeriSilo will not fall back to the local computer."
                .to_owned(),
        ),
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }))
        .setup(|app| {
            let resource_root = app
                .path()
                .resource_dir()
                .map_err(|error| format!("VeriSilo resource directory is unavailable: {error}"))?;
            app.manage(AppState::new(resource_root));

            let open_item =
                MenuItem::with_id(app, TRAY_OPEN_ID, "打开 VeriSilo", true, None::<&str>)?;
            let exit_item =
                MenuItem::with_id(app, TRAY_EXIT_ID, "退出 VeriSilo", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&open_item, &exit_item])?;
            let tray_icon = app
                .default_window_icon()
                .cloned()
                .ok_or("VeriSilo tray icon is unavailable")?;
            let main_window = app
                .get_webview_window("main")
                .ok_or("VeriSilo main window is unavailable")?;
            let window_to_hide = main_window.clone();
            main_window.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window_to_hide.hide();
                }
            });

            TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .tooltip("VeriSilo")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    TRAY_OPEN_ID => show_main_window(app),
                    TRAY_EXIT_ID => exit_from_tray(app),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button,
                        button_state,
                        ..
                    } = event
                    {
                        if is_tray_primary_activation(button, button_state) {
                            show_main_window(tray.app_handle());
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            desktop_status,
            initialize_vault,
            unlock_vault,
            lock_vault,
            change_vault_passphrase,
            backup_vault,
            restore_vault,
            discover_browsers,
            list_engine_adapters,
            install_engine_package,
            update_engine_package,
            rollback_engine_package,
            set_engine_emergency_disabled,
            remote_environment_status,
            validate_remote_environment_endpoint,
            pair_remote_environment,
            rotate_remote_environment_tls_pin,
            revoke_remote_pairing,
            force_detach_remote_environment,
            remote_environment_create,
            remote_environment_start,
            remote_environment_stop,
            remote_environment_pause,
            remote_environment_snapshot,
            remote_environment_destroy,
            remote_environment_configure_network,
            remote_environment_health,
            remote_environment_logs,
            remote_environment_open_human_session,
            remote_environment_close_human_session,
            remote_environment_grant_automation,
            remote_environment_revoke_automation,
            remote_environment_open_screen,
            remote_environment_send_input,
            detect_wsl,
            environment_backend_statuses,
            select_wsl_environment_distribution,
            environment_backend_execute,
            list_legacy_environment_artifacts,
            cleanup_legacy_environment_artifact,
            inspect_mihomo_controller,
            list_silos,
            list_active_silos,
            list_archived_silos,
            create_managed_silo,
            create_silo,
            update_silo,
            update_silo_configuration,
            rename_silo,
            update_silo_network,
            update_silo_engine,
            archive_silo,
            restore_archived_silo,
            delete_silo,
            silo_storage_usage,
            list_network_evidence,
            clear_network_evidence,
            recheck_silo_browser,
            recheck_silo_runtime,
            stop_silo,
            rebind_silo_mihomo,
            launch_silo,
        ])
        .run(tauri::generate_context!())
        .expect("error while running VeriSilo");
}

#[cfg(test)]
mod local_lifecycle_tests {
    use std::sync::TryLockError;
    use std::{fs, path::PathBuf};

    use chrono::Utc;
    use tauri::tray::{MouseButton, MouseButtonState};
    use uuid::Uuid;

    use super::{
        is_tray_primary_activation, managed_launcher_error, managed_vault_error,
        verified_current_wsl_artifact, LocalEnvironmentControl,
    };
    use crate::domain::{
        BrowserDescriptor, BrowserKind, CreateSiloInput, NetworkProfile, Silo, SiloExecutionTarget,
        SCHEMA_VERSION,
    };
    use crate::launcher::LauncherError;
    use crate::vault::{VaultError, VaultRuntime};

    fn temporary_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("verisilo-lib-{label}-{}", Uuid::new_v4()))
    }

    fn wsl_silo(id: Uuid, distribution: &str) -> Silo {
        Silo {
            id,
            schema_version: SCHEMA_VERSION,
            name: "WSL artifact test".to_owned(),
            color: "#5b5ce2".to_owned(),
            browser: Some(BrowserDescriptor {
                kind: BrowserKind::Chrome,
                executable_path: "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe"
                    .to_owned(),
                version: Some("126.0.0.0".to_owned()),
            }),
            execution_target: SiloExecutionTarget::Wsl {
                distribution: distribution.to_owned(),
            },
            profile_directory: "C:\\Users\\Test\\AppData\\Local\\VeriSilo\\silo".to_owned(),
            network_profile: NetworkProfile::Direct {
                proxy_required: false,
            },
            engine: Default::default(),
            seed_reference: Uuid::new_v4(),
            created_at: Utc::now(),
            identity_locked_at: None,
            archived_at: None,
        }
    }

    fn wsl_artifact_directory(root: &std::path::Path, silo_id: Uuid) -> PathBuf {
        root.join("environments")
            .join("wsl")
            .join(silo_id.to_string())
    }

    fn write_wsl_binding(root: &std::path::Path, silo_id: Uuid, distribution: &str) -> PathBuf {
        let directory = wsl_artifact_directory(root, silo_id);
        fs::create_dir_all(&directory).expect("create WSL artifact directory");
        fs::write(
            directory.join("binding.json"),
            serde_json::to_vec(&serde_json::json!({
                "schemaVersion": crate::environment::backend::ENVIRONMENT_CONTRACT_VERSION,
                "environmentId": silo_id,
                "backend": "wsl-chromium",
                "providerKey": distribution,
            }))
            .expect("serialize WSL binding"),
        )
        .expect("write WSL binding");
        directory
    }

    #[test]
    fn verified_current_wsl_artifact_accepts_only_one_matching_wsl_binding() {
        let root = temporary_root("verified-wsl-binding");
        let silo_id = Uuid::new_v4();
        let silo = wsl_silo(silo_id, "Ubuntu-24.04");
        write_wsl_binding(&root, silo_id, "Ubuntu-24.04");

        assert_eq!(
            verified_current_wsl_artifact(&root, &silo).expect("verify matching WSL artifact"),
            Some("Ubuntu-24.04".to_owned())
        );

        fs::remove_dir_all(root).expect("remove matching WSL fixture");
    }

    #[test]
    fn verified_current_wsl_artifact_returns_none_without_an_artifact() {
        let root = temporary_root("verified-wsl-none");
        fs::create_dir_all(&root).expect("create empty WSL fixture");
        let silo = wsl_silo(Uuid::new_v4(), "Ubuntu-24.04");

        assert_eq!(
            verified_current_wsl_artifact(&root, &silo).expect("verify missing WSL artifact"),
            None
        );

        fs::remove_dir_all(root).expect("remove empty WSL fixture");
    }

    #[test]
    fn verified_current_wsl_artifact_rejects_extra_backend_artifacts() {
        let root = temporary_root("verified-wsl-extra-backend");
        let silo_id = Uuid::new_v4();
        let silo = wsl_silo(silo_id, "Ubuntu-24.04");
        write_wsl_binding(&root, silo_id, "Ubuntu-24.04");
        fs::create_dir_all(
            root.join("environments")
                .join("sandbox")
                .join(silo_id.to_string()),
        )
        .expect("create extra backend artifact");

        assert!(verified_current_wsl_artifact(&root, &silo).is_err());

        fs::remove_dir_all(root).expect("remove extra backend fixture");
    }

    #[test]
    fn verified_current_wsl_artifact_rejects_missing_or_incomplete_binding() {
        let missing_root = temporary_root("verified-wsl-missing-binding");
        let missing_id = Uuid::new_v4();
        let missing_silo = wsl_silo(missing_id, "Ubuntu-24.04");
        fs::create_dir_all(wsl_artifact_directory(&missing_root, missing_id))
            .expect("create missing binding artifact");
        assert!(verified_current_wsl_artifact(&missing_root, &missing_silo).is_err());
        fs::remove_dir_all(missing_root).expect("remove missing binding fixture");

        let incomplete_root = temporary_root("verified-wsl-incomplete-binding");
        let incomplete_id = Uuid::new_v4();
        let incomplete_silo = wsl_silo(incomplete_id, "Ubuntu-24.04");
        let directory = wsl_artifact_directory(&incomplete_root, incomplete_id);
        fs::create_dir_all(&directory).expect("create incomplete binding artifact");
        fs::write(directory.join("binding.json"), b"{}").expect("write incomplete binding");
        assert!(verified_current_wsl_artifact(&incomplete_root, &incomplete_silo).is_err());
        fs::remove_dir_all(incomplete_root).expect("remove incomplete binding fixture");
    }

    #[test]
    fn verified_current_wsl_artifact_rejects_distribution_mismatch() {
        let root = temporary_root("verified-wsl-distribution-mismatch");
        let silo_id = Uuid::new_v4();
        let silo = wsl_silo(silo_id, "Ubuntu-24.04");
        write_wsl_binding(&root, silo_id, "Debian");

        assert!(verified_current_wsl_artifact(&root, &silo).is_err());

        fs::remove_dir_all(root).expect("remove distribution mismatch fixture");
    }

    #[test]
    fn wsl_destroy_then_transient_vault_failure_can_be_retried_safely() {
        let root = temporary_root("wsl-delete-retry");
        fs::create_dir_all(&root).expect("create WSL delete fixture");
        let mut vault = VaultRuntime::default();
        vault
            .initialize(&root, "a WSL deletion retry passphrase")
            .expect("initialize retry Vault");
        let silo = vault
            .create_silo(
                &root,
                CreateSiloInput {
                    name: "WSL delete retry".to_owned(),
                    color: "#5b5ce2".to_owned(),
                    browser_kind: BrowserKind::Chrome,
                    executable_path: "/usr/bin/chromium".to_owned(),
                    execution_target: SiloExecutionTarget::Wsl {
                        distribution: "Ubuntu-24.04".to_owned(),
                    },
                    network_profile: NetworkProfile::Direct {
                        proxy_required: false,
                    },
                    engine: Default::default(),
                    proxy_credentials: None,
                    mihomo_controller_secret: None,
                },
            )
            .expect("create WSL Silo");
        let artifact = write_wsl_binding(&root, silo.id, "Ubuntu-24.04");
        assert!(verified_current_wsl_artifact(&root, &silo)
            .expect("verify WSL artifact before destroy")
            .is_some());

        fs::remove_dir_all(artifact).expect("simulate successful WSL destroy");
        assert!(vault.delete_silo(&root, silo.id, true, true).is_err());
        assert_eq!(vault.list_silos().expect("list retained Silo").len(), 1);

        vault
            .delete_silo(&root, silo.id, false, true)
            .expect("retry Vault deletion after transient failure");
        assert!(vault.list_silos().expect("list deleted Silos").is_empty());

        fs::remove_dir_all(root).expect("remove WSL delete fixture");
    }

    #[test]
    fn tray_primary_activation_is_a_completed_left_click() {
        assert!(is_tray_primary_activation(
            MouseButton::Left,
            MouseButtonState::Up
        ));
        assert!(!is_tray_primary_activation(
            MouseButton::Left,
            MouseButtonState::Down
        ));
        assert!(!is_tray_primary_activation(
            MouseButton::Right,
            MouseButtonState::Up
        ));
    }

    #[test]
    fn managed_failures_return_stable_user_codes_without_internal_details() {
        assert_eq!(
            managed_launcher_error(LauncherError::ProxyPreflight(
                "proxy.internal.example:1080".to_owned(),
            )),
            "managed_network_mismatch"
        );
        assert_eq!(
            managed_vault_error(VaultError::SiloProfileInUse),
            "managed_profile_in_use"
        );
    }

    #[test]
    fn provider_reservation_blocks_launch_update_archive_and_delete_until_completion() {
        let control = LocalEnvironmentControl::default();
        let provider = control.reserve().expect("provider reservation");

        for blocked_operation in ["launch", "update", "archive", "delete"] {
            assert!(
                matches!(
                    control.reservation.try_lock(),
                    Err(TryLockError::WouldBlock)
                ),
                "{blocked_operation} must share the in-flight provider reservation"
            );
        }

        drop(provider);
        drop(
            control
                .reserve()
                .expect("lifecycle operation proceeds after provider completion"),
        );
    }
}
