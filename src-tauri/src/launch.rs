//! Process launch — `Habbo.exe -server <id> -ticket <sso>`.
//!
//! Order of operations mirrors HabboLauncher's `Launcher.LaunchFlashClient`:
//!
//!   1. Normalise the AIR manifest (META-INF/AIR is the source of truth, the
//!      staged root copy is removed).
//!   2. Pick the SWF — stock or custom.
//!   3. Stock only: rewrite the AIR application id to
//!      `com.sulake.habboair.<avatarId>` so several clients can run at once.
//!      Custom SWFs are built against the stock identity and break if we do
//!      this, so they keep the original manifest.
//!   4. Sleep ~1s. The AIR runtime reads the manifest and SWF at startup and
//!      races the writes above if we spawn immediately.
//!   5. Optionally start G-Earth and wait for its proxy port to come up.
//!   6. Spawn the client, then tie the two process lifetimes together.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use tauri::AppHandle;

use crate::clients::{ClientId, ClientKind};
use crate::diag::report;
use crate::gearth;
use crate::settings::Settings;
use crate::swf;
use crate::ticket::LoginTicket;

/// Windows: detach so the client outlives the invoking command.
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x0000_0008;

/// Grace period between rewriting the manifest/SWF and starting the runtime.
const PRE_LAUNCH_SETTLE: Duration = Duration::from_millis(1000);

/// Entry point. Dispatches to the launch convention the client actually uses.
pub fn launch(
    app: &AppHandle,
    id: ClientId,
    client_dir: &Path,
    ticket: Option<&LoginTicket>,
    settings: &Settings,
) -> Result<(), String> {
    match id.kind() {
        ClientKind::Air => {
            let ticket = ticket.ok_or_else(|| "This client needs a login ticket".to_string())?;
            launch_air(app, client_dir, ticket, settings)
        }
        ClientKind::Unity => {
            let ticket = ticket.ok_or_else(|| "This client needs a login ticket".to_string())?;
            launch_unity(app, id, client_dir, ticket)
        }
        // Origins takes no ticket at all — the server is in the exe name.
        ClientKind::Shockwave => launch_origins(app, client_dir, settings),
    }
}

/// Unity builds (regular Unity client and Habbo X) run straight from
/// `StandaloneWindows` with the same `-server`/`-ticket` pair the AIR client uses.
/// Nothing in the bundle needs patching.
fn launch_unity(
    app: &AppHandle,
    id: ClientId,
    client_dir: &Path,
    ticket: &LoginTicket,
) -> Result<(), String> {
    let exe = crate::install::unity_exe(client_dir);
    if !exe.is_file() {
        return Err(format!(
            "{} executable not found at {}",
            id.label(),
            exe.display()
        ));
    }
    let work_dir = exe
        .parent()
        .ok_or_else(|| "Could not determine the Unity client directory".to_string())?;

    report(
        app,
        format!("Starting {} -server {}", id.label(), ticket.server_id),
    );

    let mut cmd = Command::new(&exe);
    cmd.current_dir(work_dir)
        .arg("-server")
        .arg(&ticket.server_id)
        .arg("-ticket")
        .arg(&ticket.sso_ticket);

    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }

    let child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start {}: {e}", id.label()))?;
    report(app, format!("Client started (pid {})", child.id()));
    Ok(())
}

/// Origins runs from a throwaway copy of the install.
///
/// The Shockwave client writes into its own directory and refuses to run twice
/// from the same one, so HabboLauncher copies the whole install to
/// `%TEMP%\shockwave-habbo-<guid>` per launch. The server is encoded in the
/// executable name and **no ticket or argument is passed** — the client prompts
/// for login itself. G-Earth uses its Origins port (40001) here, which is also
/// how Origins traffic gets proxied.
fn launch_origins(app: &AppHandle, client_dir: &Path, settings: &Settings) -> Result<(), String> {
    let server = settings.origins_server;
    let xl = settings.origins_xl;

    let mut exe_name = crate::install::origins_exe(client_dir, server.exe_suffix(), xl);
    if !exe_name.is_file() && xl {
        // Fall back to the non-XL build rather than failing outright.
        report(app, "XL build not present — using the standard Origins client");
        exe_name = crate::install::origins_exe(client_dir, server.exe_suffix(), false);
    }
    if !exe_name.is_file() {
        return Err(format!(
            "Origins executable for {} not found ({})",
            server.label(),
            exe_name.display()
        ));
    }
    let exe_file = exe_name
        .file_name()
        .ok_or_else(|| "Bad Origins executable path".to_string())?
        .to_owned();

    cleanup_old_origins_temp_dirs();

    let temp_dir = std::env::temp_dir().join(format!("shockwave-habbo-{}", crate::settings::new_id()));
    report(app, format!("Preparing Origins {}…", server.label()));
    copy_dir_recursive(client_dir, &temp_dir)
        .map_err(|e| format!("Failed to stage the Origins client: {e}"))?;

    // G-Earth first, on its Origins port, so the client connects through it.
    let gearth_handle = launch_gearth_for_origins(app, settings);

    let exe = temp_dir.join(&exe_file);
    report(app, format!("Starting {}", exe_file.to_string_lossy()));

    let mut cmd = Command::new(&exe);
    cmd.current_dir(&temp_dir);

    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }

    let child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start the Origins client: {e}"))?;
    report(app, format!("Client started (pid {})", child.id()));

    if let Some(handle) = gearth_handle {
        gearth::bind_lifetimes(app.clone(), child, handle);
    }
    Ok(())
}

fn launch_air(
    app: &AppHandle,
    client_dir: &Path,
    ticket: &LoginTicket,
    settings: &Settings,
) -> Result<(), String> {
    let exe = client_dir.join("Habbo.exe");
    if !exe.is_file() {
        return Err(format!("Habbo.exe not found in {}", client_dir.display()));
    }

    let meta = client_dir.join("META-INF").join("AIR").join("application.xml");
    if !meta.is_file() {
        return Err("META-INF/AIR/application.xml is missing from the client install".into());
    }

    // 1. Manifest hygiene + staged-root removal.
    finalize_xml_for_launch(client_dir)?;

    // 2. Stock vs custom SWF.
    let using_custom = swf::apply_swf_choice(app, client_dir, settings.custom_swf.enabled)?;

    // 3. Always start from the pristine manifest, then optionally re-apply a
    //    per-avatar application id.
    swf::restore_manifest(client_dir)?;
    if settings.machine_id_isolation {
        apply_machine_id_isolation(app, client_dir, ticket, using_custom);
    }

    // 4. Let the filesystem settle before AIR reads these files.
    std::thread::sleep(PRE_LAUNCH_SETTLE);

    // 5. G-Earth, when configured and actually present on disk. Never fatal —
    //    the client must still start if this fails.
    let gearth_handle = launch_gearth_if_enabled(app, settings);

    // 6. Client.
    let args = [
        "-server".to_string(),
        ticket.server_id.clone(),
        "-ticket".to_string(),
        ticket.sso_ticket.clone(),
    ];

    let mut cmd = Command::new(&exe);
    cmd.current_dir(client_dir).args(&args);

    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }

    report(
        app,
        format!("Starting Habbo.exe -server {}", ticket.server_id),
    );

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("Failed to start Habbo.exe: {e}");
            report(app, msg.clone());
            return Err(msg);
        }
    };

    report(app, format!("Client started (pid {})", child.id()));

    // Free G-Earth's proxy port when the client exits, so the next launch isn't
    // blocked. Only that direction — see bind_lifetimes for why the reverse is
    // not supervised.
    if let Some(handle) = gearth_handle {
        gearth::bind_lifetimes(app.clone(), child, handle);
    }

    Ok(())
}

/// Gives the account its own AIR identity, and therefore its own machine id.
///
/// AIR keys its application storage on the application id, and the Habbo client
/// stores its generated machine id there — so `com.sulake.habboair.<avatarId>`
/// yields one machine id per account instead of one shared by all of them.
///
/// Skipped in two cases:
///
/// * **No DevID license.** An opaque `LIC…` license is bound to a single
///   application id; rewriting it makes AIR refuse to start with "invalid
///   license". Detected rather than assumed.
/// * **A user custom SWF is active.** Third-party builds are compiled against
///   the stock AIR identity, matching HabboLauncher's behaviour.
///
/// Never fatal: worst case the account shares the default machine id.
fn apply_machine_id_isolation(
    app: &AppHandle,
    client_dir: &Path,
    ticket: &LoginTicket,
    using_custom_swf: bool,
) {
    if using_custom_swf {
        report(app, "Custom SWF active — keeping the stock AIR identity");
        return;
    }

    if !crate::install::has_developer_license(client_dir) {
        report(
            app,
            "No developer license in this build — accounts will share one machine id",
        );
        return;
    }

    match swf::set_flash_application_id(client_dir, &ticket.sso_ticket) {
        Ok(app_id) => report(app, format!("Machine id isolated as {app_id}")),
        Err(e) => {
            // Fall back to the stock identity rather than refusing to launch.
            log::warn!("[Launch] could not isolate machine id: {e}");
            let _ = swf::restore_manifest(client_dir);
            report(app, "Could not isolate machine id — using the shared identity");
        }
    }
}

/// G-Earth for Origins — a different protocol flag and a different port (40001).
///
/// Prefers a dedicated Origins G-Earth build when one is configured, matching
/// HabboLauncher, which falls back to the main G-Earth path otherwise.
fn launch_gearth_for_origins(
    app: &AppHandle,
    settings: &Settings,
) -> Option<gearth::GEarthHandle> {
    if !settings.g_earth.enabled {
        return None;
    }

    let origins_path = settings.g_earth.origins_path.trim();
    let main_path = settings.g_earth.path.trim();
    let path = if !origins_path.is_empty() && Path::new(origins_path).is_file() {
        origins_path
    } else if !main_path.is_empty() && Path::new(main_path).is_file() {
        main_path
    } else {
        report(
            app,
            "G-Earth is enabled but no usable path is set — starting without it",
        );
        return None;
    };

    // Origins traffic is proxied the same way: G-Earth takes the upstream
    // directly as host:port[:user:pass].
    report(app, "Starting G-Earth (Origins)…");

    match gearth::launch(path, gearth::Mode::Origins, None) {
        Ok(handle) => {
            if handle.reused {
                report(
                    app,
                    format!("Reusing the G-Earth already on port {}", handle.port),
                );
            } else if handle.ready {
                report(app, format!("G-Earth listening on port {}", handle.port));
            } else {
                report(
                    app,
                    format!(
                        "G-Earth did not open port {} in time — starting client anyway",
                        handle.port
                    ),
                );
            }
            Some(handle)
        }
        Err(e) => {
            report(app, format!("G-Earth failed to start: {e}"));
            None
        }
    }
}

/// Copies a directory tree. Used to stage a per-launch Origins copy.
fn copy_dir_recursive(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Removes staged Origins copies from previous launches. Ones still in use are
/// locked by the running client and simply fail to delete, which is fine.
fn cleanup_old_origins_temp_dirs() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("shockwave-habbo-") {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Starts G-Earth when enabled.
///
/// Never fatal. An earlier version aborted the whole launch when G-Earth failed,
/// on the reasoning that connecting unintercepted isn't what the user asked for.
/// In practice that turned every G-Earth hiccup into "the game won't open at
/// all", which is worse. Problems are now reported loudly and the client starts
/// regardless — matching HabboLauncher.
fn launch_gearth_if_enabled(app: &AppHandle, settings: &Settings) -> Option<gearth::GEarthHandle> {
    if !settings.g_earth.enabled {
        return None;
    }

    let path = settings.g_earth.path.trim();
    if path.is_empty() {
        report(
            app,
            "G-Earth is enabled but no path is set — starting without it",
        );
        return None;
    }
    if !Path::new(path).is_file() {
        report(
            app,
            format!("G-Earth not found at {path} — starting without it"),
        );
        return None;
    }

    // G-Earth takes the upstream proxy directly as host:port[:user:pass] — the
    // same format HabboLauncher uses. No local bridge is involved here; that's
    // only needed for Chrome, which can't carry proxy credentials on its
    // command line.
    report(app, "Starting G-Earth…");

    match gearth::launch(path, gearth::Mode::Flash, None) {
        Ok(handle) => {
            if handle.reused {
                report(
                    app,
                    format!("Reusing the G-Earth already on port {}", handle.port),
                );
            } else if handle.ready {
                report(app, format!("G-Earth listening on port {}", handle.port));
            } else {
                report(
                    app,
                    format!(
                        "G-Earth did not open port {} in time — starting client anyway",
                        handle.port
                    ),
                );
            }
            Some(handle)
        }
        Err(e) => {
            report(app, format!("G-Earth failed to start: {e}"));
            None
        }
    }
}

/// Delegates to the installer's normalisation so there is exactly one
/// implementation. This used to be a hand-copied duplicate; the copy lacked the
/// guard that stops the AIR runtime shell's root `license.txt` from overwriting
/// the patch's DevID license, and so destroyed machine-id isolation on the first
/// launch after every install.
fn finalize_xml_for_launch(client_dir: &Path) -> Result<(), String> {
    crate::install::normalize_air_application_xml(client_dir)
}
