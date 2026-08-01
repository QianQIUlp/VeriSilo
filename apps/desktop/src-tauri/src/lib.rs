use std::{
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};

use chrono::Utc;
use serde::Serialize;
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

fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn is_tray_primary_activation(button: MouseButton, button_state: MouseButtonState) -> bool {
    button == MouseButton::Left && button_state == MouseButtonState::Up
}

use domain::{
    app_data_root, discover_browsers as discover_installed_browsers, BrowserCandidate,
    BrowserVerification, CreateSiloInput, RuntimeActivation, Silo, SiloStorageUsage,
    UpdateSiloEngineInput, UpdateSiloInput, UpdateSiloNetworkInput, VaultLockState, VaultStatus,
};
use engine::{
    EngineAdapter, EngineAdapterId, EngineCapabilityId, EngineDescriptor, EngineHealth,
    EngineMaintenanceReceipt, EngineNegotiation, EnginePackageRequest,
    ExternalPackageEngineAdapter, StockChromiumAdapter, VaultSeedIdentityTokenDeriver,
};
use environment::backend::{EnvironmentActionReceipt, EnvironmentBackendStatus};
use environment::{EnvironmentManager, EnvironmentOperationRequest, WslStatus};
use launcher::{managed_profiles_are_quiescent_for_vault_restore, profile_in_use, RuntimeManager};
use mihomo::{MihomoControllerInput, MihomoSnapshot};
use runtime_watchdog::RuntimeWatchdog;
use vault::{RemoteVaultState, VaultBackupReceipt, VaultRuntime};

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

pub struct AppState {
    root: PathBuf,
    // Local stock/provider lifecycle operations first reserve local_control,
    // then acquire Vault → Runtime → Environments when each is needed. A
    // command may drop an inner guard but must never reacquire an earlier one.
    // The reservation stays held across slow provider/launcher completion.
    local_control: LocalEnvironmentControl,
    vault: Mutex<VaultRuntime>,
    runtime: Arc<Mutex<RuntimeManager>>,
    runtime_watchdog: RuntimeWatchdog,
    environments: Mutex<EnvironmentManager>,
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
        Self {
            root,
            local_control: LocalEnvironmentControl::default(),
            vault: Mutex::new(VaultRuntime::default()),
            runtime,
            runtime_watchdog,
            environments: Mutex::new(environments),
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
fn list_engine_adapters() -> Result<Vec<EngineAdapterStatus>, String> {
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
        let adapter = ExternalPackageEngineAdapter::production_prototype(id)
            .map_err(|error| error.to_string())?;
        statuses.push(summarize_engine(&adapter));
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
    adapter_id: EngineAdapterId,
    request: EnginePackageRequest,
) -> Result<EngineMaintenanceReceipt, String> {
    production_external_engine(adapter_id)?
        .install(&request)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn update_engine_package(
    adapter_id: EngineAdapterId,
    request: EnginePackageRequest,
) -> Result<EngineMaintenanceReceipt, String> {
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

fn execute_remote_interaction(
    state: &AppState,
    silo_id: Uuid,
    interaction: impl FnOnce(
        &mut ProductionRemoteBackend,
    ) -> Result<
        RemoteInteractionReceipt,
        verisilo_remote_backend::RemoteBackendError,
    >,
) -> Result<RemoteInteractionReceipt, String> {
    let _remote_guard = state
        .remote_control
        .lock()
        .map_err(|_| "VeriSilo remote control state is unavailable.".to_owned())?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    // Resolve the local Silo and keep the Vault locked through the bounded
    // HTTPS exchange. No authorization or bearer credential escapes this
    // native command boundary.
    vault.get_silo(silo_id).map_err(|error| error.to_string())?;
    let mut remote = vault
        .remote_control_plane()
        .map_err(|error| error.to_string())?;
    let mut backend = production_remote_backend(&remote)?;
    let result = interaction(&mut backend);
    remote.backend = backend.export_snapshot();
    // Persist consumed client sequence numbers and any accepted authorization,
    // channel metadata or interaction receipt even if a later local policy
    // check fails closed.
    vault
        .persist_remote_control_plane(&state.root, remote)
        .map_err(|error| error.to_string())?;
    result.map_err(|error| error.to_string())
}

#[tauri::command]
fn remote_environment_create(
    state: State<'_, AppState>,
    silo_id: Uuid,
    network: RemoteNetworkPolicy,
    ttl_seconds: u64,
    cost_acknowledged: bool,
) -> Result<RemoteOperationResult, String> {
    execute_remote_operation(&state, silo_id, |backend| {
        backend.create(silo_id, network, ttl_seconds, cost_acknowledged)
    })
}

#[tauri::command]
fn remote_environment_start(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    execute_remote_operation(&state, silo_id, |backend| backend.start(silo_id))
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
    execute_remote_operation(&state, silo_id, |backend| backend.pause(silo_id))
}

#[tauri::command]
fn remote_environment_snapshot(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    execute_remote_operation(&state, silo_id, |backend| backend.snapshot(silo_id))
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
    execute_remote_operation(&state, silo_id, |backend| {
        backend.configure_network(silo_id, network)
    })
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
    execute_remote_interaction(&state, silo_id, |backend| {
        backend.open_human_session(silo_id, lifetime_seconds)
    })
}

#[tauri::command]
fn remote_environment_close_human_session(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RemoteInteractionReceipt, String> {
    execute_remote_interaction(&state, silo_id, |backend| {
        backend.close_human_session(silo_id)
    })
}

#[tauri::command]
fn remote_environment_grant_automation(
    state: State<'_, AppState>,
    silo_id: Uuid,
    lifetime_seconds: u64,
    scopes: Vec<RemoteAutomationScope>,
    approved_by_user: bool,
) -> Result<RemoteInteractionReceipt, String> {
    execute_remote_interaction(&state, silo_id, |backend| {
        backend.grant_automation(silo_id, lifetime_seconds, scopes, approved_by_user)
    })
}

#[tauri::command]
fn remote_environment_revoke_automation(
    state: State<'_, AppState>,
    silo_id: Uuid,
    authorization_id: Uuid,
) -> Result<RemoteInteractionReceipt, String> {
    execute_remote_interaction(&state, silo_id, |backend| {
        backend.revoke_automation(silo_id, authorization_id)
    })
}

#[tauri::command]
fn remote_environment_open_screen(
    state: State<'_, AppState>,
    silo_id: Uuid,
    principal: InteractivePrincipal,
) -> Result<RemoteInteractionReceipt, String> {
    execute_remote_interaction(&state, silo_id, |backend| {
        backend.open_screen(silo_id, principal)
    })
}

#[tauri::command]
fn remote_environment_send_input(
    state: State<'_, AppState>,
    silo_id: Uuid,
    principal: InteractivePrincipal,
    events: Vec<RemoteInputEvent>,
) -> Result<RemoteInteractionReceipt, String> {
    execute_remote_interaction(&state, silo_id, |backend| {
        backend.send_input(silo_id, principal, events)
    })
}

#[tauri::command]
fn desktop_status(state: State<'_, AppState>) -> Result<DesktopStatus, String> {
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
    drop(runtime);
    drop(vault);
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
    let _remote_guard = state
        .remote_control
        .lock()
        .map_err(|_| "VeriSilo remote control state is unavailable.".to_owned())?;
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
    // Restore is a global ownership transition. Reserve remote and local
    // lifecycle planes before Vault → Runtime → Environments so all in-flight
    // provider work finishes and no new work can race the replacement.
    let _remote_guard = state
        .remote_control
        .lock()
        .map_err(|_| "VeriSilo remote control state is unavailable.".to_owned())?;
    let _local_reservation = state.local_control.reserve()?;
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
    let mut environments = state
        .environments
        .lock()
        .map_err(|_| "VeriSilo environment provider state is unavailable.".to_owned())?;
    environments
        .select_wsl_distribution(distribution)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn environment_backend_execute(
    state: State<'_, AppState>,
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
    vault
        .get_silo(environment_id)
        .map_err(|error| error.to_string())?;
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
    drop(runtime);
    drop(vault);
    let mut environments = state
        .environments
        .lock()
        .map_err(|_| "VeriSilo environment provider state is unavailable.".to_owned())?;
    environments
        .execute(request)
        .map_err(|error| error.to_string())
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

#[tauri::command]
fn create_silo(state: State<'_, AppState>, input: CreateSiloInput) -> Result<Silo, String> {
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
    let is_active = runtime.is_active(silo_id);
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
    let is_active = runtime.is_active(silo_id);
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
    let is_active = runtime.is_active(silo_id);
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
    let is_active = runtime.is_active(silo_id);
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
    let is_active = runtime.is_active(silo_id);
    vault
        .update_silo_engine(&state.root, silo_id, input, is_active)
        .map_err(|error| error.to_string())
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
    let is_active = runtime.is_active(silo_id);
    let environments = state
        .environments
        .lock()
        .map_err(|_| "VeriSilo environment provider state is unavailable.".to_owned())?;
    environments
        .ensure_no_local_environment_artifacts(silo_id)
        .map_err(|error| error.to_string())?;
    drop(environments);
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
    let _local_reservation = state.local_control.reserve()?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let is_active = runtime.is_active(silo_id);
    let environments = state
        .environments
        .lock()
        .map_err(|_| "VeriSilo environment provider state is unavailable.".to_owned())?;
    environments
        .ensure_no_local_environment_artifacts(silo_id)
        .map_err(|error| error.to_string())?;
    drop(environments);
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
    let is_active = runtime.is_active(silo_id);
    vault
        .recheck_silo_browser(&state.root, silo_id, is_active)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn recheck_silo_runtime(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RuntimeActivation, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    let silo = vault.get_silo(silo_id).map_err(|error| error.to_string())?;
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
        .recheck_active(
            &silo,
            proxy_authentication.as_ref(),
            mihomo_authentication.as_ref(),
        )
        .map_err(|error| error.to_string())?;
    publish_runtime_status(&state, &activation, &vault_status);
    Ok(activation)
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
    let managed_profile_directories = vault
        .managed_profile_directories()
        .map_err(|error| error.to_string())?;
    let proxy_authentication = vault
        .proxy_authentication_for_silo(silo_id)
        .map_err(|error| error.to_string())?;
    let mihomo_authentication = vault
        .mihomo_controller_authentication_for_silo(silo_id)
        .map_err(|error| error.to_string())?;
    let identity_seed = (!silo.engine.is_stock())
        .then(|| vault.identity_seed_for_silo(silo_id))
        .transpose()
        .map_err(|error| error.to_string())?;
    let identity_deriver = identity_seed
        .as_ref()
        .map(|seed| VaultSeedIdentityTokenDeriver::new(seed.as_ref()))
        .transpose()
        .map_err(|error| error.to_string())?;
    let vault_status = vault.status(&state.root);
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    // Runtime is now reserved, so no edit command can pass its active check
    // between reading Vault metadata and starting this exact configuration.
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
            publish_runtime_status(&state, &activation, &vault_status);
            Ok(activation)
        }
        Err(error) => {
            let activation = runtime.activation();
            publish_runtime_status(&state, &activation, &vault_status);
            Err(error.to_string())
        }
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
                    TRAY_EXIT_ID => app.exit(0),
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
            inspect_mihomo_controller,
            list_silos,
            list_active_silos,
            list_archived_silos,
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
            rebind_silo_mihomo,
            launch_silo,
        ])
        .run(tauri::generate_context!())
        .expect("error while running VeriSilo");
}

#[cfg(test)]
mod local_lifecycle_tests {
    use std::sync::TryLockError;

    use super::{is_tray_primary_activation, LocalEnvironmentControl};
    use tauri::tray::{MouseButton, MouseButtonState};

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
