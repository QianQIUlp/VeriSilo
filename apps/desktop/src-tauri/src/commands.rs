use crate::application::{
    DesktopStatus, EngineAdapterStatus, LegacyEnvironmentArtifact, RemoteEnvironmentStatus,
};
use crate::domain::{
    BrowserCandidate, BrowserVerification, CreateManagedSiloInput, CreateSiloInput,
    ManagedIdentityPreview, RuntimeActivation, Silo, SiloStorageUsage, UpdateManagedIdentityInput,
    UpdateSiloEngineInput, UpdateSiloInput, UpdateSiloNetworkInput, VaultStatus,
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
pub(crate) fn list_engine_adapters(
    state: State<'_, AppState>,
) -> Result<Vec<EngineAdapterStatus>, String> {
    application::list_engine_adapters(&state.core)
}

#[tauri::command]
pub(crate) fn install_engine_package(
    state: State<'_, AppState>,
    adapter_id: EngineAdapterId,
    request: EnginePackageRequest,
) -> Result<EngineMaintenanceReceipt, String> {
    application::install_engine_package(&state.core, adapter_id, request)
}

#[tauri::command]
pub(crate) fn update_engine_package(
    state: State<'_, AppState>,
    adapter_id: EngineAdapterId,
    request: EnginePackageRequest,
) -> Result<EngineMaintenanceReceipt, String> {
    application::update_engine_package(&state.core, adapter_id, request)
}

#[tauri::command]
pub(crate) fn rollback_engine_package(
    adapter_id: EngineAdapterId,
) -> Result<EngineMaintenanceReceipt, String> {
    application::rollback_engine_package(adapter_id)
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

#[tauri::command]
pub(crate) fn desktop_status(state: State<'_, AppState>) -> Result<DesktopStatus, String> {
    application::desktop_status(&state.core)
}

#[tauri::command]
pub(crate) fn initialize_vault(
    state: State<'_, AppState>,
    passphrase: String,
) -> Result<VaultStatus, String> {
    application::initialize_vault(&state.core, passphrase)
}

#[tauri::command]
pub(crate) fn unlock_vault(
    state: State<'_, AppState>,
    passphrase: String,
) -> Result<VaultStatus, String> {
    application::unlock_vault(&state.core, passphrase)
}

#[tauri::command]
pub(crate) fn lock_vault(state: State<'_, AppState>) -> Result<VaultStatus, String> {
    application::lock_vault(&state.core)
}

#[tauri::command]
pub(crate) fn change_vault_passphrase(
    state: State<'_, AppState>,
    current_passphrase: String,
    new_passphrase: String,
) -> Result<VaultStatus, String> {
    application::change_vault_passphrase(&state.core, current_passphrase, new_passphrase)
}

#[tauri::command]
pub(crate) fn backup_vault(
    state: State<'_, AppState>,
    destination_path: String,
) -> Result<VaultBackupReceipt, String> {
    application::backup_vault(&state.core, destination_path)
}

#[tauri::command]
pub(crate) fn restore_vault(
    state: State<'_, AppState>,
    source_path: String,
    passphrase: String,
    confirm_overwrite: bool,
) -> Result<VaultStatus, String> {
    application::restore_vault(&state.core, source_path, passphrase, confirm_overwrite)
}

#[tauri::command]
pub(crate) fn discover_browsers() -> Vec<BrowserCandidate> {
    application::discover_browsers()
}

#[tauri::command]
pub(crate) fn detect_wsl() -> WslStatus {
    application::detect_wsl()
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
pub(crate) fn environment_backend_execute(
    state: State<'_, AppState>,
    request: EnvironmentOperationRequest,
) -> Result<EnvironmentActionReceipt, String> {
    application::environment_backend_execute(&state.core, request)
}

#[tauri::command]
pub(crate) fn list_legacy_environment_artifacts(
    state: State<'_, AppState>,
) -> Result<Vec<LegacyEnvironmentArtifact>, String> {
    application::list_legacy_environment_artifacts(&state.core)
}

#[tauri::command]
pub(crate) fn cleanup_legacy_environment_artifact(
    state: State<'_, AppState>,
    silo_id: Uuid,
    backend: EnvironmentBackendId,
    confirm_cleanup: bool,
) -> Result<EnvironmentActionReceipt, String> {
    application::cleanup_legacy_environment_artifact(&state.core, silo_id, backend, confirm_cleanup)
}

#[tauri::command]
pub(crate) fn inspect_mihomo_controller(
    input: MihomoControllerInput,
) -> Result<MihomoSnapshot, String> {
    application::inspect_mihomo_controller(input)
}

#[tauri::command]
pub(crate) fn probe_local_clash(secret: Option<String>) -> LocalClashProbe {
    application::probe_local_clash(secret)
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
pub(crate) fn create_managed_silo(
    state: State<'_, AppState>,
    input: CreateManagedSiloInput,
) -> Result<Silo, String> {
    application::create_managed_silo(&state.core, input)
}

#[tauri::command]
pub(crate) fn list_managed_identity_previews(
    state: State<'_, AppState>,
) -> Result<std::collections::HashMap<Uuid, ManagedIdentityPreview>, String> {
    application::list_managed_identity_previews(&state.core)
}

#[tauri::command]
pub(crate) fn update_managed_identity(
    state: State<'_, AppState>,
    silo_id: Uuid,
    input: UpdateManagedIdentityInput,
) -> Result<Silo, String> {
    application::update_managed_identity(&state.core, silo_id, input)
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
pub(crate) fn update_silo_configuration(
    state: State<'_, AppState>,
    silo_id: Uuid,
    input: UpdateSiloInput,
    network_input: Option<UpdateSiloNetworkInput>,
    engine_input: Option<UpdateSiloEngineInput>,
) -> Result<Silo, String> {
    application::update_silo_configuration(&state.core, silo_id, input, network_input, engine_input)
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
pub(crate) fn update_silo_network(
    state: State<'_, AppState>,
    silo_id: Uuid,
    input: UpdateSiloNetworkInput,
) -> Result<Silo, String> {
    application::update_silo_network(&state.core, silo_id, input)
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
pub(crate) fn delete_silo(
    state: State<'_, AppState>,
    silo_id: Uuid,
    confirm_permanent: bool,
) -> Result<(), String> {
    application::delete_silo(&state.core, silo_id, confirm_permanent)
}

#[tauri::command]
pub(crate) fn silo_storage_usage(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<SiloStorageUsage, String> {
    application::silo_storage_usage(&state.core, silo_id)
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
pub(crate) fn recheck_silo_browser(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<BrowserVerification, String> {
    application::recheck_silo_browser(&state.core, silo_id)
}

#[tauri::command]
pub(crate) fn recheck_silo_runtime(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RuntimeActivation, String> {
    application::recheck_silo_runtime(&state.core, silo_id)
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
pub(crate) fn rebind_silo_mihomo(
    state: State<'_, AppState>,
    silo_id: Uuid,
) -> Result<RuntimeActivation, String> {
    application::rebind_silo_mihomo(&state.core, silo_id)
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
