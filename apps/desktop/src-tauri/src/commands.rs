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
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;
use verisilo_remote_backend::agent::{
    AutomationScope as RemoteAutomationScope, InputEvent as RemoteInputEvent,
};
use verisilo_remote_backend::{
    AgentInteractionReceipt as RemoteInteractionReceipt, InteractivePrincipal,
    OperationResult as RemoteOperationResult, PairingApproval, RemoteEndpoint, RemoteNetworkPolicy,
};

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
pub(crate) fn set_engine_emergency_disabled(
    adapter_id: EngineAdapterId,
    disabled: bool,
    reason: Option<String>,
) -> Result<EngineAdapterStatus, String> {
    application::set_engine_emergency_disabled(adapter_id, disabled, reason)
}

#[tauri::command]
pub(crate) fn remote_environment_status(
    state: State<'_, AppState>,
) -> Result<RemoteEnvironmentStatus, String> {
    application::remote_environment_status(&state.core)
}

#[tauri::command]
pub(crate) fn validate_remote_environment_endpoint(
    endpoint: RemoteEndpoint,
) -> Result<RemoteEndpoint, String> {
    application::validate_remote_environment_endpoint(endpoint)
}

#[tauri::command]
pub(crate) fn pair_remote_environment(
    state: State<'_, AppState>,
    endpoint: RemoteEndpoint,
    approval: PairingApproval,
) -> Result<RemoteEnvironmentStatus, String> {
    application::pair_remote_environment(&state.core, endpoint, approval)
}

#[tauri::command]
pub(crate) fn rotate_remote_environment_tls_pin(
    state: State<'_, AppState>,
    endpoint: RemoteEndpoint,
    approval: PairingApproval,
    confirm_rotation: bool,
) -> Result<RemoteEnvironmentStatus, String> {
    application::rotate_remote_environment_tls_pin(
        &state.core,
        endpoint,
        approval,
        confirm_rotation,
    )
}

#[tauri::command]
pub(crate) fn revoke_remote_pairing(
    state: State<'_, AppState>,
    confirm_revoke: bool,
) -> Result<RemoteEnvironmentStatus, String> {
    application::revoke_remote_pairing(&state.core, confirm_revoke)
}

#[tauri::command]
pub(crate) fn force_detach_remote_environment(
    state: State<'_, AppState>,
    silo_id: Uuid,
    confirm_local_detach: bool,
    acknowledge_remote_orphan_risk: bool,
) -> Result<RemoteEnvironmentStatus, String> {
    application::force_detach_remote_environment(
        &state.core,
        silo_id,
        confirm_local_detach,
        acknowledge_remote_orphan_risk,
    )
}

#[tauri::command]
pub(crate) fn remote_environment_create(
    state: State<'_, AppState>,
    silo_id: Uuid,
    network: RemoteNetworkPolicy,
    ttl_seconds: u64,
    cost_acknowledged: bool,
) -> Result<RemoteOperationResult, String> {
    application::remote_environment_create(
        &state.core,
        silo_id,
        network,
        ttl_seconds,
        cost_acknowledged,
    )
}

#[tauri::command]
pub(crate) fn remote_environment_start(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    application::remote_environment_start(&state.core, silo_id)
}

#[tauri::command]
pub(crate) fn remote_environment_stop(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    application::remote_environment_stop(&state.core, silo_id)
}

#[tauri::command]
pub(crate) fn remote_environment_pause(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    application::remote_environment_pause(&state.core, silo_id)
}

#[tauri::command]
pub(crate) fn remote_environment_snapshot(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    application::remote_environment_snapshot(&state.core, silo_id)
}

#[tauri::command]
pub(crate) fn remote_environment_destroy(
    state: State<'_, AppState>,
    silo_id: Uuid,
    confirm_destroy: bool,
) -> Result<RemoteOperationResult, String> {
    application::remote_environment_destroy(&state.core, silo_id, confirm_destroy)
}

#[tauri::command]
pub(crate) fn remote_environment_configure_network(
    state: State<'_, AppState>,
    silo_id: Uuid,
    network: RemoteNetworkPolicy,
) -> Result<RemoteOperationResult, String> {
    application::remote_environment_configure_network(&state.core, silo_id, network)
}

#[tauri::command]
pub(crate) fn remote_environment_health(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    application::remote_environment_health(&state.core, silo_id)
}

#[tauri::command]
pub(crate) fn remote_environment_logs(
    state: State<'_, AppState>,
    silo_id: Uuid,
    cursor: Option<Uuid>,
    limit: u16,
) -> Result<RemoteOperationResult, String> {
    application::remote_environment_logs(&state.core, silo_id, cursor, limit)
}

#[tauri::command]
pub(crate) fn remote_environment_open_human_session(
    state: State<'_, AppState>,
    silo_id: Uuid,
    lifetime_seconds: u64,
) -> Result<RemoteInteractionReceipt, String> {
    application::remote_environment_open_human_session(&state.core, silo_id, lifetime_seconds)
}

#[tauri::command]
pub(crate) fn remote_environment_close_human_session(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RemoteInteractionReceipt, String> {
    application::remote_environment_close_human_session(&state.core, silo_id)
}

#[tauri::command]
pub(crate) fn remote_environment_grant_automation(
    state: State<'_, AppState>,
    silo_id: Uuid,
    lifetime_seconds: u64,
    scopes: Vec<RemoteAutomationScope>,
    approved_by_user: bool,
) -> Result<RemoteInteractionReceipt, String> {
    application::remote_environment_grant_automation(
        &state.core,
        silo_id,
        lifetime_seconds,
        scopes,
        approved_by_user,
    )
}

#[tauri::command]
pub(crate) fn remote_environment_revoke_automation(
    state: State<'_, AppState>,
    silo_id: Uuid,
    authorization_id: Uuid,
) -> Result<RemoteInteractionReceipt, String> {
    application::remote_environment_revoke_automation(&state.core, silo_id, authorization_id)
}

#[tauri::command]
pub(crate) fn remote_environment_open_screen(
    state: State<'_, AppState>,
    silo_id: Uuid,
    principal: InteractivePrincipal,
) -> Result<RemoteInteractionReceipt, String> {
    application::remote_environment_open_screen(&state.core, silo_id, principal)
}

#[tauri::command]
pub(crate) fn remote_environment_send_input(
    state: State<'_, AppState>,
    silo_id: Uuid,
    principal: InteractivePrincipal,
    events: Vec<RemoteInputEvent>,
) -> Result<RemoteInteractionReceipt, String> {
    application::remote_environment_send_input(&state.core, silo_id, principal, events)
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
pub(crate) fn environment_backend_statuses(
    state: State<'_, AppState>,
) -> Result<Vec<EnvironmentBackendStatus>, String> {
    application::environment_backend_statuses(&state.core)
}

#[tauri::command]
pub(crate) fn select_wsl_environment_distribution(
    state: State<'_, AppState>,
    distribution: String,
) -> Result<EnvironmentBackendStatus, String> {
    application::select_wsl_environment_distribution(&state.core, distribution)
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
pub(crate) fn list_legacy_environment_artifacts(
    state: State<'_, AppState>,
) -> Result<Vec<LegacyEnvironmentArtifact>, String> {
    application::list_legacy_environment_artifacts(&state.core)
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
pub(crate) fn list_silos(state: State<'_, AppState>) -> Result<Vec<Silo>, String> {
    application::list_silos(&state.core)
}

#[tauri::command]
pub(crate) fn local_api_info(
    state: State<'_, AppState>,
) -> Result<local_api::LocalApiInfo, String> {
    let slot = state
        .local_api
        .lock()
        .map_err(|_| "本机 API 状态不可用。".to_owned())?;
    let (server, url) = slot
        .as_ref()
        .ok_or_else(|| "本机 API 没有启动。".to_owned())?;
    Ok(server.info(url.clone()))
}

#[tauri::command]
pub(crate) fn list_active_silos(state: State<'_, AppState>) -> Result<Vec<Silo>, String> {
    application::list_active_silos(&state.core)
}

#[tauri::command]
pub(crate) fn list_archived_silos(state: State<'_, AppState>) -> Result<Vec<Silo>, String> {
    application::list_archived_silos(&state.core)
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
pub(crate) fn list_managed_identity_previews(
    state: State<'_, AppState>,
) -> Result<std::collections::HashMap<Uuid, ManagedIdentityPreview>, String> {
    application::list_managed_identity_previews(&state.core)
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
pub(crate) fn create_silo(
    state: State<'_, AppState>,
    input: CreateSiloInput,
) -> Result<Silo, String> {
    application::create_silo(&state.core, input)
}

#[tauri::command]
pub(crate) fn update_silo(
    state: State<'_, AppState>,
    silo_id: Uuid,
    input: UpdateSiloInput,
) -> Result<Silo, String> {
    application::update_silo(&state.core, silo_id, input)
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
        application::update_silo_configuration(&state.core, silo_id, input, network_input, engine_input)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}

#[tauri::command]
pub(crate) fn rename_silo(
    state: State<'_, AppState>,
    silo_id: Uuid,
    name: String,
) -> Result<Silo, String> {
    application::rename_silo(&state.core, silo_id, name)
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
pub(crate) fn update_silo_engine(
    state: State<'_, AppState>,
    silo_id: Uuid,
    input: UpdateSiloEngineInput,
) -> Result<Silo, String> {
    application::update_silo_engine(&state.core, silo_id, input)
}

#[tauri::command]
pub(crate) fn archive_silo(state: State<'_, AppState>, silo_id: Uuid) -> Result<(), String> {
    application::archive_silo(&state.core, silo_id)
}

#[tauri::command]
pub(crate) fn restore_archived_silo(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<Silo, String> {
    application::restore_archived_silo(&state.core, silo_id)
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
pub(crate) fn list_network_evidence(
    state: State<'_, AppState>,
    silo_id: Option<Uuid>,
) -> Result<Vec<native_host::NativeNetworkEvidenceInboxEntry>, String> {
    application::list_network_evidence(&state.core, silo_id)
}

#[tauri::command]
pub(crate) fn clear_network_evidence(
    state: State<'_, AppState>,
    silo_id: Uuid,
    confirm_clear: bool,
) -> Result<usize, String> {
    application::clear_network_evidence(&state.core, silo_id, confirm_clear)
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
) -> Result<RuntimeActivation, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        application::launch_silo_with(&state.core, silo_id)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()))
}
