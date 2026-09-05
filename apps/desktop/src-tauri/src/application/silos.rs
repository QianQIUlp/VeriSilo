use super::environments::{
    environment_runtime_is_active, prepare_wsl_distribution, verified_current_wsl_artifact,
};
use super::identity::{
    managed_browser_package_root, managed_proxy_error, managed_vault_error,
    provision_managed_artifact,
};
use super::runtime::desktop_status_with;
use super::DesktopCore;
use crate::domain::{
    CreateSiloInput, ManagedIdentityPreset, NetworkProfile, ProxyScheme as SiloProxyScheme, Silo,
    SiloExecutionTarget, SiloStorageUsage, UpdateSiloEngineInput, UpdateSiloInput,
    UpdateSiloNetworkInput,
};
use crate::environment::backend::{EnvironmentBackendId, EnvironmentOperation};
use crate::environment::EnvironmentOperationRequest;
use crate::launcher::profile_in_use;
use crate::vault::StoredIdentityArtifact;
use crate::{mihomo, native_host};
use uuid::Uuid;

pub(crate) fn list_silos(state: &DesktopCore) -> Result<Vec<Silo>, String> {
    list_silos_with(&state)
}

pub(crate) fn list_silos_with(state: &DesktopCore) -> Result<Vec<Silo>, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.list_silos().map_err(|error| error.to_string())
}

pub(crate) fn diagnose_silo_with(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Result<serde_json::Value, String> {
    let silo = list_silos_with(state)?
        .into_iter()
        .find(|silo| silo.id == silo_id)
        .ok_or_else(|| format!("没有找到 Silo {silo_id}。"))?;
    let status = desktop_status_with(state)?;
    let active = status.activation.active_silo_id == Some(silo.id);
    Ok(serde_json::json!({
        "siloId": silo.id,
        "name": silo.name,
        "adapter": silo.adapter_id(),
        "identityLocked": silo.identity_locked_at.is_some(),
        "network": silo.network_profile,
        "runtimeState": status.activation.state,
        "runtimeMessage": status.activation.message,
        "active": active,
        "clash": mihomo::diagnose_local_clash(""),
        "vault": status.vault.state,
        "websiteIdentity": status
            .website_identity
            .as_ref()
            .filter(|identity| identity.silo_id == silo.id),
    }))
}

pub(crate) fn page_action_with(
    state: &DesktopCore,
    silo_id: Uuid,
    action: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let _local_reservation = state.local_control.reserve()?;
    state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?
        .page_action(silo_id, action)
        .map_err(|error| error.to_string())
}

pub(crate) fn list_active_silos(state: &DesktopCore) -> Result<Vec<Silo>, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.list_active_silos().map_err(|error| error.to_string())
}

pub(crate) fn list_archived_silos(state: &DesktopCore) -> Result<Vec<Silo>, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault
        .list_archived_silos()
        .map_err(|error| error.to_string())
}

pub(crate) fn create_silo(state: &DesktopCore, input: CreateSiloInput) -> Result<Silo, String> {
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

pub(crate) fn update_silo(
    state: &DesktopCore,
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

pub(crate) fn update_silo_configuration(
    state: &DesktopCore,
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
            .map_err(|error| managed_proxy_error(error.to_string()))?;
        if !matches!(
            &network_input.network_profile,
            NetworkProfile::Direct {
                proxy_required: false
            } | NetworkProfile::FixedProxy {
                proxy_required: true,
                scheme: SiloProxyScheme::Http | SiloProxyScheme::Socks5,
                ..
            }
        ) {
            return Err("managed_proxy_required".to_owned());
        }
        let mut intent = vault
            .managed_identity_intent_for_silo(silo_id)
            .map_err(managed_vault_error)?;
        let has_proxy = network_input.network_profile.requires_proxy();
        intent.follow_network_exit = has_proxy;
        if !has_proxy && intent.identity_preset.requires_proxy() {
            intent.identity_preset = ManagedIdentityPreset::BalancedEnUs;
        }
        let seed = vault
            .identity_seed_for_silo(silo_id)
            .map_err(managed_vault_error)?;
        let mihomo_secret = network_input
            .mihomo_controller_secret
            .as_ref()
            .map(|secret| secret.secret.as_str());
        let result = provision_managed_artifact(
            &state.root,
            &managed_browser_package_root(&state),
            silo_id,
            &intent,
            &network_input.network_profile,
            network_input.proxy_credentials.as_ref(),
            mihomo_secret,
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

pub(crate) fn rename_silo(
    state: &DesktopCore,
    silo_id: Uuid,
    name: String,
) -> Result<Silo, String> {
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

pub(crate) fn update_silo_network(
    state: &DesktopCore,
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
            .map_err(|error| managed_proxy_error(error.to_string()))?;
        if !matches!(
            &input.network_profile,
            NetworkProfile::Direct {
                proxy_required: false
            } | NetworkProfile::FixedProxy {
                proxy_required: true,
                scheme: SiloProxyScheme::Http | SiloProxyScheme::Socks5,
                ..
            }
        ) {
            return Err("managed_proxy_required".to_owned());
        }
        if is_active {
            return Err("managed_another_silo_running".to_owned());
        }
        let mut intent = vault
            .managed_identity_intent_for_silo(silo_id)
            .map_err(managed_vault_error)?;
        let has_proxy = input.network_profile.requires_proxy();
        intent.follow_network_exit = has_proxy;
        if !has_proxy && intent.identity_preset.requires_proxy() {
            intent.identity_preset = ManagedIdentityPreset::BalancedEnUs;
        }
        let seed = vault
            .identity_seed_for_silo(silo_id)
            .map_err(managed_vault_error)?;
        let mihomo_secret = input
            .mihomo_controller_secret
            .as_ref()
            .map(|secret| secret.secret.as_str());
        let result = provision_managed_artifact(
            &state.root,
            &managed_browser_package_root(&state),
            silo_id,
            &intent,
            &input.network_profile,
            input.proxy_credentials.as_ref(),
            mihomo_secret,
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

pub(crate) fn update_silo_engine(
    state: &DesktopCore,
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

pub(crate) fn archive_silo(state: &DesktopCore, silo_id: Uuid) -> Result<(), String> {
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

pub(crate) fn restore_archived_silo(state: &DesktopCore, silo_id: Uuid) -> Result<Silo, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault
        .restore_archived_silo(&state.root, silo_id)
        .map_err(|error| error.to_string())
}

pub(crate) fn delete_silo(
    state: &DesktopCore,
    silo_id: Uuid,
    confirm_permanent: bool,
) -> Result<(), String> {
    delete_silo_with(&state, silo_id, confirm_permanent)
}

pub(crate) fn delete_silo_with(
    state: &DesktopCore,
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
    let is_active = runtime.is_active(silo_id) || environment_runtime_is_active(state, silo_id)?;
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
        prepare_wsl_distribution(state, &distribution, &[EnvironmentOperation::Destroy])?;
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
    let is_active = runtime.is_active(silo_id) || environment_runtime_is_active(state, silo_id)?;
    vault
        .delete_silo(&state.root, silo_id, is_active, confirm_permanent)
        .map_err(|error| error.to_string())
}

pub(crate) fn silo_storage_usage(
    state: &DesktopCore,
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

pub(crate) fn list_network_evidence(
    state: &DesktopCore,
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

pub(crate) fn clear_network_evidence(
    state: &DesktopCore,
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
