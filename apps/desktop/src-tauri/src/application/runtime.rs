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

use super::identity::{
    managed_launcher_error, managed_launcher_failure, managed_vault_error, ManagedLauncherFailure,
};

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum LaunchFailure {
    Plain(String),
    Managed(ManagedLauncherFailure),
}

impl From<String> for LaunchFailure {
    fn from(value: String) -> Self {
        Self::Plain(value)
    }
}

impl std::fmt::Display for LaunchFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plain(message) => formatter.write_str(message),
            Self::Managed(failure) => match &failure.detail {
                Some(detail) => write!(formatter, "{}: {detail}", failure.code),
                None => formatter.write_str(failure.code),
            },
        }
    }
}

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
    desktop_status_with_refresh(state, RuntimeManager::activation)
}

pub(super) fn desktop_status_with_refresh(
    state: &DesktopCore,
    refresh: impl FnOnce(&mut RuntimeManager) -> RuntimeActivation,
) -> Result<DesktopStatus, String> {
    let _local_reservation = state.local_control.reserve()?;
    // Reserve lifecycle changes, but never hold Vault while waiting for the
    // watchdog's Runtime lock or performing Host/proxy health I/O. Each guard
    // is released before acquiring the other; the Vault -> Runtime order for
    // operations that need both locks remains unchanged.
    let reconciliation_id = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
        runtime
            .needs_reconciliation()
            .then(|| runtime.recorded_silo_id())
            .flatten()
    };
    let (mut vault_status, reconciliation) = {
        let mut vault = state
            .vault
            .lock()
            .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
        let reconciliation = reconciliation_id.and_then(|silo_id| {
            let silo = vault.get_silo(silo_id).ok()?;
            let authentication = vault
                .mihomo_controller_authentication_for_silo(silo_id)
                .ok()
                .flatten();
            Some((silo, authentication))
        });
        (vault.status(&state.root), reconciliation)
    };
    let mut activation = {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
        if matches!(vault_status.state, VaultLockState::Unlocked) {
            if let Some((silo, authentication)) = reconciliation.filter(|(silo, _)| {
                runtime.needs_reconciliation() && runtime.recorded_silo_id() == Some(silo.id)
            }) {
                runtime.reconcile_persisted(&silo, authentication)
            } else {
                refresh(&mut runtime)
            }
        } else {
            // The shared pre-publication path below performs revocation;
            // no locked snapshot is published from this provisional value.
            runtime.cached_activation()
        }
    };
    let environment_silos = {
        let mut vault = state
            .vault
            .lock()
            .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
        // Health checks can cross the auto-lock deadline. Reads do not renew it.
        vault_status = vault.status(&state.root);
        if matches!(vault_status.state, VaultLockState::Unlocked) {
            vault
                .list_active_silos()
                .map_err(|error| error.to_string())?
        } else {
            Vec::new()
        }
    };
    if matches!(vault_status.state, VaultLockState::Unlocked) {
        reconcile_environment_runtime_if_needed(&state, &environment_silos)?;
    }
    // Inbox acceptance consumes this snapshot. Publish it only after revoking
    // runtime secrets if health/provider reconciliation crossed auto-lock.
    vault_status = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?
        .status(&state.root);
    if !matches!(vault_status.state, VaultLockState::Unlocked) {
        activation = state
            .runtime
            .lock()
            .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?
            .revoke_secrets_for_vault_lock();
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
        // Commit history before applying/acknowledging it. Applying an entry
        // also refreshes runtime health, so it must run after releasing Vault.
        let imported = {
            let mut vault = state
                .vault
                .lock()
                .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
            let imported = vault
                .import_network_evidence(&state.root, inbox.clone())
                .is_ok();
            vault_status = vault.status(&state.root);
            imported
        };
        if imported {
            if matches!(vault_status.state, VaultLockState::Unlocked) {
                let mut runtime = state
                    .runtime
                    .lock()
                    .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
                for entry in &inbox {
                    runtime.apply_network_evidence(entry);
                }
            }
            // Delete transport files only after the encrypted Vault commit (or
            // a successful duplicate/no-longer-relevant decision).
            let _ = native_host::acknowledge_network_evidence_inbox(&state.root, &inbox);
        }
    }
    // Reconcile identity against current Vault metadata after all health and
    // inbox work. Lifecycle reservation prevents an intervening Silo rebind.
    let identity_silo_id = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?
        .cached_activation()
        .identity_evidence
        .map(|evidence| evidence.silo_id);
    let identity_silo = {
        let mut vault = state
            .vault
            .lock()
            .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
        let silo = identity_silo_id.and_then(|silo_id| vault.get_silo(silo_id).ok());
        vault_status = vault.status(&state.root);
        silo
    };
    let mut website_identity = {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
        if matches!(vault_status.state, VaultLockState::Unlocked) {
            if let Some(silo) = identity_silo {
                runtime.reconcile_identity_evidence(&silo);
            } else if identity_silo_id.is_some() {
                runtime.mark_identity_evidence_stale("这份网站身份观察所属的 Silo 已不存在。");
            }
            activation =
                effective_runtime_activation(state, &vault_status, runtime.cached_activation())?;
            runtime.hydrate_website_identity(activation.active_silo_id);
            runtime.website_identity()
        } else {
            None
        }
    };
    // Runtime contention or hydration may have crossed the deadline too.
    vault_status = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?
        .status(&state.root);
    if !matches!(vault_status.state, VaultLockState::Unlocked) {
        activation = state
            .runtime
            .lock()
            .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?
            .revoke_secrets_for_vault_lock();
        website_identity = None;
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
        website_identity,
    })
}

pub(crate) fn diagnostic_status_for_silo(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Result<DesktopStatus, String> {
    {
        // Keep lifecycle attribution stable while taking short, independent
        // snapshots. Waiting for the watchdog must not block Vault readers.
        let _local_reservation = state.local_control.reserve()?;
        let local_activation = state
            .runtime
            .lock()
            .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?
            .cached_activation();
        let activation = {
            let environment = state
                .environment_runtime
                .lock()
                .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())?;
            if environment.has_active_silo() {
                environment.activation.clone()
            } else {
                local_activation
            }
        };
        let vault_status = state
            .vault
            .lock()
            .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?
            .status(&state.root);
        if matches!(vault_status.state, VaultLockState::Unlocked)
            && activation.active_silo_id.is_some_and(|id| id != silo_id)
        {
            // Do not refresh/reconcile another Silo's provider while diagnosing
            // this one. Its lifecycle/watchdog remains responsible for health.
            return Ok(DesktopStatus {
                vault: vault_status,
                activation,
                website_identity: None,
            });
        }
    }
    desktop_status_with(state)
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
                    identity_evidence: None,
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
                            identity_evidence: None,
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
                            identity_evidence: None,
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
                identity_evidence: None,
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
) -> Result<RuntimeActivation, LaunchFailure> {
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
                return Err((if managed_camoufox {
                    "managed_another_silo_running".to_owned()
                } else {
                    "Close the active browser Silo before starting another Silo.".to_owned()
                }).into());
            }
            {
                let mut environment_runtime = state.environment_runtime.lock().map_err(|_| {
                    "VeriSilo environment runtime state is unavailable.".to_owned()
                })?;
                if environment_runtime.has_active_silo() {
                    return Err((if managed_camoufox {
                        "managed_another_silo_running".to_owned()
                    } else {
                        "Stop the active Silo before starting another run location.".to_owned()
                    }).into());
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
                        LaunchFailure::Managed(managed_launcher_failure(error))
                    } else {
                        LaunchFailure::Plain(error.to_string())
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
                        .to_owned().into(),
                );
            }
            drop(runtime);
            {
                let mut environment_runtime = state.environment_runtime.lock().map_err(|_| {
                    "VeriSilo environment runtime state is unavailable.".to_owned()
                })?;
                if environment_runtime.has_active_silo() {
                    return Err("Stop the active Silo before starting another run location."
                        .to_owned().into());
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
                    identity_evidence: None,
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
                    identity_evidence: None,
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
                            identity_evidence: None,
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
                            identity_evidence: None,
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
                            .unwrap_or(error).into())
                    } else {
                        Err(error.into())
                    }
                }
            }
        }
        SiloExecutionTarget::Remote { .. } => Err(
            "This Silo targets a remote node, but this build has no verified remote browser identity runtime. VeriSilo will not fall back to the local computer."
                .to_owned().into(),
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
