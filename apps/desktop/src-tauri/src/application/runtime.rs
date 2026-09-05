use super::environments::{
    clear_environment_runtime_record, environment_runtime_is_active,
    environment_runtime_record_exists, persist_environment_runtime_record,
    prepare_wsl_distribution, reconcile_environment_runtime_if_needed,
    stop_environment_runtime_for_vault_lock, wsl_network_profile, EnvironmentRuntimeRecord,
    ENVIRONMENT_RUNTIME_RECORD_SCHEMA_VERSION,
};
use super::DesktopCore;
use crate::domain::{
    BrowserVerification, RuntimeActivation, RuntimeState, SiloExecutionTarget, VaultLockState,
    VaultStatus,
};
use crate::engine::{EngineAdapterId, SiloEngineConfig, VaultSeedIdentityTokenDeriver};
use crate::environment::backend::{EnvironmentBackendId, EnvironmentOperation};
use crate::environment::EnvironmentOperationRequest;
use crate::launcher::RuntimeManager;
use crate::vault::VaultRuntime;
use crate::{engine, native_host, website_identity};
use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use super::identity::{managed_launcher_error, managed_vault_error};

pub(crate) fn publish_runtime_status(
    state: &DesktopCore,
    activation: &RuntimeActivation,
    vault: &VaultStatus,
) {
    let _ = native_host::write_runtime_status_snapshot(&state.root, activation, vault);
}

pub(crate) fn reconcile_runtime_if_possible(
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
pub(crate) struct DesktopStatus {
    pub(crate) vault: VaultStatus,
    pub(crate) activation: RuntimeActivation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) website_identity: Option<website_identity::WebsiteIdentityObservation>,
}

pub(crate) fn desktop_status(state: &DesktopCore) -> Result<DesktopStatus, String> {
    desktop_status_with(&state)
}

pub(crate) fn desktop_status_with(state: &DesktopCore) -> Result<DesktopStatus, String> {
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
    let website_identity = if matches!(vault_status.state, VaultLockState::Unlocked) {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
        runtime.hydrate_website_identity(activation.active_silo_id);
        runtime.website_identity()
    } else {
        None
    };
    Ok(DesktopStatus {
        vault: vault_status,
        activation,
        website_identity,
    })
}

pub(crate) fn recheck_silo_browser(
    state: &DesktopCore,
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

pub(crate) fn recheck_silo_runtime(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Result<RuntimeActivation, String> {
    let _local_reservation = state.local_control.reserve()?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let vault_status = vault.status(&state.root);
    vault.record_activity().map_err(|error| error.to_string())?;
    let silo = vault.get_silo(silo_id).map_err(|error| error.to_string())?;
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

pub(crate) fn stop_silo_with(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Result<RuntimeActivation, String> {
    let _local_reservation = state.local_control.reserve()?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let vault_status = vault.status(&state.root);
    if matches!(vault_status.state, VaultLockState::Locked) {
        drop(vault);
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
        if runtime.active_managed_camoufox_silo_id() != Some(silo_id) {
            return Err("保险库已锁定，而且该 Silo 不是当前托管浏览器。".to_owned());
        }
        let activation = runtime
            .stop_managed_camoufox(silo_id)
            .map_err(managed_launcher_error)?;
        publish_runtime_status(state, &activation, &vault_status);
        return Ok(activation);
    }
    vault.record_activity().map_err(|error| error.to_string())?;
    let silo = vault.get_silo(silo_id).map_err(|error| error.to_string())?;
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

pub(crate) fn rebind_silo_mihomo(
    state: &DesktopCore,
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

pub(crate) fn launch_silo_with(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Result<RuntimeActivation, String> {
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
            runtime.release_inactive_managed_session();
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

pub(crate) fn effective_runtime_activation(
    state: &DesktopCore,
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
