use super::environments::stop_environment_runtime_for_vault_lock;
use super::runtime::{publish_runtime_status, reconcile_runtime_if_possible};
use super::DesktopCore;
use crate::domain::{VaultLockState, VaultStatus};
use crate::launcher::managed_profiles_are_quiescent_for_vault_restore;
use crate::vault::VaultBackupReceipt;
use std::path::PathBuf;

pub(crate) fn initialize_vault(
    state: &DesktopCore,
    passphrase: String,
) -> Result<VaultStatus, String> {
    initialize_vault_with(&state, &passphrase)
}

pub(crate) fn initialize_vault_with(
    state: &DesktopCore,
    passphrase: &str,
) -> Result<VaultStatus, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault
        .initialize(&state.root, passphrase)
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

pub(crate) fn unlock_vault(state: &DesktopCore, passphrase: String) -> Result<VaultStatus, String> {
    unlock_vault_with(&state, &passphrase)
}

pub(crate) fn unlock_vault_with(
    state: &DesktopCore,
    passphrase: &str,
) -> Result<VaultStatus, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault
        .unlock(&state.root, passphrase)
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

pub(crate) fn lock_vault(state: &DesktopCore) -> Result<VaultStatus, String> {
    lock_vault_with(&state)
}

pub(crate) fn lock_vault_with(state: &DesktopCore) -> Result<VaultStatus, String> {
    let _local_reservation = state.local_control.reserve()?;
    let _remote_guard = state
        .remote_control
        .lock()
        .map_err(|_| "VeriSilo remote control state is unavailable.".to_owned())?;
    stop_environment_runtime_for_vault_lock(state);
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
    publish_runtime_status(state, &activation, &vault_status);
    Ok(vault_status)
}

pub(crate) fn change_vault_passphrase(
    state: &DesktopCore,
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

pub(crate) fn backup_vault(
    state: &DesktopCore,
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

pub(crate) fn restore_vault(
    state: &DesktopCore,
    source_path: String,
    passphrase: String,
    confirm_overwrite: bool,
) -> Result<VaultStatus, String> {
    // Restore is a global ownership transition. Reserve local and remote
    // lifecycle planes before Vault → Runtime → Environments so all in-flight
    // provider work finishes and no new work can race the replacement.
    let _local_reservation = state.local_control.reserve()?;
    let _remote_guard = state
        .remote_control
        .lock()
        .map_err(|_| "VeriSilo remote control state is unavailable.".to_owned())?;
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
