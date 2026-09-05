use super::DesktopCore;
use crate::domain::{
    NetworkProfile, ProxyScheme as SiloProxyScheme, RuntimeActivation, RuntimeState, Silo,
    SiloExecutionTarget,
};
use crate::environment;
use crate::environment::backend::{
    EnvironmentActionReceipt, EnvironmentBackendId, EnvironmentBackendStatus,
    EnvironmentNetworkProfile, EnvironmentOperation, OperationAvailability,
    ProxyScheme as EnvironmentProxyScheme,
};
use crate::environment::{EnvironmentOperationRequest, WslStatus};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Mutex, MutexGuard};
use uuid::Uuid;

pub(crate) const ENVIRONMENT_RUNTIME_RECORD_FILE: &str = "environment-runtime.json";

pub(crate) const ENVIRONMENT_RUNTIME_RECORD_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyEnvironmentArtifact {
    pub(crate) silo_id: Uuid,
    pub(crate) backend: EnvironmentBackendId,
    pub(crate) cleanup_available: bool,
    pub(crate) message: String,
}

#[derive(Default)]
pub(crate) struct LocalEnvironmentControl {
    pub(crate) reservation: Mutex<()>,
}

impl LocalEnvironmentControl {
    pub(crate) fn reserve(&self) -> Result<MutexGuard<'_, ()>, String> {
        self.reservation
            .lock()
            .map_err(|_| "VeriSilo local environment reservation is unavailable.".to_owned())
    }
}

pub(crate) struct EnvironmentRuntimeState {
    pub(crate) activation: RuntimeActivation,
    pub(crate) wsl_distribution: Option<String>,
    pub(crate) reconciled: bool,
    pub(crate) recovery_blocked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EnvironmentRuntimeRecord {
    pub(crate) schema_version: u32,
    pub(crate) silo_id: Uuid,
    pub(crate) distribution: String,
}

pub(crate) fn environment_runtime_record_is_valid(record: &EnvironmentRuntimeRecord) -> bool {
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
    pub(crate) fn load(root: &std::path::Path) -> Self {
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

    pub(crate) fn is_active(&self, silo_id: Uuid) -> bool {
        self.activation.active_silo_id == Some(silo_id)
            && matches!(
                self.activation.state,
                RuntimeState::Preflight
                    | RuntimeState::Launching
                    | RuntimeState::Running
                    | RuntimeState::RecoveryRequired
            )
    }

    pub(crate) fn has_active_silo(&self) -> bool {
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

pub(crate) fn persist_environment_runtime_record(
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

pub(crate) fn environment_runtime_record_exists(root: &std::path::Path) -> Result<bool, String> {
    let path = root.join(ENVIRONMENT_RUNTIME_RECORD_FILE);
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "Could not inspect the Linux runtime record path: {error}"
        )),
    }
}

pub(crate) fn clear_environment_runtime_record(
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

pub(crate) fn quarantine_invalid_environment_runtime_record(
    root: &std::path::Path,
) -> Result<(), String> {
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

pub(crate) fn recover_invalid_environment_runtime_record(
    state: &DesktopCore,
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

pub(crate) fn environment_operation_available(
    status: &EnvironmentBackendStatus,
    operation: EnvironmentOperation,
) -> bool {
    status.capabilities.iter().any(|capability| {
        capability.operation == operation
            && matches!(&capability.availability, OperationAvailability::Available)
    })
}

pub(crate) fn prepare_wsl_distribution(
    state: &DesktopCore,
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

pub(crate) fn wsl_network_profile(silo: &Silo) -> Result<EnvironmentNetworkProfile, String> {
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

pub(crate) fn environment_runtime_is_active(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Result<bool, String> {
    state
        .environment_runtime
        .lock()
        .map(|runtime| runtime.is_active(silo_id))
        .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())
}

pub(crate) fn environment_runtime_has_active_silo(state: &DesktopCore) -> Result<bool, String> {
    state
        .environment_runtime
        .lock()
        .map(|runtime| runtime.has_active_silo())
        .map_err(|_| "VeriSilo environment runtime state is unavailable.".to_owned())
}

pub(crate) fn reconcile_environment_runtime_if_needed(
    state: &DesktopCore,
    silos: &[Silo],
) -> Result<(), String> {
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

pub(crate) fn stop_environment_runtime_for_vault_lock(state: &DesktopCore) {
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

pub(crate) fn detect_wsl() -> WslStatus {
    environment::detect_wsl()
}

pub(crate) fn environment_backend_statuses(
    state: &DesktopCore,
) -> Result<Vec<EnvironmentBackendStatus>, String> {
    let environments = state
        .environments
        .lock()
        .map_err(|_| "VeriSilo environment provider state is unavailable.".to_owned())?;
    Ok(environments.statuses())
}

pub(crate) fn select_wsl_environment_distribution(
    state: &DesktopCore,
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

pub(crate) fn execute_environment_backend(
    state: &DesktopCore,
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

pub(crate) fn environment_backend_execute(
    state: &DesktopCore,
    request: EnvironmentOperationRequest,
) -> Result<EnvironmentActionReceipt, String> {
    execute_environment_backend(&state, request)
}

pub(crate) fn legacy_environment_artifacts(
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

pub(crate) fn list_legacy_environment_artifacts(
    state: &DesktopCore,
) -> Result<Vec<LegacyEnvironmentArtifact>, String> {
    let _local_reservation = state.local_control.reserve()?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let silos = vault.list_silos().map_err(|error| error.to_string())?;
    legacy_environment_artifacts(&state.root, &silos)
}

pub(crate) fn cleanup_legacy_environment_artifact(
    state: &DesktopCore,
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

pub(crate) fn verified_current_wsl_artifact(
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
