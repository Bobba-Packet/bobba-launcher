mod clients;
mod hotels;
mod install;
mod launch;
mod settings;
mod ticket;
mod updater;

use clients::{ClientId, ClientStatus};
use settings::Settings;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use ticket::LoginTicket;
use updater::LauncherUpdate;

fn extract_habbo_url(args: &[String]) -> Option<String> {
    args.iter()
        .find(|arg| arg.trim_start_matches('"').starts_with("habbo://"))
        .map(|arg| arg.trim_matches('"').to_string())
}

fn focus_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[tauri::command]
fn show_launcher(app: AppHandle) {
    focus_main_window(&app);
}

fn emit_habbo_url(app: &AppHandle, url: String) {
    let _ = app.emit("habbo-deep-link", url);
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show_i = MenuItem::with_id(app, "show", "Open Launcher", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

    let icon = app
        .default_window_icon()
        .ok_or("missing window icon")?
        .clone();

    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Bobba Launcher")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => focus_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                focus_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

struct AppState {
    settings: Mutex<Settings>,
}

fn settings_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

fn load_settings(app: &AppHandle) -> Settings {
    match settings_path(app) {
        Ok(p) => Settings::load(&p),
        Err(_) => Settings::default(),
    }
}

fn save_settings(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    settings.save(&settings_path(app)?)
}

#[tauri::command]
fn list_hotels() -> Vec<hotels::Hotel> {
    hotels::hotels()
}

#[tauri::command]
fn list_clients(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<ClientStatus>, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    let root = install::data_root(&app)?;
    Ok(clients::statuses(&root, &settings.versions))
}

#[tauri::command]
fn get_selected(state: State<'_, AppState>) -> Result<ClientId, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.selected)
}

#[tauri::command]
fn get_default_hotel(state: State<'_, AppState>) -> Result<String, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.default_hotel_host.clone())
}

#[tauri::command]
fn set_selected(
    app: AppHandle,
    state: State<'_, AppState>,
    id: ClientId,
) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.selected = id;
    save_settings(&app, &settings)
}

#[tauri::command]
fn set_default_hotel(
    app: AppHandle,
    state: State<'_, AppState>,
    host: String,
) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.default_hotel_host = host.trim().to_string();
    save_settings(&app, &settings)
}

#[tauri::command]
fn get_auto_download_updates(state: State<'_, AppState>) -> Result<bool, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.auto_download_updates)
}

#[tauri::command]
fn set_auto_download_updates(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.auto_download_updates = enabled;
    save_settings(&app, &settings)
}

#[tauri::command]
fn get_launcher_version(app: AppHandle) -> String {
    updater::current_version(&app)
}

#[tauri::command]
async fn check_launcher_update(app: AppHandle) -> Result<Option<LauncherUpdate>, String> {
    let current = updater::current_version(&app);
    updater::check_for_update(&current).await
}

#[tauri::command]
async fn download_launcher_update(app: AppHandle, update: LauncherUpdate) -> Result<(), String> {
    updater::download_and_install(&app, &update).await
}

#[tauri::command]
fn parse_login_ticket(raw: String) -> Option<LoginTicket> {
    ticket::parse_ticket(&raw)
}

/// habbo:// URL the launcher was started with (protocol handler invocation), if any.
#[tauri::command]
fn get_startup_ticket() -> Option<String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    extract_habbo_url(&args)
}

#[tauri::command]
async fn install_client(
    app: AppHandle,
    state: State<'_, AppState>,
    id: ClientId,
    hotel_host: Option<String>,
) -> Result<ClientStatus, String> {
    let host = {
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        hotel_host.unwrap_or_else(|| settings.default_hotel_host.clone())
    };

    let root = install::data_root(&app)?;
    let (version, _path) = install::ensure_installed(&app, &root, id, &host).await?;

    let settings_snapshot = {
        let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
        settings.set_version(id, version.clone());
        settings.default_hotel_host = host;
        save_settings(&app, &settings)?;
        settings.clone()
    };

    Ok(clients::status_of(id, &root, &settings_snapshot.versions))
}

#[tauri::command]
async fn launch_client(
    app: AppHandle,
    state: State<'_, AppState>,
    id: ClientId,
    ticket_raw: String,
) -> Result<(), String> {
    let ticket = ticket::parse_ticket(&ticket_raw)
        .ok_or_else(|| "Invalid login ticket. Paste a habbo:// link or server.ticket.V4 code.".to_string())?;

    if !id.supported() {
        return Err("AirBobba is not available yet".into());
    }

    let root = install::data_root(&app)?;
    let host = ticket.server_host.clone();

    // Ensure client exists / is updated using the ticket's hotel for Classic
    let version = {
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        settings.version_of(id)
    };

    let client_path = match version {
        Some(v) => match install::resolve_install(&root, id, &v) {
            Ok(p) => p,
            Err(_) => {
                // Try repairing incomplete AirPlus installs (extensions missing)
                let maybe = install::client_dir(&root, id, &v)?;
                if install::repair_if_needed(&app, &maybe).await.is_ok() {
                    maybe
                } else {
                    let (v, p) = install::ensure_installed(&app, &root, id, &host).await?;
                    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
                    settings.set_version(id, v);
                    save_settings(&app, &settings)?;
                    p
                }
            }
        },
        None => {
            let (v, p) = install::ensure_installed(&app, &root, id, &host).await?;
            let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
            settings.set_version(id, v);
            save_settings(&app, &settings)?;
            p
        }
    };

    // Always refresh XML staging + extensions before spawn
    install::repair_if_needed(&app, &client_path).await?;
    launch::launch(&client_path, &ticket)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be early so a second habbo:// launch is forwarded here
        // instead of opening another window.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            focus_main_window(app);
            if let Some(url) = extract_habbo_url(&argv) {
                emit_habbo_url(app, url);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            // Register habbo:// for the current exe so links work from the
            // first run onwards, even in dev or portable installs.
            #[cfg(desktop)]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let _ = app.deep_link().register_all();

                // Cold-start / OS delivery of deep links (macOS / some Windows paths)
                let handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        let s = url.as_str().to_string();
                        if s.starts_with("habbo://") {
                            emit_habbo_url(&handle, s);
                        }
                    }
                });

                setup_tray(app)?;
            }

            // Closing the window hides to tray so clipboard watching continues.
            if let Some(window) = app.get_webview_window("main") {
                let win = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                    }
                });
            }

            let settings = load_settings(app.handle());
            app.manage(AppState {
                settings: Mutex::new(settings),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_hotels,
            list_clients,
            get_selected,
            get_default_hotel,
            set_selected,
            set_default_hotel,
            get_auto_download_updates,
            set_auto_download_updates,
            get_launcher_version,
            check_launcher_update,
            download_launcher_update,
            parse_login_ticket,
            get_startup_ticket,
            show_launcher,
            install_client,
            launch_client,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Bobba Launcher");
}
