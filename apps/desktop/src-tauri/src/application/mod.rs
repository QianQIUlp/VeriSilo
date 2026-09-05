//! Shared desktop operations. Tauri commands and the local API call this layer.

use crate::environment::EnvironmentManager;
use crate::launcher::RuntimeManager;
use crate::native_host;
use crate::runtime_watchdog::RuntimeWatchdog;
use crate::vault::VaultRuntime;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

mod environments;
pub(crate) use environments::{
    cleanup_legacy_environment_artifact, detect_wsl, environment_backend_execute,
    environment_backend_statuses, list_legacy_environment_artifacts,
    select_wsl_environment_distribution, EnvironmentRuntimeState, LegacyEnvironmentArtifact,
    LocalEnvironmentControl,
};

mod runtime;
pub(crate) use runtime::{
    desktop_status, desktop_status_with, launch_silo_with, rebind_silo_mihomo,
    recheck_silo_browser, recheck_silo_runtime, stop_silo_with, DesktopStatus,
};

mod silos;
pub(crate) use silos::{
    archive_silo, clear_network_evidence, create_silo, delete_silo, delete_silo_with,
    diagnose_silo_with, list_active_silos, list_archived_silos, list_network_evidence, list_silos,
    list_silos_with, page_action_with, rename_silo, restore_archived_silo, silo_storage_usage,
    update_silo, update_silo_configuration, update_silo_engine, update_silo_network,
};

mod engines;
pub(crate) use engines::{
    discover_browsers, install_engine_package, list_engine_adapters, rollback_engine_package,
    set_engine_emergency_disabled, update_engine_package, EngineAdapterStatus,
};

mod remote;
pub(crate) use remote::{
    force_detach_remote_environment, pair_remote_environment,
    remote_environment_close_human_session, remote_environment_configure_network,
    remote_environment_create, remote_environment_destroy, remote_environment_grant_automation,
    remote_environment_health, remote_environment_logs, remote_environment_open_human_session,
    remote_environment_open_screen, remote_environment_pause, remote_environment_revoke_automation,
    remote_environment_send_input, remote_environment_snapshot, remote_environment_start,
    remote_environment_status, remote_environment_stop, revoke_remote_pairing,
    rotate_remote_environment_tls_pin, validate_remote_environment_endpoint,
    RemoteEnvironmentStatus,
};

mod vault;
pub(crate) use vault::{
    backup_vault, change_vault_passphrase, initialize_vault, initialize_vault_with, lock_vault,
    lock_vault_with, restore_vault, unlock_vault, unlock_vault_with,
};

mod network;
pub(crate) use network::{inspect_mihomo_controller, probe_local_clash};

mod identity;
pub(crate) use identity::{
    create_managed_silo, create_managed_silo_with, list_managed_identity_previews,
    update_managed_identity,
};

pub(crate) struct DesktopCore {
    pub(crate) root: PathBuf,
    pub(crate) resource_root: PathBuf,
    // Local stock/provider lifecycle operations first reserve local_control,
    // then acquire Vault → Runtime → Environments when each is needed. A
    // command may drop an inner guard but must never reacquire an earlier one.
    // The reservation stays held across slow provider/launcher completion.
    pub(crate) local_control: LocalEnvironmentControl,
    pub(crate) vault: Mutex<VaultRuntime>,
    pub(crate) runtime: Arc<Mutex<RuntimeManager>>,
    pub(crate) runtime_watchdog: RuntimeWatchdog,
    pub(crate) environments: Mutex<EnvironmentManager>,
    pub(crate) environment_runtime: Mutex<EnvironmentRuntimeState>,
    // Remote lifecycle exchanges are serialized. Commands hold this guard
    // before Vault so a user lock/revocation linearizes after any in-flight
    // request and no later request can reuse the dropped credential.
    pub(crate) remote_control: Mutex<()>,
}

impl DesktopCore {
    pub(crate) fn open(root: PathBuf, resource_root: PathBuf) -> Self {
        let runtime = Arc::new(Mutex::new(RuntimeManager::open(&root)));
        let runtime_watchdog =
            RuntimeWatchdog::start(&runtime).expect("VeriSilo needs a native runtime watchdog");
        let environments = EnvironmentManager::new(root.clone(), resource_root.clone())
            .expect("VeriSilo needs valid fixed environment provider roots");
        let environment_runtime = EnvironmentRuntimeState::load(&root);
        Self {
            root,
            resource_root,
            local_control: LocalEnvironmentControl::default(),
            vault: Mutex::new(VaultRuntime::default()),
            runtime,
            runtime_watchdog,
            environments: Mutex::new(environments),
            environment_runtime: Mutex::new(environment_runtime),
            remote_control: Mutex::new(()),
        }
    }
}

impl Drop for DesktopCore {
    fn drop(&mut self) {
        self.runtime_watchdog.shutdown();
        let _ = native_host::clear_runtime_status_snapshot(&self.root);
    }
}

#[cfg(test)]
mod tests;
