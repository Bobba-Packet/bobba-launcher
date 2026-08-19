mod clients;
mod diag;
mod gearth;
mod hotels;
mod install;
mod launch;
mod platforms;
mod settings;
mod swf;
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

// ============================================================
// Platforms
// ============================================================

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PlatformInfo {
    id: platforms::Platform,
    label: String,
    blurb: String,
    needs_ticket: bool,
    needs_server_choice: bool,
    clients: Vec<clients::ClientId>,
    hotels: Vec<hotels::Hotel>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OriginsServerInfo {
    id: platforms::OriginsServer,
    label: String,
    host: String,
}

/// Everything the UI needs to render the platform selector in one call.
#[tauri::command]
fn list_platforms() -> Vec<PlatformInfo> {
    platforms::Platform::ALL
        .into_iter()
        .map(|p| PlatformInfo {
            id: p,
            label: p.label().into(),
            blurb: p.blurb().into(),
            needs_ticket: p.needs_ticket(),
            needs_server_choice: p.needs_server_choice(),
            clients: p.clients().to_vec(),
            hotels: hotels::for_platform(p),
        })
        .collect()
}

#[tauri::command]
fn list_origins_servers() -> Vec<OriginsServerInfo> {
    platforms::OriginsServer::ALL
        .into_iter()
        .map(|s| OriginsServerInfo {
            id: s,
            label: s.label().into(),
            host: s.host().into(),
        })
        .collect()
}

#[tauri::command]
fn get_platform(state: State<'_, AppState>) -> Result<platforms::Platform, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.platform)
}

/// Switching platform also moves `selected` to that platform's default client
/// when the current one belongs elsewhere, so the two can't drift apart.
#[tauri::command]
fn set_platform(
    app: AppHandle,
    state: State<'_, AppState>,
    platform: platforms::Platform,
) -> Result<clients::ClientId, String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.set_platform(platform);
    let selected = settings.selected;
    save_settings(&app, &settings)?;
    Ok(selected)
}

#[tauri::command]
fn get_origins_server(state: State<'_, AppState>) -> Result<platforms::OriginsServer, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.origins_server)
}

#[tauri::command]
fn set_origins_server(
    app: AppHandle,
    state: State<'_, AppState>,
    server: platforms::OriginsServer,
) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.origins_server = server;
    save_settings(&app, &settings)
}

#[tauri::command]
fn get_origins_xl(state: State<'_, AppState>) -> Result<bool, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.origins_xl)
}

#[tauri::command]
fn set_origins_xl(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.origins_xl = enabled;
    save_settings(&app, &settings)
}

#[tauri::command]
fn toggle_gearth(app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.g_earth.enabled = !settings.g_earth.enabled;
    let new_val = settings.g_earth.enabled;
    save_settings(&app, &settings)?;
    Ok(new_val)
}

#[tauri::command]
fn get_hidden_platforms(state: State<'_, AppState>) -> Result<Vec<platforms::Platform>, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.hidden_platforms.clone())
}

#[tauri::command]
fn set_hidden_platforms(
    app: AppHandle,
    state: State<'_, AppState>,
    hidden: Vec<platforms::Platform>,
) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.hidden_platforms = hidden;
    save_settings(&app, &settings)
}

#[tauri::command]
fn get_hidden_origins_servers(state: State<'_, AppState>) -> Result<Vec<platforms::OriginsServer>, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.hidden_origins_servers.clone())
}

#[tauri::command]
fn set_hidden_origins_servers(
    app: AppHandle,
    state: State<'_, AppState>,
    hidden: Vec<platforms::OriginsServer>,
) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.hidden_origins_servers = hidden;
    save_settings(&app, &settings)
}

#[tauri::command]
fn get_skip_update_clients(state: State<'_, AppState>) -> Result<Vec<ClientId>, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.skip_update_clients.clone())
}

#[tauri::command]
fn set_skip_update_clients(
    app: AppHandle,
    state: State<'_, AppState>,
    clients: Vec<ClientId>,
) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.skip_update_clients = clients;
    save_settings(&app, &settings)
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
    settings.set_selected_client(id);
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
fn get_minimize_to_tray(state: State<'_, AppState>) -> Result<bool, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.minimize_to_tray)
}

#[tauri::command]
fn set_minimize_to_tray(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.minimize_to_tray = enabled;
    save_settings(&app, &settings)
}

#[tauri::command]
fn get_machine_id_isolation(state: State<'_, AppState>) -> Result<bool, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.machine_id_isolation)
}

#[tauri::command]
fn set_machine_id_isolation(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.machine_id_isolation = enabled;
    save_settings(&app, &settings)
}

#[tauri::command]
fn get_auto_launch_delay(state: State<'_, AppState>) -> Result<u32, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.auto_launch_delay)
}

#[tauri::command]
fn set_auto_launch_delay(
    app: AppHandle,
    state: State<'_, AppState>,
    seconds: u32,
) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.auto_launch_delay = seconds;
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
    let previous_version = {
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        settings.version_of(id)
    };
    let (version, _path) = install::ensure_installed(&app, &root, id, &host).await?;

    if let Some(prev) = previous_version.as_ref() {
        if *prev != version {
            install::remove_install(&root, id, prev);
        }
    }

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
    perform_launch(&app, &state.settings, id, &ticket_raw).await
}

/// Shared launch path, so a pasted ticket and a saved session both go through
/// the same install-verify → SWF choice → G-Earth → spawn sequence.
///
/// Takes the settings mutex directly rather than `State`, which lets the
/// session-launch command reuse it without cloning Tauri's state guard.
async fn perform_launch(
    app: &AppHandle,
    settings_mutex: &Mutex<Settings>,
    id: ClientId,
    ticket_raw: &str,
) -> Result<(), String> {
    // Origins takes no ticket — the server is encoded in the executable name, so
    // a launch there is valid with nothing pasted at all.
    let needs_ticket = id.platform().needs_ticket();
    let ticket = if needs_ticket {
        let parsed = ticket::parse_ticket(ticket_raw).ok_or_else(|| {
            "Invalid login ticket. Paste a habbo:// link or server.ticket.V4 code.".to_string()
        })?;
        // A Habbo X ticket can't launch a regular hotel client, or vice versa.
        if parsed.platform != id.platform() {
            return Err(format!(
                "That ticket is for {}, but {} is selected.",
                parsed.platform.label(),
                id.platform().label()
            ));
        }
        Some(parsed)
    } else {
        None
    };

    let root = install::data_root(app)?;
    let host = match ticket.as_ref() {
        Some(t) => t.server_host.clone(),
        None => id.platform().gamedata_host().to_string(),
    };
    let previous_version = {
        let settings = settings_mutex.lock().map_err(|e| e.to_string())?;
        settings.version_of(id)
    };

    // Check if this client should skip auto-update.
    let skip_update = {
        let settings = settings_mutex.lock().map_err(|e| e.to_string())?;
        settings.skip_update_clients.contains(&id)
    };

    // If skip_update and we have a working install, use it directly.
    if skip_update {
        if let Some(prev) = previous_version.as_ref() {
            if let Ok(path) = install::resolve_install(&root, id, prev) {
                install::repair_if_needed(app, id, &path).await?;
                let snapshot = {
                    let settings = settings_mutex.lock().map_err(|e| e.to_string())?;
                    settings.clone()
                };
                let app_for_task = app.clone();
                return tokio::task::spawn_blocking(move || {
                    launch::launch(&app_for_task, id, &path, ticket.as_ref(), &snapshot)
                })
                .await
                .map_err(|e| format!("Launch task panicked: {e}"))?;
            }
        }
    }

    // Verify remote build; download if needed; fall back to local on network failure.
    let client_path = match install::ensure_installed(app, &root, id, &host).await {
        Ok((version, path)) => {
            if let Some(prev) = previous_version.as_ref() {
                if *prev != version {
                    install::remove_install(&root, id, prev);
                }
            }
            let mut settings = settings_mutex.lock().map_err(|e| e.to_string())?;
            settings.set_version(id, version);
            save_settings(app, &settings)?;
            path
        }
        Err(update_err) => {
            let Some(prev) = previous_version.as_ref() else {
                return Err(update_err);
            };
            match install::resolve_install(&root, id, prev) {
                Ok(path) => path,
                Err(_) => {
                    let maybe = install::client_dir(&root, id, prev)?;
                    if install::repair_if_needed(app, id, &maybe).await.is_ok() {
                        maybe
                    } else {
                        return Err(update_err);
                    }
                }
            }
        }
    };

    // Always refresh XML staging + extensions before spawn (AIR only; other
    // kinds are just verified).
    install::repair_if_needed(app, id, &client_path).await?;

    // Snapshot settings so the (blocking) launch path never holds the mutex.
    let snapshot = {
        let settings = settings_mutex.lock().map_err(|e| e.to_string())?;
        settings.clone()
    };

    // The launch path blocks: a 1s settle plus up to 30s polling for G-Earth's
    // proxy port. Running that inline would stall the async runtime and freeze
    // the window, so hand it to a blocking worker.
    let app_for_task = app.clone();
    tokio::task::spawn_blocking(move || {
        launch::launch(
            &app_for_task,
            id,
            &client_path,
            ticket.as_ref(),
            &snapshot,
        )
    })
    .await
    .map_err(|e| format!("Launch task panicked: {e}"))?
}

// ============================================================
// G-Earth / custom SWF
// ============================================================

#[tauri::command]
fn get_gearth_settings(state: State<'_, AppState>) -> Result<settings::GEarthSettings, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.g_earth.clone())
}

#[tauri::command]
fn set_gearth_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    value: settings::GEarthSettings,
) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.g_earth = value;
    save_settings(&app, &settings)
}

#[tauri::command]
fn get_custom_swf_settings(
    state: State<'_, AppState>,
) -> Result<settings::CustomSwfSettings, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.custom_swf.clone())
}

#[tauri::command]
fn set_custom_swf_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    value: settings::CustomSwfSettings,
) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.custom_swf = value;
    save_settings(&app, &settings)
}

/// Downloads the configured custom SWF into the shared cache, and mirrors it
/// into the currently selected client install when one exists.
#[tauri::command]
async fn download_custom_swf(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let (link, selected, version) = {
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        (
            settings.custom_swf.link.clone(),
            settings.selected,
            settings.version_of(settings.selected),
        )
    };

    let root = install::data_root(&app)?;
    let client_dir = version.and_then(|v| install::client_dir(&root, selected, &v).ok());
    swf::download_custom_swf(&app, &link, client_dir.as_deref()).await
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

            // State must exist before the close handler is installed, because the
            // handler reads the minimize-to-tray preference out of it.
            let settings = load_settings(app.handle());
            app.manage(AppState {
                settings: Mutex::new(settings),
            });

            // Closing the window hides to tray by default so clipboard
            // ticket-watching keeps working. With minimizeToTray off, closing
            // quits instead.
            if let Some(window) = app.get_webview_window("main") {
                let win = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        let handle = win.app_handle();
                        let to_tray = handle
                            .state::<AppState>()
                            .settings
                            .lock()
                            .map(|s| s.minimize_to_tray)
                            .unwrap_or(true);

                        if to_tray {
                            api.prevent_close();
                            let _ = win.hide();
                        } else {
                            // Exit explicitly: the tray icon would otherwise keep
                            // the process alive after the last window closed.
                            handle.exit(0);
                        }
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_hotels,
            list_clients,
            // platforms
            list_platforms,
            list_origins_servers,
            get_platform,
            set_platform,
            get_origins_server,
            set_origins_server,
            get_origins_xl,
            set_origins_xl,
            toggle_gearth,
            get_hidden_platforms,
            set_hidden_platforms,
            get_hidden_origins_servers,
            set_hidden_origins_servers,
            get_skip_update_clients,
            set_skip_update_clients,
            get_selected,
            get_default_hotel,
            set_selected,
            set_default_hotel,
            get_auto_download_updates,
            set_auto_download_updates,
            get_minimize_to_tray,
            set_minimize_to_tray,
            get_machine_id_isolation,
            set_machine_id_isolation,
            get_auto_launch_delay,
            set_auto_launch_delay,
            get_launcher_version,
            check_launcher_update,
            download_launcher_update,
            parse_login_ticket,
            get_startup_ticket,
            show_launcher,
            install_client,
            launch_client,
            // G-Earth + custom SWF
            get_gearth_settings,
            set_gearth_settings,
            get_custom_swf_settings,
            set_custom_swf_settings,
            download_custom_swf,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Bobba Launcher");
}
