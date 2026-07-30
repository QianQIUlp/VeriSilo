//! Tauri-free compilation root for the production desktop core modules.
//!
//! Keep these as path modules: the harness must compile and test the exact
//! production sources instead of maintaining test-only copies. `environment`
//! loads its production `environment_backend.rs` module from the same source
//! directory.

#[path = "../../../apps/desktop/src-tauri/src/domain.rs"]
pub mod domain;
#[path = "../../../apps/desktop/src-tauri/src/engine.rs"]
pub mod engine;
#[path = "../../../apps/desktop/src-tauri/src/environment.rs"]
pub mod environment;
#[path = "../../../apps/desktop/src-tauri/src/launcher.rs"]
pub mod launcher;
#[path = "../../../apps/desktop/src-tauri/src/mihomo.rs"]
pub mod mihomo;
#[path = "../../../apps/desktop/src-tauri/src/native_host.rs"]
pub mod native_host;
#[path = "../../../apps/desktop/src-tauri/src/proxy_relay.rs"]
pub mod proxy_relay;
#[path = "../../../apps/desktop/src-tauri/src/runtime_watchdog.rs"]
pub mod runtime_watchdog;
#[path = "../../../apps/desktop/src-tauri/src/vault.rs"]
pub mod vault;

/// Compile-time anchor for Vault methods that are consumed by the Tauri
/// command layer, which this core-only crate intentionally does not include.
#[doc(hidden)]
pub fn compile_tauri_vault_control_plane(
    runtime: &mut vault::VaultRuntime,
    root: &std::path::Path,
) -> Result<(), vault::VaultError> {
    let state = runtime.remote_control_plane()?;
    runtime.persist_remote_control_plane(root, state)
}

/// Compile-time anchor for the native runtime watchdog, whose production
/// owner lives in the Tauri AppState omitted from this harness.
#[doc(hidden)]
pub fn compile_native_runtime_watchdog(
    runtime: &std::sync::Arc<std::sync::Mutex<launcher::RuntimeManager>>,
) -> std::io::Result<()> {
    drop(runtime_watchdog::RuntimeWatchdog::start(runtime)?);
    Ok(())
}

/// Compile-time anchor for the quiescent runtime ownership transition used by
/// the Tauri Vault restore command omitted from this harness.
#[doc(hidden)]
pub fn compile_vault_restore_runtime_epoch(
    runtime: &mut launcher::RuntimeManager,
) -> Option<domain::RuntimeActivation> {
    let preparation = runtime.prepare_for_vault_restore()?;
    Some(runtime.complete_successful_vault_restore(preparation))
}
