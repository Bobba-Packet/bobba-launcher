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
use tauri::{AppHandle, Manager, State};
use ticket::LoginTicket;
use updater::LauncherUpdate;

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
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
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
            install_client,
            launch_client,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Bobba Launcher");
}
