use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

const GITHUB_REPO: &str = "Bobba-Packet/bobba-launcher";
const USER_AGENT: &str = "Bobba-Launcher";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherUpdate {
    pub version: String,
    pub notes: Option<String>,
    pub html_url: String,
    pub download_url: String,
    pub asset_name: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    body: Option<String>,
    html_url: String,
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

fn normalize_version(raw: &str) -> String {
    raw.trim().trim_start_matches('v').trim().to_string()
}

fn is_newer(remote: &str, current: &str) -> bool {
    let remote = normalize_version(remote);
    let current = normalize_version(current);
    match (parse_semver(&remote), parse_semver(&current)) {
        (Some(r), Some(c)) => r > c,
        _ => remote != current && !remote.is_empty(),
    }
}

fn parse_semver(v: &str) -> Option<(u64, u64, u64)> {
    let mut parts = v.split('.').take(3);
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts
        .next()
        .unwrap_or("0")
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .unwrap_or("0")
        .parse()
        .ok()?;
    Some((major, minor, patch))
}

fn pick_windows_asset(assets: &[GhAsset]) -> Option<&GhAsset> {
    // Prefer NSIS setup installer, then any .exe
    assets
        .iter()
        .find(|a| {
            let n = a.name.to_lowercase();
            n.ends_with("-setup.exe") || n.ends_with("_setup.exe") || n.contains("setup") && n.ends_with(".exe")
        })
        .or_else(|| {
            assets.iter().find(|a| {
                let n = a.name.to_lowercase();
                n.ends_with(".exe") && !n.ends_with(".sig")
            })
        })
}

pub async fn check_for_update(current_version: &str) -> Result<Option<LauncherUpdate>, String> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }

    if !response.status().is_success() {
        return Err(format!(
            "GitHub releases request failed ({})",
            response.status()
        ));
    }

    let release: GhRelease = response.json().await.map_err(|e| e.to_string())?;
    if !is_newer(&release.tag_name, current_version) {
        return Ok(None);
    }

    let asset = pick_windows_asset(&release.assets).ok_or_else(|| {
        "Latest release has no Windows .exe asset".to_string()
    })?;

    Ok(Some(LauncherUpdate {
        version: normalize_version(&release.tag_name),
        notes: release.body,
        html_url: release.html_url,
        download_url: asset.browser_download_url.clone(),
        asset_name: asset.name.clone(),
    }))
}

pub async fn download_and_install(app: &AppHandle, update: &LauncherUpdate) -> Result<(), String> {
    let _ = app.emit(
        "launcher-update-progress",
        serde_json::json!({
            "stage": "download",
            "percent": 0,
            "message": format!("Downloading launcher v{}…", update.version),
        }),
    );

    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(&update.download_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;

    let total = response.content_length();
    let temp_dir = std::env::temp_dir().join("bobba-launcher-update");
    std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

    // Unique path per attempt so a stuck previous download cannot lock the name.
    let dest: PathBuf = temp_dir.join(format!(
        "{}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
        &update.asset_name
    ));

    {
        let mut file = tokio::fs::File::create(&dest)
            .await
            .map_err(|e| e.to_string())?;
        let mut stream = response.bytes_stream();
        let mut downloaded: u64 = 0;

        use tokio::io::AsyncWriteExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| e.to_string())?;
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
            downloaded += chunk.len() as u64;
            let percent = total.map(|t| {
                if t == 0 {
                    0
                } else {
                    ((downloaded as f64 / t as f64) * 100.0).round() as u32
                }
            });
            let _ = app.emit(
                "launcher-update-progress",
                serde_json::json!({
                    "stage": "download",
                    "percent": percent,
                    "message": format!("Downloading launcher v{}…", update.version),
                }),
            );
        }
        file.flush().await.map_err(|e| e.to_string())?;
        file.sync_all().await.map_err(|e| e.to_string())?;
        // File must be fully closed before Windows will allow CreateProcess on it.
    }

    let _ = app.emit(
        "launcher-update-progress",
        serde_json::json!({
            "stage": "install",
            "percent": 100,
            "message": "Installing update…",
        }),
    );

    // NSIS setup: silent install so it can replace binaries after we quit.
    let is_setup = update.asset_name.to_lowercase().contains("setup");
    let mut last_err = String::new();
    for attempt in 0..8u32 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(150 * u64::from(attempt))).await;
        }
        let mut cmd = std::process::Command::new(&dest);
        if is_setup {
            cmd.arg("/S");
        }
        match cmd.spawn() {
            Ok(_) => {
                // Quit so the installer can replace the running binary
                app.exit(0);
                return Ok(());
            }
            Err(e) => {
                last_err = e.to_string();
                // ERROR_SHARING_VIOLATION / antivirus scan — retry briefly
                let retryable = last_err.contains("sendo usado")
                    || last_err.contains("being used")
                    || last_err.contains("os error 32")
                    || last_err.contains("Sharing violation");
                if !retryable {
                    break;
                }
            }
        }
    }

    Err(format!(
        "Failed to start updater ({}): {last_err}. Close other Bobba Launcher windows and try again, or install manually from {}",
        dest.display(),
        update.html_url
    ))
}

pub fn current_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}
