use std::{path::PathBuf, sync::Mutex};

use serde::Serialize;
use tauri::{Manager, State};
use uuid::Uuid;

pub mod domain;
pub mod launcher;
pub mod native_host;
pub mod vault;

use domain::{
    app_data_root, discover_browsers as discover_installed_browsers, BrowserCandidate,
    CreateSiloInput, RuntimeActivation, Silo, VaultStatus,
};
use launcher::{profile_in_use, RuntimeManager};
use vault::VaultRuntime;

pub struct AppState {
    root: PathBuf,
    vault: Mutex<VaultRuntime>,
    runtime: Mutex<RuntimeManager>,
}

impl AppState {
    fn new() -> Self {
        let root =
            app_data_root().expect("VeriSilo needs a writable local application data directory");
        Self {
            root,
            vault: Mutex::new(VaultRuntime::default()),
            runtime: Mutex::new(RuntimeManager::default()),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopStatus {
    vault: VaultStatus,
    activation: RuntimeActivation,
}

#[tauri::command]
fn desktop_status(state: State<'_, AppState>) -> Result<DesktopStatus, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let activation = runtime.activation();
    Ok(DesktopStatus {
        vault: vault.status(&state.root),
        activation,
    })
}

#[tauri::command]
fn initialize_vault(state: State<'_, AppState>, passphrase: String) -> Result<VaultStatus, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault
        .initialize(&state.root, &passphrase)
        .map_err(|error| error.to_string())?;
    Ok(vault.status(&state.root))
}

#[tauri::command]
fn unlock_vault(state: State<'_, AppState>, passphrase: String) -> Result<VaultStatus, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault
        .unlock(&state.root, &passphrase)
        .map_err(|error| error.to_string())?;
    Ok(vault.status(&state.root))
}

#[tauri::command]
fn lock_vault(state: State<'_, AppState>) -> Result<VaultStatus, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.lock();
    Ok(vault.status(&state.root))
}

#[tauri::command]
fn discover_browsers() -> Vec<BrowserCandidate> {
    discover_installed_browsers()
}

#[tauri::command]
fn list_silos(state: State<'_, AppState>) -> Result<Vec<Silo>, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.list_silos().map_err(|error| error.to_string())
}

#[tauri::command]
fn create_silo(state: State<'_, AppState>, input: CreateSiloInput) -> Result<Silo, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault
        .create_silo(&state.root, input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn archive_silo(state: State<'_, AppState>, silo_id: Uuid) -> Result<(), String> {
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let is_active = runtime.is_active(silo_id);
    drop(runtime);
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
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

#[tauri::command]
fn launch_silo(state: State<'_, AppState>, silo_id: Uuid) -> Result<RuntimeActivation, String> {
    let (silo, managed_profile_directories) = {
        let mut vault = state
            .vault
            .lock()
            .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
        let silo = vault.get_silo(silo_id).map_err(|error| error.to_string())?;
        let managed_profile_directories = vault
            .managed_profile_directories()
            .map_err(|error| error.to_string())?;
        (silo, managed_profile_directories)
    };
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    runtime
        .launch(&silo, &managed_profile_directories)
        .map_err(|error| error.to_string())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            desktop_status,
            initialize_vault,
            unlock_vault,
            lock_vault,
            discover_browsers,
            list_silos,
            create_silo,
            archive_silo,
            launch_silo,
        ])
        .run(tauri::generate_context!())
        .expect("error while running VeriSilo");
}
