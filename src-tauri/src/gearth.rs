//! G-Earth integration — port of HabboLauncher's `GEarthMonitor.cs` and
//! `ProcessBinder.cs`.
//!
//! Launch sequence mirrors the C# original:
//!   1. Count existing listeners on the G-Earth proxy port (the *baseline*).
//!      An already-running G-Earth must not be mistaken for the one we start.
//!   2. Spawn G-Earth with CWD set to its own directory (it resolves its JRE
//!      and extensions relative to CWD).
//!   3. Poll until the listener count exceeds the baseline, then wait a short
//!      grace period for it to finish binding.
//!   4. Resolve the PID that actually owns the listening socket — G-Earth is a
//!      JVM launcher, so the PID we spawned is often not the one listening.
//!
//! Readiness is a *soft* gate: if G-Earth never comes up within the timeout we
//! still launch the client, exactly like the C# version.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// G-Earth's proxy listener for the Flash/AIR client.
pub const FLASH_PROXY_PORT: u16 = 30000;
/// G-Earth's proxy listener for Origins/Shockwave.
pub const ORIGINS_PROXY_PORT: u16 = 40001;

const READY_TIMEOUT: Duration = Duration::from_secs(30);
const READY_POLL: Duration = Duration::from_millis(300);
const READY_GRACE: Duration = Duration::from_millis(500);

/// Which G-Earth protocol mode to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Flash,
    /// Reserved for Origins/Shockwave support; the port and flag differ.
    #[allow(dead_code)]
    Origins,
}

impl Mode {
    fn flag(self) -> &'static str {
        match self {
            Mode::Flash => "flash",
            Mode::Origins => "origins",
        }
    }

    pub fn port(self) -> u16 {
        match self {
            Mode::Flash => FLASH_PROXY_PORT,
            Mode::Origins => ORIGINS_PROXY_PORT,
        }
    }
}

/// Outcome of a G-Earth launch attempt.
#[derive(Debug, Clone)]
pub struct GEarthHandle {
    /// PID that owns the listening proxy socket, when we managed to resolve it.
    pub pid: Option<u32>,
    /// Port G-Earth is expected to be listening on.
    pub port: u16,
    /// True when a listener is up (either we started it, or it already was).
    pub ready: bool,
    /// True when an already-running G-Earth was adopted rather than started.
    pub reused: bool,
}

/// Spawns G-Earth and waits (softly) for it to start listening.
///
/// `proxy_arg` is the value for `--proxy`, in HabboLauncher's format:
/// `host:port` or `host:port:user:pass`. Pass `None` for no proxy.
///
/// Accepts either a native launcher (`G-Earth.exe`) or a `.jar`, which is run
/// through `java -jar` since a jar isn't directly executable.
pub fn launch(exe: &str, mode: Mode, proxy_arg: Option<&str>) -> Result<GEarthHandle, String> {
    let exe_path = Path::new(exe);
    if !exe_path.is_file() {
        return Err(format!("G-Earth not found at {exe}"));
    }
    let dir = exe_path
        .parent()
        .ok_or_else(|| "Could not determine G-Earth directory".to_string())?;

    let port = mode.port();

    // Already up? Reuse it. Starting a second instance just loses the race for
    // the proxy port, and the baseline check below would then never see the
    // count rise, burning the full timeout for nothing.
    let baseline = count_listeners_on_port(port);
    if baseline > 0 {
        return Ok(GEarthHandle {
            pid: find_pid_by_listening_port(port),
            port,
            ready: true,
            reused: true,
        });
    }

    let is_jar = exe_path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("jar"))
        .unwrap_or(false);

    // Build the argument list once; both spawn strategies below need it.
    let mut args: Vec<String> = Vec::new();
    let program: std::path::PathBuf = if is_jar {
        // Prefer a JRE bundled next to the jar (G-Earth ships one) before
        // falling back to whatever `java` is on PATH.
        let bundled = dir.join("jre").join("bin").join("javaw.exe");
        args.push("-jar".into());
        args.push(exe_path.display().to_string());
        if bundled.is_file() {
            bundled
        } else {
            std::path::PathBuf::from("javaw")
        }
    } else {
        exe_path.to_path_buf()
    };
    args.push("-c".into());
    args.push(mode.flag().into());
    if let Some(p) = proxy_arg {
        if !p.is_empty() {
            args.push("--proxy".into());
            args.push(p.to_string());
        }
    }

    spawn_detached(&program, dir, &args)?;

    // No immediate-exit check: G-Earth's native stub hands off to its bundled
    // JVM and exits straight away, so a dead child handle is normal here and
    // says nothing about whether G-Earth is running. The listening port is the
    // only trustworthy readiness signal.
    let ready = wait_for_listener(port, baseline);
    let pid = find_pid_by_listening_port(port);

    Ok(GEarthHandle {
        pid,
        port,
        ready,
        reused: false,
    })
}

/// Starts a process. Falls back to elevation via `cmd /c start "" /B` when
/// the target requires admin (G-Earth does — it edits the hosts file).
fn spawn_detached(program: &Path, dir: &Path, args: &[String]) -> Result<(), String> {
    let mut cmd = Command::new(program);
    cmd.current_dir(dir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    match cmd.spawn() {
        Ok(_) => Ok(()),
        Err(e) => {
            // ERROR_ELEVATION_REQUIRED (740) — the exe manifest demands admin.
            if e.raw_os_error() == Some(740) {
                let joined = args.iter()
                    .map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a.clone() })
                    .collect::<Vec<_>>()
                    .join(" ");
                let mut elevate = Command::new("powershell");
                elevate
                    .current_dir(dir)
                    .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command",
                        &format!("Start-Process '{}' -ArgumentList '{}' -WorkingDirectory '{}' -Verb RunAs -WindowStyle Normal",
                            program.display(), joined, dir.display())])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    elevate.creation_flags(CREATE_NO_WINDOW);
                }
                elevate.spawn().map_err(|e| format!("Failed to elevate {}: {e}", program.display()))?;
                Ok(())
            } else {
                Err(format!("Failed to start {}: {e}", program.display()))
            }
        }
    }
}

/// Polls until a *new* listener appears on `port` (count above `baseline`).
/// Returns false on timeout — the caller still proceeds with the launch.
fn wait_for_listener(port: u16, baseline: usize) -> bool {
    let start = Instant::now();
    while start.elapsed() < READY_TIMEOUT {
        if count_listeners_on_port(port) > baseline {
            // Give G-Earth a moment to finish wiring up the listener.
            std::thread::sleep(READY_GRACE);
            return true;
        }
        std::thread::sleep(READY_POLL);
    }
    false
}

/// Number of sockets in LISTEN state on `port`.
pub fn count_listeners_on_port(port: u16) -> usize {
    netstat_listeners(port).len()
}

/// PID owning the first LISTEN socket found on `port`.
pub fn find_pid_by_listening_port(port: u16) -> Option<u32> {
    netstat_listeners(port).into_iter().next()
}

/// Parses `netstat -ano` and returns the PIDs of every listening TCP socket
/// bound to `port`.
///
/// Two things this deliberately avoids:
///
/// * Matching `LISTENING` as text. That word is **localised** by Windows, so on
///   a non-English install the check silently never matches. A listening socket
///   is instead identified by having no peer address, which is locale-neutral.
/// * Substring-matching the whole line for `:port`, which made `:3000` match
///   `:30000`. The local-address column is compared instead.
fn netstat_listeners(port: u16) -> Vec<u32> {
    let mut cmd = Command::new("netstat");
    cmd.arg("-ano").stdin(Stdio::null()).stderr(Stdio::null());

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let Ok(output) = cmd.output() else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let suffix = format!(":{port}");
    let mut pids = Vec::new();

    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        // TCP listening rows have 5 columns: Proto Local Foreign State PID
        if fields.len() < 5 {
            continue;
        }
        if !fields[0].eq_ignore_ascii_case("TCP") {
            continue;
        }
        if !fields[1].ends_with(&suffix) {
            continue;
        }
        if !is_unbound_peer(fields[2]) {
            continue;
        }
        if let Some(pid) = fields.last().and_then(|p| p.parse::<u32>().ok()) {
            if pid > 0 && !pids.contains(&pid) {
                pids.push(pid);
            }
        }
    }
    pids
}

/// A listening socket has no connected peer, which netstat renders as an
/// all-zero address. Works regardless of Windows display language.
fn is_unbound_peer(field: &str) -> bool {
    matches!(field, "0.0.0.0:0" | "[::]:0" | "*:*")
}



/// Force-kills a process tree by PID. Used to tear down G-Earth, whose real
/// process is a JVM child we never owned a handle to.
pub fn kill_pid(pid: u32) {
    let mut cmd = Command::new("taskkill");
    cmd.args(["/T", "/F", "/PID", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let _ = cmd.status();
}

/// Watches the client and shuts G-Earth down once it exits, so the proxy port
/// is free for the next launch.
///
/// **Deliberately one-directional.** HabboLauncher's `ProcessBinder` also closes
/// the client when G-Earth disappears, and two attempts to reproduce that here
/// both ended up killing the game during normal play:
///
/// 1. Judging G-Earth by its listening socket — it closes that listener once the
///    client connects, so the supervisor concluded it had died seconds after a
///    successful connection.
/// 2. Judging it by process liveness — G-Earth spans a stub plus several JVM
///    processes and binds the port many times over, so the single PID resolved
///    from the socket is not reliably the long-lived one. `OpenProcess` failing
///    for any reason (including access denied) also reads as "gone".
///
/// The upside never justified the risk: if G-Earth is closed mid-session the
/// client merely loses its connection and Habbo shows an ordinary disconnect,
/// which is visible and recoverable. The old behaviour made the game vanish with
/// no crash, no error and no trace.
///
/// Runs on a detached thread; the client `Child` is consumed.
pub fn bind_lifetimes(app: tauri::AppHandle, mut client: std::process::Child, gearth: GEarthHandle) {
    std::thread::spawn(move || {
        const POLL: Duration = Duration::from_millis(1500);
        loop {
            std::thread::sleep(POLL);

            match client.try_wait() {
                Ok(Some(status)) => {
                    // Adopted instances belong to the user, not to us.
                    if gearth.reused {
                        crate::diag::log_only(
                            &app,
                            format!(
                                "Client exited ({status}); leaving the pre-existing G-Earth running"
                            ),
                        );
                        return;
                    }
                    let pid = gearth.pid.or_else(|| find_pid_by_listening_port(gearth.port));
                    match pid {
                        Some(pid) => {
                            kill_pid(pid);
                            crate::diag::log_only(
                                &app,
                                format!("Client exited ({status}); stopped G-Earth (pid {pid})"),
                            );
                        }
                        None => crate::diag::log_only(
                            &app,
                            format!("Client exited ({status}); no G-Earth process to stop"),
                        ),
                    }
                    return;
                }
                Err(e) => {
                    crate::diag::log_only(&app, format!("Stopped supervising the client: {e}"));
                    return;
                }
                Ok(None) => {}
            }
        }
    });
}
