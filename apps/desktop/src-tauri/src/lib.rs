use domain::{select_vault_name, DEFAULT_VAULT_NAME};
use std::{fs, sync::Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

pub mod domain;
pub mod engine;
pub mod environment;
pub mod launcher;
pub mod local_api;
pub mod mihomo;
pub mod native_host;
pub mod proxy_relay;
mod runtime_watchdog;
pub mod vault;
pub mod website_identity;

mod application;
mod commands;

pub struct AppState {
    core: application::DesktopCore,
    _vault_instance: local_api::VaultInstanceGuard,
    local_api: Mutex<Option<(local_api::LocalApiServer, String)>>,
}

impl Drop for AppState {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.local_api.lock() {
            if let Some((mut server, _)) = slot.take() {
                server.shutdown();
            }
        }
    }
}

const TRAY_OPEN_ID: &str = "tray-open";

const TRAY_EXIT_ID: &str = "tray-exit";

fn ensure_main_window<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<WebviewWindow<R>> {
    if let Some(window) = app.get_webview_window("main") {
        return Ok(window);
    }
    let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("VeriSilo")
        .inner_size(1120.0, 780.0)
        .min_inner_size(760.0, 620.0)
        .resizable(true)
        .visible(false)
        .build()?;
    let window_to_hide = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = window_to_hide.hide();
        }
    });
    Ok(window)
}

pub(crate) fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Ok(window) = ensure_main_window(app) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn exit_from_tray<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let local_reservation = match state.core.local_control.reserve() {
        Ok(reservation) => reservation,
        Err(_) => {
            show_main_window(app);
            return;
        }
    };
    let mut runtime = match state.core.runtime.lock() {
        Ok(runtime) => runtime,
        Err(_) => {
            show_main_window(app);
            return;
        }
    };
    let can_exit = match runtime.active_managed_camoufox_silo_id() {
        Some(silo_id) => runtime.stop_managed_camoufox(silo_id).is_ok(),
        None => true,
    };
    drop(runtime);
    drop(local_reservation);
    if can_exit {
        let _ = native_host::clear_runtime_status_snapshot(&state.core.root);
        if let Ok(path) = local_api::discovery_path() {
            let _ = fs::remove_file(path);
        }
        app.exit(0);
    } else {
        show_main_window(app);
    }
}

fn is_tray_primary_activation(button: MouseButton, button_state: MouseButtonState) -> bool {
    button == MouseButton::Left && button_state == MouseButtonState::Up
}

pub fn run() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let cli_background = args.iter().any(|arg| arg == "--cli-background");
    let vault_name = match startup_vault_name(&args) {
        Ok(name) => name,
        Err(error) => {
            eprintln!("{error}");
            return;
        }
    };
    if !cli_background && local_api::request_existing_app_open() {
        return;
    }
    tauri::Builder::default()
        .setup(move |app| {
            let resource_root = app
                .path()
                .resource_dir()
                .map_err(|error| format!("VeriSilo resource directory is unavailable: {error}"))?;
            let guard = local_api::VaultInstanceGuard::acquire(&vault_name)?;
            app.manage(AppState {
                core: application::DesktopCore::open(
                    domain::app_data_root()
                        .expect("VeriSilo needs a writable local application data directory"),
                    resource_root,
                ),
                _vault_instance: guard,
                local_api: Mutex::new(None),
            });
            if let Ok((server, url)) = local_api::spawn(app.handle().clone()) {
                if let Ok(mut slot) = app.state::<AppState>().local_api.lock() {
                    *slot = Some((server, url));
                }
            }
            if cli_background && vault_name != DEFAULT_VAULT_NAME {
                return Ok(());
            }

            let open_item =
                MenuItem::with_id(app, TRAY_OPEN_ID, "打开 VeriSilo", true, None::<&str>)?;
            let exit_item =
                MenuItem::with_id(app, TRAY_EXIT_ID, "退出 VeriSilo", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&open_item, &exit_item])?;
            let tray_icon = app
                .default_window_icon()
                .cloned()
                .ok_or("VeriSilo tray icon is unavailable")?;
            if !cli_background {
                show_main_window(app.handle());
            }

            TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .tooltip("VeriSilo")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    TRAY_OPEN_ID => show_main_window(app),
                    TRAY_EXIT_ID => exit_from_tray(app),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button,
                        button_state,
                        ..
                    } = event
                    {
                        if is_tray_primary_activation(button, button_state) {
                            show_main_window(tray.app_handle());
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::desktop_status,
            commands::local_api_info,
            commands::initialize_vault,
            commands::unlock_vault,
            commands::lock_vault,
            commands::change_vault_passphrase,
            commands::backup_vault,
            commands::restore_vault,
            commands::discover_browsers,
            commands::list_engine_adapters,
            commands::install_engine_package,
            commands::update_engine_package,
            commands::rollback_engine_package,
            commands::set_engine_emergency_disabled,
            commands::remote_environment_status,
            commands::validate_remote_environment_endpoint,
            commands::pair_remote_environment,
            commands::rotate_remote_environment_tls_pin,
            commands::revoke_remote_pairing,
            commands::force_detach_remote_environment,
            commands::remote_environment_create,
            commands::remote_environment_start,
            commands::remote_environment_stop,
            commands::remote_environment_pause,
            commands::remote_environment_snapshot,
            commands::remote_environment_destroy,
            commands::remote_environment_configure_network,
            commands::remote_environment_health,
            commands::remote_environment_logs,
            commands::remote_environment_open_human_session,
            commands::remote_environment_close_human_session,
            commands::remote_environment_grant_automation,
            commands::remote_environment_revoke_automation,
            commands::remote_environment_open_screen,
            commands::remote_environment_send_input,
            commands::detect_wsl,
            commands::environment_backend_statuses,
            commands::select_wsl_environment_distribution,
            commands::environment_backend_execute,
            commands::list_legacy_environment_artifacts,
            commands::cleanup_legacy_environment_artifact,
            commands::inspect_mihomo_controller,
            commands::probe_local_clash,
            commands::list_silos,
            commands::list_active_silos,
            commands::list_archived_silos,
            commands::create_managed_silo,
            commands::list_managed_identity_previews,
            commands::update_managed_identity,
            commands::create_silo,
            commands::update_silo,
            commands::update_silo_configuration,
            commands::rename_silo,
            commands::update_silo_network,
            commands::update_silo_engine,
            commands::archive_silo,
            commands::restore_archived_silo,
            commands::delete_silo,
            commands::silo_storage_usage,
            commands::list_network_evidence,
            commands::clear_network_evidence,
            commands::recheck_silo_browser,
            commands::recheck_silo_runtime,
            commands::stop_silo,
            commands::rebind_silo_mihomo,
            commands::launch_silo,
        ])
        .run(tauri::generate_context!())
        .expect("error while running VeriSilo");
}

fn startup_vault_name(args: &[String]) -> Result<String, String> {
    let mut name = DEFAULT_VAULT_NAME.to_owned();
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--vault" {
            name = args
                .get(index + 1)
                .cloned()
                .ok_or_else(|| "--vault 后面需要 Vault 名称。".to_owned())?;
            index += 1;
        }
        index += 1;
    }
    select_vault_name(&name).map_err(|error| error.to_string())?;
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tray_primary_activation_is_a_completed_left_click() {
        assert!(is_tray_primary_activation(
            MouseButton::Left,
            MouseButtonState::Up
        ));
        assert!(!is_tray_primary_activation(
            MouseButton::Left,
            MouseButtonState::Down
        ));
        assert!(!is_tray_primary_activation(
            MouseButton::Right,
            MouseButtonState::Up
        ));
    }
}
