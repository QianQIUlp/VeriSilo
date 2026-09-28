use super::environments::{
    clear_environment_runtime_record, environment_runtime_is_active,
    environment_runtime_record_exists, persist_environment_runtime_record,
    prepare_wsl_distribution, reconcile_environment_runtime_if_needed,
    stop_environment_runtime_for_vault_lock, wsl_network_profile, EnvironmentRuntimeRecord,
    ENVIRONMENT_RUNTIME_RECORD_SCHEMA_VERSION,
};
use super::DesktopCore;
use crate::domain::{
    BrowserVerification, RuntimeActivation, RuntimeSessionStatus, RuntimeState,
    SiloExecutionTarget, VaultLockState, VaultStatus, LOCAL_MANAGED_SESSION_LIMIT,
};
use crate::engine::{EngineAdapterId, SiloEngineConfig, VaultSeedIdentityTokenDeriver};
use crate::environment::backend::{EnvironmentBackendId, EnvironmentOperation};
use crate::environment::EnvironmentOperationRequest;
use crate::launcher::RuntimeManager;
use crate::local_runtimes::supports_managed_concurrency;
use crate::vault::VaultRuntime;
use crate::{engine, native_host, website_identity};
use chrono::Utc;
use serde::Serialize;
use std::sync::{Arc, Mutex};
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

fn scrub_locked_session(session: &mut RuntimeSessionStatus) {
    session.website_identity = None;
    session.activation.browser_verification = None;
    session.activation.engine_evidence = None;
    session.activation.network_evidence = None;
    session.activation.identity_evidence = None;
    if session.activation.state != RuntimeState::RecoveryRequired {
        session.activation.message =
            Some("保险库已锁定；当前运行证据暂不可用，浏览器占用状态未更改。".to_owned());
    }
}

fn merge_legacy_session(
    state: &DesktopCore,
    sessions: &mut Vec<RuntimeSessionStatus>,
    silo_id: Uuid,
    activation: RuntimeActivation,
    website_identity: Option<website_identity::WebsiteIdentityObservation>,
    legacy_occupied: bool,
) {
    let legacy_status = RuntimeSessionStatus {
        silo_id,
        activation,
        website_identity,
    };
    if let Some(current) = sessions
        .iter_mut()
        .find(|session| session.silo_id == silo_id)
    {
        if legacy_occupied && state.local_runtimes.slot_occupied(silo_id) {
            current.activation = RuntimeActivation {
                active_silo_id: Some(silo_id),
                state: RuntimeState::RecoveryRequired,
                updated_at: Utc::now(),
                message: Some("Two runtime records claim this Silo; resolve both browser processes and Profile locks before reuse.".to_owned()),
                browser_verification: None,
                engine_evidence: None,
                network_evidence: None,
                identity_evidence: None,
            };
            current.website_identity = None;
        } else if legacy_occupied {
            *current = legacy_status;
        }
    } else {
        sessions.push(legacy_status);
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
    _vault: &VaultStatus,
) {
    let Ok(_publication) = state.runtime_status_publish.lock() else {
        return;
    };
    let latest_vault = match state.vault.lock() {
        Ok(mut vault) => vault.status(&state.root),
        Err(_) => return,
    };
    let mut sessions = state.local_runtimes.snapshots();
    if let Ok(runtime) = state.runtime.try_lock() {
        let legacy_activation = runtime.cached_activation();
        if let Some(silo_id) = legacy_activation
            .active_silo_id
            .or_else(|| runtime.recorded_silo_id())
        {
            let website_identity = (legacy_activation.active_silo_id == Some(silo_id)
                && legacy_activation.state == RuntimeState::Running)
                .then(|| runtime.website_identity())
                .flatten();
            merge_legacy_session(
                state,
                &mut sessions,
                silo_id,
                legacy_activation,
                website_identity,
                runtime.has_runtime_ownership(),
            );
        }
    } else if let Some(silo_id) = activation.active_silo_id {
        merge_legacy_session(
            state,
            &mut sessions,
            silo_id,
            activation.clone(),
            None,
            true,
        );
    }
    if let Ok(environment) = state.environment_runtime.try_lock() {
        if let Some(silo_id) = environment.activation.active_silo_id {
            if !sessions.iter().any(|session| session.silo_id == silo_id) {
                sessions.push(RuntimeSessionStatus {
                    silo_id,
                    activation: environment.activation.clone(),
                    website_identity: None,
                });
            }
        }
    }
    sessions.sort_by_key(|session| session.silo_id);
    if !matches!(latest_vault.state, VaultLockState::Unlocked) {
        for session in &mut sessions {
            scrub_locked_session(session);
        }
    }
    let active = sessions
        .iter()
        .filter(|session| session.activation.active_silo_id == Some(session.silo_id))
        .collect::<Vec<_>>();
    let mut unique = if active.len() == 1 {
        Some(&active[0].activation)
    } else if active.is_empty() && sessions.len() == 1 {
        Some(&sessions[0].activation)
    } else if sessions.is_empty() {
        Some(activation)
    } else {
        None
    };
    let fallback_locked =
        if sessions.is_empty() && !matches!(latest_vault.state, VaultLockState::Unlocked) {
            let mut fallback = RuntimeSessionStatus {
                silo_id: activation.active_silo_id.unwrap_or_else(Uuid::nil),
                activation: activation.clone(),
                website_identity: None,
            };
            scrub_locked_session(&mut fallback);
            Some(fallback.activation)
        } else {
            None
        };
    if fallback_locked.is_some() {
        unique = fallback_locked.as_ref();
    }
    let _ =
        native_host::write_runtime_sessions_snapshot(&state.root, unique, &sessions, &latest_vault);
    drop(_publication);
    if !matches!(latest_vault.state, VaultLockState::Unlocked) {
        state.local_runtimes.revoke_all_for_vault_lock();
    }
}

fn save_recent_run(
    state: &DesktopCore,
    silo_id: Uuid,
    activation: &RuntimeActivation,
) -> Result<(), String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault
        .update_recent_run(&state.root, silo_id, activation)
        .map(|_| ())
        .map_err(|error| format!("最近运行记录保存失败：{error}"))
}

fn report_recent_run_error(error: &str) {
    eprintln!("recent run snapshot unavailable: {error}");
}

fn save_recent_run_from_status(
    state: &DesktopCore,
    activation: &RuntimeActivation,
) -> Result<(), String> {
    let silo_id = if let Some(id) = activation.active_silo_id {
        id
    } else {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
        match runtime.recorded_silo_id() {
            Some(id) => id,
            None => return Ok(()),
        }
    };
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    if !matches!(vault.status(&state.root).state, VaultLockState::Unlocked) {
        return Ok(());
    }
    let Some(record) = vault
        .get_recent_run(silo_id)
        .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };
    // A global stopped record cannot be attributed to a newer failed launch
    // whose runtime never acquired its own ID.
    if activation.active_silo_id.is_none() && record.runtime_id.is_none() {
        return Ok(());
    }
    vault
        .update_recent_run(&state.root, silo_id, activation)
        .map(|_| ())
        .map_err(|error| format!("最近运行记录保存失败：{error}"))
}

fn save_recent_run_from_session(
    state: &DesktopCore,
    session: &RuntimeSessionStatus,
) -> Result<(), String> {
    let runtime_id = session
        .activation
        .network_evidence
        .as_ref()
        .map(|evidence| evidence.runtime_id)
        .or_else(|| {
            session
                .activation
                .identity_evidence
                .as_ref()
                .map(|evidence| evidence.runtime_id)
        })
        .or_else(|| {
            state
                .local_runtimes
                .get(session.silo_id)
                .and_then(|runtime| {
                    runtime
                        .try_lock()
                        .ok()
                        .and_then(|runtime| runtime.recorded_runtime_id())
                })
        });
    let Some(runtime_id) = runtime_id else {
        return Ok(());
    };
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let recent = vault
        .get_recent_run(session.silo_id)
        .map_err(|error| error.to_string())?;
    if recent.as_ref().and_then(|recent| recent.runtime_id) != Some(runtime_id) {
        return Ok(());
    }
    vault
        .update_recent_run(&state.root, session.silo_id, &session.activation)
        .map(|_| ())
        .map_err(|error| format!("最近运行记录保存失败：{error}"))
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

pub(crate) fn reconcile_local_runtimes_if_possible(state: &DesktopCore) {
    for (silo_id, runtime_arc) in state.local_runtimes.entries() {
        let needs_reconciliation = runtime_arc
            .try_lock()
            .is_ok_and(|runtime| runtime.needs_reconciliation());
        if !needs_reconciliation {
            continue;
        }
        let silo_and_authentication = state.vault.lock().ok().and_then(|mut vault| {
            let silo = vault.get_silo(silo_id).ok()?;
            let authentication = vault
                .mihomo_controller_authentication_for_silo(silo_id)
                .ok()
                .flatten();
            Some((silo, authentication))
        });
        let Some((silo, authentication)) = silo_and_authentication else {
            continue;
        };
        if let Ok(mut runtime) = runtime_arc.try_lock() {
            if runtime.needs_reconciliation() {
                let activation = runtime.reconcile_persisted(&silo, authentication);
                let website_identity = runtime.website_identity();
                drop(runtime);
                state
                    .local_runtimes
                    .update(silo_id, activation, website_identity);
            }
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DesktopStatus {
    pub(crate) vault: VaultStatus,
    pub(crate) activation: Option<RuntimeActivation>,
    pub(crate) sessions: Vec<RuntimeSessionStatus>,
    pub(crate) managed_session_limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) website_identity: Option<website_identity::WebsiteIdentityObservation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) recent_runs_warning: Option<&'static str>,
}

pub(crate) fn runtime_for_silo(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Option<Arc<Mutex<RuntimeManager>>> {
    state.local_runtimes.get(silo_id)
}

fn collect_runtime_sessions(state: &DesktopCore) -> Result<Vec<RuntimeSessionStatus>, String> {
    let mut sessions = state.local_runtimes.snapshots();
    let legacy = state.runtime.try_lock().map_err(|_| {
        "The legacy local runtime is busy; its status cannot be attributed yet.".to_owned()
    })?;
    let legacy_id = legacy
        .cached_activation()
        .active_silo_id
        .or_else(|| legacy.recorded_silo_id());
    if let Some(silo_id) = legacy_id {
        let activation = legacy.cached_activation();
        let website_identity = (activation.active_silo_id == Some(silo_id)
            && activation.state == RuntimeState::Running)
            .then(|| legacy.website_identity())
            .flatten();
        merge_legacy_session(
            state,
            &mut sessions,
            silo_id,
            activation,
            website_identity,
            legacy.has_runtime_ownership(),
        );
    }
    drop(legacy);
    let environment = state
        .environment_runtime
        .lock()
        .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())?;
    if let Some(silo_id) = environment.activation.active_silo_id {
        if !sessions.iter().any(|session| session.silo_id == silo_id) {
            sessions.push(RuntimeSessionStatus {
                silo_id,
                activation: environment.activation.clone(),
                website_identity: None,
            });
        }
    }
    sessions.sort_by_key(|session| session.silo_id);
    Ok(sessions)
}

pub(crate) fn list_runtime_sessions(
    state: &DesktopCore,
) -> Result<Vec<RuntimeSessionStatus>, String> {
    let vault_status = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?
        .status(&state.root);
    if !matches!(vault_status.state, VaultLockState::Unlocked) {
        state.local_runtimes.revoke_all_for_vault_lock();
    }
    let mut sessions = collect_runtime_sessions(state)?;
    if !matches!(vault_status.state, VaultLockState::Unlocked) {
        for session in &mut sessions {
            scrub_locked_session(session);
        }
    }
    Ok(sessions)
}

pub(crate) fn get_silo_runtime(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Result<RuntimeSessionStatus, String> {
    let vault_status = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?
        .status(&state.root);
    if !matches!(vault_status.state, VaultLockState::Unlocked) {
        state.local_runtimes.revoke_all_for_vault_lock();
    }
    if let Some(runtime_arc) = runtime_for_silo(state, silo_id) {
        if !Arc::ptr_eq(&runtime_arc, &state.runtime) {
            if let Some(mut session) = state.local_runtimes.snapshot_for(silo_id) {
                if !matches!(vault_status.state, VaultLockState::Unlocked) {
                    scrub_locked_session(&mut session);
                }
                return Ok(session);
            }
        }
        let runtime = runtime_arc.try_lock().map_err(|_| {
            "The requested Silo runtime is busy; retry after its operation finishes.".to_owned()
        })?;
        let activation = runtime.cached_activation();
        let mut session = RuntimeSessionStatus {
            silo_id,
            website_identity: (matches!(vault_status.state, VaultLockState::Unlocked)
                && activation.active_silo_id == Some(silo_id)
                && activation.state == RuntimeState::Running)
                .then(|| runtime.website_identity())
                .flatten(),
            activation,
        };
        if !matches!(vault_status.state, VaultLockState::Unlocked) {
            scrub_locked_session(&mut session);
        }
        return Ok(session);
    }
    if let Ok(environment) = state.environment_runtime.try_lock() {
        if environment.activation.active_silo_id == Some(silo_id) {
            return Ok(RuntimeSessionStatus {
                silo_id,
                activation: environment.activation.clone(),
                website_identity: None,
            });
        }
    }
    state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?
        .get_silo(silo_id)
        .map_err(|error| error.to_string())?;
    Ok(RuntimeSessionStatus {
        silo_id,
        activation: RuntimeActivation::idle(),
        website_identity: None,
    })
}

fn unique_activation(
    state: &DesktopCore,
    sessions: &[RuntimeSessionStatus],
    fallback: RuntimeActivation,
    fallback_website_identity: Option<website_identity::WebsiteIdentityObservation>,
) -> (
    Option<RuntimeActivation>,
    Option<website_identity::WebsiteIdentityObservation>,
) {
    let occupied = sessions
        .iter()
        .filter(|session| {
            session.activation.active_silo_id == Some(session.silo_id)
                || state.local_runtimes.is_in_use(session.silo_id)
        })
        .collect::<Vec<_>>();
    let unique = if occupied.len() == 1 {
        Some(occupied[0])
    } else if occupied.is_empty() && sessions.len() == 1 {
        sessions.first()
    } else {
        None
    };
    if let Some(session) = unique {
        (
            Some(session.activation.clone()),
            session.website_identity.clone(),
        )
    } else if sessions.is_empty() {
        (Some(fallback), fallback_website_identity)
    } else if occupied.is_empty() {
        (Some(RuntimeActivation::idle()), None)
    } else {
        (None, None)
    }
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
    // Managed sessions have independent runtime locks. A slow B launch must
    // not hold a global lifecycle reservation while A reports status.
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
        state.local_runtimes.revoke_all_for_vault_lock();
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
            let mut runtime_busy = false;
            if matches!(vault_status.state, VaultLockState::Unlocked) {
                for entry in &inbox {
                    if let Some(runtime_arc) = runtime_for_silo(state, entry.silo_id) {
                        match runtime_arc.try_lock() {
                            Ok(mut runtime) => {
                                let activation = runtime.apply_network_evidence(entry);
                                let website_identity = runtime.website_identity();
                                drop(runtime);
                                state.local_runtimes.update(
                                    entry.silo_id,
                                    activation,
                                    website_identity,
                                );
                            }
                            Err(_) => runtime_busy = true,
                        }
                    }
                }
            }
            // Delete transport files only after the encrypted Vault commit (or
            // a successful duplicate/no-longer-relevant decision).
            if !runtime_busy {
                let _ = native_host::acknowledge_network_evidence_inbox(&state.root, &inbox);
            }
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
        state.local_runtimes.revoke_all_for_vault_lock();
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
    let mut sessions = collect_runtime_sessions(state)?;
    if !matches!(vault_status.state, VaultLockState::Unlocked) {
        for session in &mut sessions {
            scrub_locked_session(session);
        }
        let mut fallback = RuntimeSessionStatus {
            silo_id: activation.active_silo_id.unwrap_or_else(Uuid::nil),
            activation,
            website_identity: None,
        };
        scrub_locked_session(&mut fallback);
        activation = fallback.activation;
        website_identity = None;
    }
    let recent_runs_warning = if matches!(vault_status.state, VaultLockState::Unlocked) {
        let mut failed = false;
        let (legacy_id, legacy_activation) = state
            .runtime
            .lock()
            .map(|runtime| {
                let activation = runtime.cached_activation();
                (
                    activation
                        .active_silo_id
                        .or_else(|| runtime.recorded_silo_id()),
                    activation,
                )
            })
            .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
        if legacy_id.is_some_and(|id| {
            runtime_for_silo(state, id).is_some_and(|runtime| Arc::ptr_eq(&runtime, &state.runtime))
        }) {
            if let Err(error) = save_recent_run_from_status(state, &legacy_activation) {
                report_recent_run_error(&error);
                failed = true;
            }
        }
        for session in state.local_runtimes.snapshots() {
            if let Err(error) = save_recent_run_from_session(state, &session) {
                report_recent_run_error(&error);
                failed = true;
            }
        }
        failed.then_some("save_failed")
    } else {
        None
    };
    let (unique_activation, unique_website_identity) =
        unique_activation(state, &sessions, activation.clone(), website_identity);
    publish_runtime_status(&state, &activation, &vault_status);
    Ok(DesktopStatus {
        vault: vault_status,
        activation: unique_activation,
        sessions,
        managed_session_limit: LOCAL_MANAGED_SESSION_LIMIT,
        website_identity: unique_website_identity,
        recent_runs_warning,
    })
}

pub(crate) fn diagnostic_status_for_silo(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Result<DesktopStatus, String> {
    let _reservation = state.local_control.reserve_silo(silo_id)?;
    let (mut vault_status, target_reconciliation) = {
        let mut vault = state
            .vault
            .lock()
            .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
        let status = vault.status(&state.root);
        let reconciliation = if matches!(status.state, VaultLockState::Unlocked) {
            vault.get_silo(silo_id).ok().map(|silo| {
                let authentication = vault
                    .mihomo_controller_authentication_for_silo(silo_id)
                    .ok()
                    .flatten();
                (silo, authentication)
            })
        } else {
            None
        };
        (status, reconciliation)
    };
    if let Some(runtime_arc) = runtime_for_silo(state, silo_id) {
        if let Ok(mut runtime) = runtime_arc.try_lock() {
            let activation = if let Some((silo, authentication)) =
                target_reconciliation.filter(|_| runtime.needs_reconciliation())
            {
                runtime.reconcile_persisted(&silo, authentication)
            } else if matches!(vault_status.state, VaultLockState::Unlocked) {
                runtime.activation()
            } else {
                runtime.revoke_secrets_for_vault_lock()
            };
            runtime.hydrate_website_identity(activation.active_silo_id);
            let website_identity = runtime.website_identity();
            drop(runtime);
            state
                .local_runtimes
                .update(silo_id, activation, website_identity);
        }
    }
    vault_status = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?
        .status(&state.root);
    if !matches!(vault_status.state, VaultLockState::Unlocked) {
        state.local_runtimes.revoke_all_for_vault_lock();
    }
    let target = get_silo_runtime(state, silo_id)?;
    if matches!(vault_status.state, VaultLockState::Unlocked)
        && (target.activation.network_evidence.is_some()
            || target.activation.identity_evidence.is_some())
    {
        if let Err(error) = save_recent_run(state, silo_id, &target.activation) {
            report_recent_run_error(&error);
        }
    }
    publish_runtime_status(state, &target.activation, &vault_status);
    let mut sessions = state.local_runtimes.snapshots();
    if let Some(current) = sessions
        .iter_mut()
        .find(|session| session.silo_id == silo_id)
    {
        *current = target.clone();
    } else {
        sessions.push(target.clone());
    }
    if let Ok(environment) = state.environment_runtime.try_lock() {
        if let Some(environment_id) = environment.activation.active_silo_id {
            if !sessions
                .iter()
                .any(|session| session.silo_id == environment_id)
            {
                sessions.push(RuntimeSessionStatus {
                    silo_id: environment_id,
                    activation: environment.activation.clone(),
                    website_identity: None,
                });
            }
        }
    }
    sessions.sort_by_key(|session| session.silo_id);
    if !matches!(vault_status.state, VaultLockState::Unlocked) {
        for session in &mut sessions {
            scrub_locked_session(session);
        }
    }
    Ok(DesktopStatus {
        vault: vault_status,
        activation: Some(target.activation),
        sessions,
        managed_session_limit: LOCAL_MANAGED_SESSION_LIMIT,
        website_identity: target.website_identity,
        recent_runs_warning: None,
    })
}

pub(crate) fn recheck_silo_browser(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Result<BrowserVerification, String> {
    let _reservation = state.local_control.reserve_silo(silo_id)?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let is_active =
        state.local_runtimes.is_in_use(silo_id) || environment_runtime_is_active(&state, silo_id)?;
    vault
        .recheck_silo_browser(&state.root, silo_id, is_active)
        .map_err(|error| error.to_string())
}

pub(crate) fn recheck_silo_runtime(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Result<RuntimeActivation, String> {
    let _local_reservation = state.local_control.reserve_silo(silo_id)?;
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
            drop(vault);
            let runtime_arc = runtime_for_silo(state, silo_id)
                .ok_or_else(|| "该 Silo 当前没有可重新检查的活动或待恢复会话。".to_owned())?;
            let mut runtime = runtime_arc
                .lock()
                .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
            let activation = runtime
                .recheck_active(
                    &silo,
                    proxy_authentication.as_ref(),
                    mihomo_authentication.as_ref(),
                )
                .map_err(|error| error.to_string())?;
            let website_identity = runtime.website_identity();
            drop(runtime);
            state.local_runtimes.update(silo_id, activation.clone(), website_identity);
            if let Err(error) = save_recent_run(state, silo_id, &activation) {
                report_recent_run_error(&error);
            }
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
    let _local_reservation = state.local_control.reserve_silo(silo_id)?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let vault_status = vault.status(&state.root);
    if matches!(vault_status.state, VaultLockState::Locked) {
        drop(vault);
        let runtime_arc = runtime_for_silo(state, silo_id)
            .ok_or_else(|| "保险库已锁定，而且该 Silo 不是当前托管浏览器。".to_owned())?;
        let mut runtime = runtime_arc
            .lock()
            .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
        if runtime.active_managed_camoufox_silo_id() != Some(silo_id) {
            return Err("保险库已锁定，而且该 Silo 不是当前托管浏览器。".to_owned());
        }
        let activation = runtime
            .stop_managed_camoufox(silo_id)
            .map_err(managed_launcher_error)?;
        drop(runtime);
        state
            .local_runtimes
            .update(silo_id, activation.clone(), None);
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
            let runtime_arc = runtime_for_silo(state, silo_id)
                .ok_or_else(|| "该 Silo 没有活动的本地托管会话。".to_owned())?;
            let mut runtime = runtime_arc
                .lock()
                .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
            let activation = runtime
                .stop_managed_camoufox(silo_id)
                .map_err(managed_launcher_error)?;
            drop(runtime);
            state.local_runtimes.update(silo_id, activation.clone(), None);
            if let Err(error) = save_recent_run(state, silo_id, &activation) {
                report_recent_run_error(&error);
            }
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
    let _reservation = state.local_control.reserve_silo(silo_id)?;
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
    drop(vault);
    let runtime_arc = runtime_for_silo(state, silo_id)
        .ok_or_else(|| "The requested Silo has no local runtime to reconnect.".to_owned())?;
    let mut runtime = runtime_arc
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let activation = runtime
        .rebind_active_mihomo(
            &silo,
            proxy_authentication.as_ref(),
            mihomo_authentication.as_ref(),
        )
        .map_err(|error| error.to_string())?;
    let website_identity = runtime.website_identity();
    drop(runtime);
    state
        .local_runtimes
        .update(silo_id, activation.clone(), website_identity);
    if let Err(error) = save_recent_run(state, silo_id, &activation) {
        report_recent_run_error(&error);
    }
    publish_runtime_status(&state, &activation, &vault_status);
    Ok(activation)
}

pub(crate) fn launch_silo_with(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Result<RuntimeActivation, LaunchFailure> {
    let managed_parallel = {
        let mut vault = state
            .vault
            .lock()
            .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
        let silo = vault.get_silo(silo_id).map_err(|error| error.to_string())?;
        supports_managed_concurrency(&silo)
    };
    if managed_parallel {
        let _reservation = state.local_control.reserve_silo(silo_id)?;
        launch_silo_with_reserved(state, silo_id, managed_parallel)
    } else {
        let _reservation = state.local_control.reserve()?;
        launch_silo_with_reserved(state, silo_id, managed_parallel)
    }
}

fn launch_silo_with_reserved(
    state: &DesktopCore,
    silo_id: Uuid,
    managed_parallel: bool,
) -> Result<RuntimeActivation, LaunchFailure> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    let silo = vault.get_silo(silo_id).map_err(|error| error.to_string())?;
    if supports_managed_concurrency(&silo) != managed_parallel {
        return Err(
            "Silo execution settings changed during launch admission; retry the launch."
                .to_owned()
                .into(),
        );
    }
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
            let start_guard = if managed_parallel {
                Some(state.local_runtimes.reserve_start(&silo)?)
            } else {
                if state.local_runtimes.has_occupied() {
                    return Err("Stop or resolve every Managed Silo before starting this exclusive local engine.".to_owned().into());
                }
                state.local_runtimes.note_legacy_owner(silo_id);
                None
            };
            let runtime_arc = start_guard
                .as_ref()
                .map(|guard| guard.runtime())
                .unwrap_or_else(|| Arc::clone(&state.runtime));
            let already_occupied = runtime_arc
                .try_lock()
                .map_err(|_| "The selected local runtime is busy; retry after its health check finishes.".to_owned())?
                .has_runtime_ownership();
            if already_occupied {
                return Err((if managed_camoufox {
                    "This Managed Silo is already running or requires recovery.".to_owned()
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
            if matches!(silo.adapter_id(), EngineAdapterId::StockChrome | EngineAdapterId::StockEdge | EngineAdapterId::Camoufox) {
                vault.begin_recent_run(&state.root, &silo)
                    .map_err(|error| error.to_string())?;
            }
            if silo.adapter_id() == EngineAdapterId::Camoufox {
                if let Err(error) = vault.materialize_identity_artifact(&state.root, silo_id) {
                    let failure = RuntimeActivation {
                        active_silo_id: None,
                        state: RuntimeState::Failed,
                        updated_at: Utc::now(),
                        message: None,
                        browser_verification: None,
                        engine_evidence: None,
                        network_evidence: None,
                        identity_evidence: None,
                    };
                    vault
                        .update_recent_run(&state.root, silo_id, &failure)
                        .map_err(|save_error| {
                            format!("{error}; 最近运行记录保存失败：{save_error}")
                        })?;
                    return Err(managed_vault_error(error).into());
                }
            }
            if !matches!(vault.status(&state.root).state, VaultLockState::Unlocked) {
                return Err("The Vault locked during launch preparation; retry after unlocking it."
                    .to_owned()
                    .into());
            }
            drop(vault);
            let mut runtime = runtime_arc
                .lock()
                .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
            runtime.release_inactive_managed_session();
            let launch_started_at = Utc::now();
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
                    let website_identity = runtime.website_identity();
                    drop(runtime);
                    state.local_runtimes.update(silo_id, activation.clone(), website_identity);
                    drop(start_guard);
                    vault_status = state
                        .vault
                        .lock()
                        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?
                        .status(&state.root);
                    if !matches!(vault_status.state, VaultLockState::Unlocked) {
                        state.local_runtimes.revoke_all_for_vault_lock();
                        let current = runtime_arc
                            .lock()
                            .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?
                            .cached_activation();
                        publish_runtime_status(state, &current, &vault_status);
                        return Err("The Vault locked during launch; runtime credentials were revoked."
                            .to_owned()
                            .into());
                    }
                    if let Err(error) = save_recent_run(state, silo_id, &activation) {
                        report_recent_run_error(&error);
                    }
                    publish_runtime_status(&state, &activation, &vault_status);
                    Ok(activation)
                }
                Err(error) => {
                    let activation = runtime.activation();
                    let website_identity = runtime.website_identity();
                    drop(runtime);
                    state.local_runtimes.update(silo_id, activation.clone(), website_identity);
                    drop(start_guard);
                    vault_status = state
                        .vault
                        .lock()
                        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?
                        .status(&state.root);
                    if !matches!(vault_status.state, VaultLockState::Unlocked) {
                        state.local_runtimes.revoke_all_for_vault_lock();
                    }
                    // Early launch rejection may leave the previous activation intact.
                    // Never attach that previous run to the new attempt's declaration.
                    let failed_snapshot = if activation.updated_at < launch_started_at ||
                        !matches!(activation.state, RuntimeState::Failed | RuntimeState::VerificationFailed | RuntimeState::RecoveryRequired) {
                        RuntimeActivation { state: RuntimeState::Failed, ..RuntimeActivation::idle() }
                    } else {
                        activation.clone()
                    };
                    if let Err(save_error) = save_recent_run(state, silo_id, &failed_snapshot) {
                        report_recent_run_error(&save_error);
                    }
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
            if state.local_runtimes.has_occupied() {
                return Err("Stop or resolve every Managed Silo before starting this exclusive Linux environment.".to_owned().into());
            }
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
