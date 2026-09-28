use super::environments::environment_runtime_is_active;
use super::runtime::publish_runtime_status;
use super::DesktopCore;
use crate::domain::{RuntimeActivation, Silo};
use crate::engine::{production_engine_adapter_for_silo, EngineHealthState};
use crate::managed_backup::{
    ManagedSiloBackupInput, ManagedSiloBackupInspection, ManagedSiloBackupReceipt,
    ManagedSiloBackupSourceInput, ManagedSiloRestoreInput,
};
use crate::vault::VaultRuntime;
use std::path::Path;
use uuid::Uuid;

fn engine_version(vault: &mut VaultRuntime, id: Uuid) -> Result<String, String> {
    let silo = vault
        .list_silos()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|silo| silo.id == id)
        .ok_or_else(|| "Silo not found.".to_owned())?;
    let adapter = production_engine_adapter_for_silo(&silo.engine, None)
        .map_err(|error| error.to_string())?;
    if adapter.health().state != EngineHealthState::Healthy {
        return Err(
            "A healthy, compatible Camoufox engine package is required for this backup.".to_owned(),
        );
    }
    Ok(adapter.descriptor().engine_version)
}

fn ensure_stopped(state: &DesktopCore, id: Uuid) -> Result<(), String> {
    if state.local_runtimes.is_in_use(id) || environment_runtime_is_active(state, id)? {
        return Err("Stop the Managed Silo before backing up or restoring it.".to_owned());
    }
    Ok(())
}

pub(crate) fn backup_managed_silo(
    state: &DesktopCore,
    id: Uuid,
    input: ManagedSiloBackupInput,
) -> Result<ManagedSiloBackupReceipt, String> {
    let _reservation = state.local_control.reserve_silo(id)?;
    ensure_stopped(state, id)?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let version = engine_version(&mut vault, id)?;
    vault
        .backup_managed_silo(
            &state.root,
            id,
            Path::new(&input.destination_path),
            &input.passphrase,
            &version,
        )
        .map_err(|error| error.to_string())
}

pub(crate) fn inspect_managed_silo_backup(
    state: &DesktopCore,
    id: Uuid,
    input: ManagedSiloBackupSourceInput,
) -> Result<ManagedSiloBackupInspection, String> {
    let _reservation = state.local_control.reserve_silo(id)?;
    ensure_stopped(state, id)?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let version = engine_version(&mut vault, id)?;
    vault
        .inspect_managed_silo_backup(
            &state.root,
            id,
            Path::new(&input.source_path),
            &input.passphrase,
            &version,
        )
        .map_err(|error| error.to_string())
}

pub(crate) fn restore_managed_silo_backup(
    state: &DesktopCore,
    id: Uuid,
    input: ManagedSiloRestoreInput,
) -> Result<Silo, String> {
    let _reservation = state.local_control.reserve_silo(id)?;
    ensure_stopped(state, id)?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let version = engine_version(&mut vault, id)?;
    let restored = vault
        .restore_managed_silo_backup(
            &state.root,
            id,
            Path::new(&input.source_path),
            &input.passphrase,
            &input.expected_archive_sha256,
            input.confirm_overwrite,
            &version,
        )
        .map_err(|error| error.to_string())?;
    let status = vault.status(&state.root);
    drop(vault);
    let activation = if let Some(handle) = state.local_runtimes.get(id) {
        let mut runtime = handle.lock()
            .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
        runtime.invalidate_restored_silo(id)
    } else {
        RuntimeActivation::idle()
    };
    state.local_runtimes.update(id, activation.clone(), None);
    publish_runtime_status(state, &activation, &status);
    Ok(restored)
}
