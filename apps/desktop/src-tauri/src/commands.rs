use crate::application::{
    DesktopStatus, EngineAdapterStatus, LegacyEnvironmentArtifact, RemoteEnvironmentStatus,
};
use crate::domain::{
    BrowserCandidate, BrowserVerification, CreateManagedSiloInput, CreateSiloInput,
    ManagedIdentityPreview, RuntimeActivation, Silo, SiloStorageUsage, SiloStorageUsageSummary,
    UpdateManagedIdentityInput, UpdateSiloEngineInput, UpdateSiloInput, UpdateSiloNetworkInput,
    VaultStatus,
};
use crate::engine::{EngineAdapterId, EngineMaintenanceReceipt, EnginePackageRequest};
use crate::environment::backend::{
    EnvironmentActionReceipt, EnvironmentBackendId, EnvironmentBackendStatus,
};
use crate::environment::{EnvironmentOperationRequest, WslStatus};
use crate::mihomo::{LocalClashProbe, MihomoControllerInput, MihomoSnapshot};
use crate::native_host;
use crate::vault::VaultBackupReceipt;
use crate::{application, local_api, AppState};
use tauri::{AppHandle, Manager};
use uuid::Uuid;
use verisilo_remote_backend::agent::{
    AutomationScope as RemoteAutomationScope, InputEvent as RemoteInputEvent,
};
use verisilo_remote_backend::{
    AgentInteractionReceipt as RemoteInteractionReceipt, InteractivePrincipal,
    OperationResult as RemoteOperationResult, PairingApproval, RemoteEndpoint, RemoteNetworkPolicy,
};

async fn run_blocking<T: Send + 'static>(
    app: AppHandle,
    operation: impl FnOnce(&application::DesktopCore) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        operation(&state.core)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn list_engine_adapters(
    app: AppHandle,
) -> Result<Vec<EngineAdapterStatus>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::list_engine_adapters(&state.core)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn install_engine_package(
    app: AppHandle,
    adapter_id: EngineAdapterId,
    request: EnginePackageRequest,
) -> Result<EngineMaintenanceReceipt, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::install_engine_package(&state.core, adapter_id, request)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn update_engine_package(
    app: AppHandle,
    adapter_id: EngineAdapterId,
    request: EnginePackageRequest,
) -> Result<EngineMaintenanceReceipt, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::update_engine_package(&state.core, adapter_id, request)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn rollback_engine_package(
    adapter_id: EngineAdapterId,
) -> Result<EngineMaintenanceReceipt, String> {
    tauri::async_runtime::spawn_blocking(move || application::rollback_engine_package(adapter_id))
        .await
        .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn set_engine_emergency_disabled(
    adapter_id: EngineAdapterId,
    disabled: bool,
    reason: Option<String>,
) -> Result<EngineAdapterStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        application::set_engine_emergency_disabled(adapter_id, disabled, reason)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn remote_environment_status(
    app: AppHandle,
) -> Result<RemoteEnvironmentStatus, String> {
    run_blocking(app, move |core| {
        application::remote_environment_status(core)
    })
    .await
}

#[tauri::command]
pub(crate) fn validate_remote_environment_endpoint(
    endpoint: RemoteEndpoint,
) -> Result<RemoteEndpoint, String> {
    application::validate_remote_environment_endpoint(endpoint)
}

#[tauri::command]
pub(crate) async fn pair_remote_environment(
    app: AppHandle,
    endpoint: RemoteEndpoint,
    approval: PairingApproval,
) -> Result<RemoteEnvironmentStatus, String> {
    run_blocking(app, move |core| {
        application::pair_remote_environment(core, endpoint, approval)
    })
    .await
}

#[tauri::command]
pub(crate) async fn rotate_remote_environment_tls_pin(
    app: AppHandle,
    endpoint: RemoteEndpoint,
    approval: PairingApproval,
    confirm_rotation: bool,
) -> Result<RemoteEnvironmentStatus, String> {
    run_blocking(app, move |core| {
        application::rotate_remote_environment_tls_pin(core, endpoint, approval, confirm_rotation)
    })
    .await
}

#[tauri::command]
pub(crate) async fn revoke_remote_pairing(
    app: AppHandle,
    confirm_revoke: bool,
) -> Result<RemoteEnvironmentStatus, String> {
    run_blocking(app, move |core| {
        application::revoke_remote_pairing(core, confirm_revoke)
    })
    .await
}

#[tauri::command]
pub(crate) async fn force_detach_remote_environment(
    app: AppHandle,
    silo_id: Uuid,
    confirm_local_detach: bool,
    acknowledge_remote_orphan_risk: bool,
) -> Result<RemoteEnvironmentStatus, String> {
    run_blocking(app, move |core| {
        application::force_detach_remote_environment(
            core,
            silo_id,
            confirm_local_detach,
            acknowledge_remote_orphan_risk,
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn remote_environment_create(
    app: AppHandle,
    silo_id: Uuid,
    network: RemoteNetworkPolicy,
    ttl_seconds: u64,
    cost_acknowledged: bool,
) -> Result<RemoteOperationResult, String> {
    run_blocking(app, move |core| {
        application::remote_environment_create(
            core,
            silo_id,
            network,
            ttl_seconds,
            cost_acknowledged,
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn remote_environment_start(
    app: AppHandle,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    run_blocking(app, move |core| {
        application::remote_environment_start(core, silo_id)
    })
    .await
}

#[tauri::command]
pub(crate) async fn remote_environment_stop(
    app: AppHandle,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    run_blocking(app, move |core| {
        application::remote_environment_stop(core, silo_id)
    })
    .await
}

#[tauri::command]
pub(crate) async fn remote_environment_pause(
    app: AppHandle,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    run_blocking(app, move |core| {
        application::remote_environment_pause(core, silo_id)
    })
    .await
}

#[tauri::command]
pub(crate) async fn remote_environment_snapshot(
    app: AppHandle,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    run_blocking(app, move |core| {
        application::remote_environment_snapshot(core, silo_id)
    })
    .await
}

#[tauri::command]
pub(crate) async fn remote_environment_destroy(
    app: AppHandle,
    silo_id: Uuid,
    confirm_destroy: bool,
) -> Result<RemoteOperationResult, String> {
    run_blocking(app, move |core| {
        application::remote_environment_destroy(core, silo_id, confirm_destroy)
    })
    .await
}

#[tauri::command]
pub(crate) async fn remote_environment_configure_network(
    app: AppHandle,
    silo_id: Uuid,
    network: RemoteNetworkPolicy,
) -> Result<RemoteOperationResult, String> {
    run_blocking(app, move |core| {
        application::remote_environment_configure_network(core, silo_id, network)
    })
    .await
}

#[tauri::command]
pub(crate) async fn remote_environment_health(
    app: AppHandle,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    run_blocking(app, move |core| {
        application::remote_environment_health(core, silo_id)
    })
    .await
}

#[tauri::command]
pub(crate) async fn remote_environment_logs(
    app: AppHandle,
    silo_id: Uuid,
    cursor: Option<Uuid>,
    limit: u16,
) -> Result<RemoteOperationResult, String> {
    run_blocking(app, move |core| {
        application::remote_environment_logs(core, silo_id, cursor, limit)
    })
    .await
}

#[tauri::command]
pub(crate) async fn remote_environment_open_human_session(
    app: AppHandle,
    silo_id: Uuid,
    lifetime_seconds: u64,
) -> Result<RemoteInteractionReceipt, String> {
    run_blocking(app, move |core| {
        application::remote_environment_open_human_session(core, silo_id, lifetime_seconds)
    })
    .await
}

#[tauri::command]
pub(crate) async fn remote_environment_close_human_session(
    app: AppHandle,
    silo_id: Uuid,
) -> Result<RemoteInteractionReceipt, String> {
    run_blocking(app, move |core| {
        application::remote_environment_close_human_session(core, silo_id)
    })
    .await
}

#[tauri::command]
pub(crate) async fn remote_environment_grant_automation(
    app: AppHandle,
    silo_id: Uuid,
    lifetime_seconds: u64,
    scopes: Vec<RemoteAutomationScope>,
    approved_by_user: bool,
) -> Result<RemoteInteractionReceipt, String> {
    run_blocking(app, move |core| {
        application::remote_environment_grant_automation(
            core,
            silo_id,
            lifetime_seconds,
            scopes,
            approved_by_user,
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn remote_environment_revoke_automation(
    app: AppHandle,
    silo_id: Uuid,
    authorization_id: Uuid,
) -> Result<RemoteInteractionReceipt, String> {
    run_blocking(app, move |core| {
        application::remote_environment_revoke_automation(core, silo_id, authorization_id)
    })
    .await
}

#[tauri::command]
pub(crate) async fn remote_environment_open_screen(
    app: AppHandle,
    silo_id: Uuid,
    principal: InteractivePrincipal,
) -> Result<RemoteInteractionReceipt, String> {
    run_blocking(app, move |core| {
        application::remote_environment_open_screen(core, silo_id, principal)
    })
    .await
}

#[tauri::command]
pub(crate) async fn remote_environment_send_input(
    app: AppHandle,
    silo_id: Uuid,
    principal: InteractivePrincipal,
    events: Vec<RemoteInputEvent>,
) -> Result<RemoteInteractionReceipt, String> {
    run_blocking(app, move |core| {
        application::remote_environment_send_input(core, silo_id, principal, events)
    })
    .await
}

// Heavy commands run their body on the blocking thread pool via
// `tauri::async_runtime::spawn_blocking` (the launch_silo/stop_silo pattern):
// Tauri 2 executes non-async commands inline on the main/UI thread, and these
// bodies take process spawns, Argon2 derives, filesystem walks, or global
// lifecycle locks.
#[tauri::command]
pub(crate) async fn desktop_status(app: AppHandle) -> Result<DesktopStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::desktop_status(&state.core)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn initialize_vault(
    app: AppHandle,
    passphrase: String,
) -> Result<VaultStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::initialize_vault(&state.core, passphrase)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn unlock_vault(
    app: AppHandle,
    passphrase: String,
) -> Result<VaultStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::unlock_vault(&state.core, passphrase)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn lock_vault(app: AppHandle) -> Result<VaultStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::lock_vault(&state.core)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn change_vault_passphrase(
    app: AppHandle,
    current_passphrase: String,
    new_passphrase: String,
) -> Result<VaultStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::change_vault_passphrase(&state.core, current_passphrase, new_passphrase)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn backup_vault(
    app: AppHandle,
    destination_path: String,
) -> Result<VaultBackupReceipt, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::backup_vault(&state.core, destination_path)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn restore_vault(
    app: AppHandle,
    source_path: String,
    passphrase: String,
    confirm_overwrite: bool,
) -> Result<VaultStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::restore_vault(&state.core, source_path, passphrase, confirm_overwrite)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn discover_browsers() -> Vec<BrowserCandidate> {
    tauri::async_runtime::spawn_blocking(application::discover_browsers)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub(crate) async fn detect_wsl() -> WslStatus {
    // A detection panic degrades to the unavailable status instead of taking
    // down the blocking worker; the serialized shape is unchanged.
    tauri::async_runtime::spawn_blocking(application::detect_wsl)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub(crate) async fn environment_backend_statuses(
    app: AppHandle,
) -> Result<Vec<EnvironmentBackendStatus>, String> {
    run_blocking(app, move |core| {
        application::environment_backend_statuses(core)
    })
    .await
}

#[tauri::command]
pub(crate) async fn select_wsl_environment_distribution(
    app: AppHandle,
    distribution: String,
) -> Result<EnvironmentBackendStatus, String> {
    run_blocking(app, move |core| {
        application::select_wsl_environment_distribution(core, distribution)
    })
    .await
}

#[tauri::command]
pub(crate) async fn environment_backend_execute(
    app: AppHandle,
    request: EnvironmentOperationRequest,
) -> Result<EnvironmentActionReceipt, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::environment_backend_execute(&state.core, request)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn list_legacy_environment_artifacts(
    app: AppHandle,
) -> Result<Vec<LegacyEnvironmentArtifact>, String> {
    run_blocking(app, move |core| {
        application::list_legacy_environment_artifacts(core)
    })
    .await
}

#[tauri::command]
pub(crate) async fn cleanup_legacy_environment_artifact(
    app: AppHandle,
    silo_id: Uuid,
    backend: EnvironmentBackendId,
    confirm_cleanup: bool,
) -> Result<EnvironmentActionReceipt, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::cleanup_legacy_environment_artifact(
            &state.core,
            silo_id,
            backend,
            confirm_cleanup,
        )
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn inspect_mihomo_controller(
    input: MihomoControllerInput,
) -> Result<MihomoSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || application::inspect_mihomo_controller(input))
        .await
        .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn probe_local_clash(secret: Option<String>) -> Result<LocalClashProbe, String> {
    tauri::async_runtime::spawn_blocking(move || application::probe_local_clash(secret))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn list_silos(app: AppHandle) -> Result<Vec<Silo>, String> {
    run_blocking(app, application::list_silos).await
}

#[tauri::command]
pub(crate) async fn local_api_info(app: AppHandle) -> Result<local_api::LocalApiInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let slot = state
            .local_api
            .lock()
            .map_err(|_| "本机 API 状态不可用。".to_owned())?;
        let (server, url) = slot
            .as_ref()
            .ok_or_else(|| "本机 API 没有启动。".to_owned())?;
        Ok(server.info(url.clone()))
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn list_active_silos(app: AppHandle) -> Result<Vec<Silo>, String> {
    run_blocking(app, application::list_active_silos).await
}

#[tauri::command]
pub(crate) async fn list_archived_silos(app: AppHandle) -> Result<Vec<Silo>, String> {
    run_blocking(app, application::list_archived_silos).await
}

#[tauri::command]
pub(crate) async fn create_managed_silo(
    app: AppHandle,
    input: CreateManagedSiloInput,
) -> Result<Silo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::create_managed_silo(&state.core, input)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn list_managed_identity_previews(
    app: AppHandle,
) -> Result<std::collections::HashMap<Uuid, ManagedIdentityPreview>, String> {
    run_blocking(app, move |core| {
        application::list_managed_identity_previews(core)
    })
    .await
}

#[tauri::command]
pub(crate) async fn update_managed_identity(
    app: AppHandle,
    silo_id: Uuid,
    input: UpdateManagedIdentityInput,
) -> Result<Silo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::update_managed_identity(&state.core, silo_id, input)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn create_silo(app: AppHandle, input: CreateSiloInput) -> Result<Silo, String> {
    run_blocking(app, move |core| application::create_silo(core, input)).await
}

#[tauri::command]
pub(crate) async fn update_silo(
    app: AppHandle,
    silo_id: Uuid,
    input: UpdateSiloInput,
) -> Result<Silo, String> {
    run_blocking(app, move |core| {
        application::update_silo(core, silo_id, input)
    })
    .await
}

#[tauri::command]
pub(crate) async fn update_silo_configuration(
    app: AppHandle,
    silo_id: Uuid,
    input: UpdateSiloInput,
    network_input: Option<UpdateSiloNetworkInput>,
    engine_input: Option<UpdateSiloEngineInput>,
) -> Result<Silo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::update_silo_configuration(
            &state.core,
            silo_id,
            input,
            network_input,
            engine_input,
        )
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn rename_silo(
    app: AppHandle,
    silo_id: Uuid,
    name: String,
) -> Result<Silo, String> {
    run_blocking(app, move |core| {
        application::rename_silo(core, silo_id, name)
    })
    .await
}

#[tauri::command]
pub(crate) async fn update_silo_network(
    app: AppHandle,
    silo_id: Uuid,
    input: UpdateSiloNetworkInput,
) -> Result<Silo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::update_silo_network(&state.core, silo_id, input)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn update_silo_engine(
    app: AppHandle,
    silo_id: Uuid,
    input: UpdateSiloEngineInput,
) -> Result<Silo, String> {
    run_blocking(app, move |core| {
        application::update_silo_engine(core, silo_id, input)
    })
    .await
}

#[tauri::command]
pub(crate) async fn archive_silo(app: AppHandle, silo_id: Uuid) -> Result<(), String> {
    run_blocking(app, move |core| application::archive_silo(core, silo_id)).await
}

#[tauri::command]
pub(crate) async fn restore_archived_silo(app: AppHandle, silo_id: Uuid) -> Result<Silo, String> {
    run_blocking(app, move |core| {
        application::restore_archived_silo(core, silo_id)
    })
    .await
}

#[tauri::command]
pub(crate) async fn delete_silo(
    app: AppHandle,
    silo_id: Uuid,
    confirm_permanent: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::delete_silo(&state.core, silo_id, confirm_permanent)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn silo_storage_usage(
    app: AppHandle,
    silo_id: Uuid,
) -> Result<SiloStorageUsage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::silo_storage_usage(&state.core, silo_id)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn silo_storage_usages(
    app: AppHandle,
) -> Result<Vec<SiloStorageUsageSummary>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::silo_storage_usages(&state.core)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn list_network_evidence(
    app: AppHandle,
    silo_id: Option<Uuid>,
) -> Result<Vec<native_host::NativeNetworkEvidenceInboxEntry>, String> {
    run_blocking(app, move |core| {
        application::list_network_evidence(core, silo_id)
    })
    .await
}

#[tauri::command]
pub(crate) async fn clear_network_evidence(
    app: AppHandle,
    silo_id: Uuid,
    confirm_clear: bool,
) -> Result<usize, String> {
    run_blocking(app, move |core| {
        application::clear_network_evidence(core, silo_id, confirm_clear)
    })
    .await
}

#[tauri::command]
pub(crate) async fn recheck_silo_browser(
    app: AppHandle,
    silo_id: Uuid,
) -> Result<BrowserVerification, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::recheck_silo_browser(&state.core, silo_id)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn recheck_silo_runtime(
    app: AppHandle,
    silo_id: Uuid,
) -> Result<RuntimeActivation, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::recheck_silo_runtime(&state.core, silo_id)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn stop_silo(app: AppHandle, silo_id: Uuid) -> Result<RuntimeActivation, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::stop_silo_with(&state.core, silo_id)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn rebind_silo_mihomo(
    app: AppHandle,
    silo_id: Uuid,
) -> Result<RuntimeActivation, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::rebind_silo_mihomo(&state.core, silo_id)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) async fn launch_silo(
    app: AppHandle,
    silo_id: Uuid,
) -> Result<RuntimeActivation, application::LaunchFailure> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::launch_silo_with(&state.core, silo_id)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string().into()))
}
