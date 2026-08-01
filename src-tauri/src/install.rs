//! Client download/install — mirrors HabboCustomLauncher (LilithRainbows).
//!
//! Classic = official AIR from hotel `/gamedata/clienturls`
//! AirPlus = HabboAir.swf from HabboAirPlus releases + AirPlus patch
//! AirBobba (Bobba Client) = HabboAir.swf from bobba-client releases + Bobba patch

use std::fs::{self, File};
use std::io::{copy, Write};
use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use zip::ZipArchive;

use crate::clients::ClientId;

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) HabboLauncher/1.0.41 BobbaPacketLauncher/0.1";

const AIRPLUS_SWF_URL: &str =
    "https://github.com/LilithRainbows/HabboAirPlus/releases/download/latest/HabboAir.swf";

const AIRBOBBA_GITHUB_REPO: &str = "Bobba-Packet/bobba-client";

const ASSET_BASE: &str =
    "https://raw.githubusercontent.com/LilithRainbows/HabboCustomLauncher/main/Assets";

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    updated_at: String,
    browser_download_url: String,
}

#[derive(Debug, Clone)]
struct RemoteClient {
    /// Folder / VERSION.txt identity — changes when GitHub ships a new build.
    version: String,
    /// HabboAir.swf download URL (or Classic zip URL).
    client_url: String,
    /// Extra patch zip for AirBobba, if any.
    patch_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub stage: String,
    pub percent: Option<u8>,
    pub message: String,
}

fn emit_progress(app: &AppHandle, stage: &str, percent: Option<u8>, message: impl Into<String>) {
    let _ = app.emit(
        "client-progress",
        ProgressEvent {
            stage: stage.into(),
            percent,
            message: message.into(),
        },
    );
}

pub fn data_root(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

pub fn client_dir(root: &Path, id: ClientId, version: &str) -> Result<PathBuf, String> {
    let folder = id
        .download_dir_name()
        .ok_or_else(|| "This client is not available yet".to_string())?;
    Ok(root.join("downloads").join(folder).join(version))
}

fn air_patch_asset() -> &'static str {
    match std::env::consts::ARCH {
        "x86" => "HabboAirWindowsPatch_x86.zip",
        _ => "HabboAirWindowsPatch_x64.zip",
    }
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())
}

async fn download_file(
    app: &AppHandle,
    url: &str,
    dest: &Path,
    label: &str,
) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let client = http_client()?;
    emit_progress(app, "download", Some(0), format!("Downloading {label}…"));
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download failed ({url}): {e}"))?
        .error_for_status()
        .map_err(|e| format!("Download HTTP error ({url}): {e}"))?;

    let total = response.content_length();
    let mut stream = response.bytes_stream();
    let mut file = File::create(dest).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if let Some(total) = total {
            let pct = ((downloaded as f64 / total as f64) * 100.0).min(100.0) as u8;
            emit_progress(
                app,
                "download",
                Some(pct),
                format!("Downloading {label}… ({pct}%)"),
            );
        }
    }
    Ok(())
}

async fn download_asset_zip(app: &AppHandle, name: &str, dest_dir: &Path) -> Result<PathBuf, String> {
    let url = format!("{ASSET_BASE}/{name}");
    let zip_path = dest_dir.join(name);
    download_file(app, &url, &zip_path, name).await?;
    Ok(zip_path)
}

fn should_skip(relative: &str, skip_prefixes: &[&str]) -> bool {
    let normalized = relative.replace('\\', "/").trim_start_matches('/').to_string();
    for skip in skip_prefixes {
        let skip_norm = skip.replace('\\', "/");
        if normalized.eq_ignore_ascii_case(&skip_norm) {
            return true;
        }
        let prefix = format!("{skip_norm}/");
        if normalized.len() > prefix.len()
            && normalized[..prefix.len()].eq_ignore_ascii_case(&prefix)
        {
            return true;
        }
    }
    false
}

fn unzip(zip_path: &Path, dest: &Path, skip_prefixes: &[&str]) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry
            .enclosed_name()
            .ok_or_else(|| "Invalid zip entry path".to_string())?
            .to_path_buf();
        let name_str = name.to_string_lossy();
        if should_skip(&name_str, skip_prefixes) {
            continue;
        }
        let out = dest.join(&name);
        if entry.is_dir() || name_str.ends_with('/') {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = File::create(&out).map_err(|e| e.to_string())?;
            copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn set_swf_version(path: &Path, version: u8) -> Result<(), String> {
    let mut data = fs::read(path).map_err(|e| e.to_string())?;
    if data.len() < 4 {
        return Err("HabboAir.swf is too small".into());
    }
    data[3] = version;
    fs::write(path, data).map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
struct ClientUrlsJson {
    #[serde(rename = "flash-windows-version")]
    flash_windows_version: String,
    #[serde(rename = "flash-windows")]
    flash_windows: String,
}

async fn fetch_official_clienturls(hotel_host: &str) -> Result<(String, String), String> {
    let host = hotel_host
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let url = format!("https://{host}/gamedata/clienturls");
    let client = http_client()?;
    let text = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("clienturls request failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("clienturls HTTP error: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let parsed: ClientUrlsJson =
        serde_json::from_str(&text).map_err(|e| format!("Invalid clienturls JSON: {e}"))?;
    Ok((parsed.flash_windows_version, parsed.flash_windows))
}

async fn github_swf_version(swf_url: &str) -> Result<String, String> {
    // Stable folder name: avoid Utc::now() which created a new broken install on every launch.
    // Prefer Last-Modified epoch when GitHub returns it; otherwise a fixed pin.
    let client = http_client()?;
    let response = client
        .head(swf_url)
        .send()
        .await
        .map_err(|e| format!("SWF HEAD failed ({swf_url}): {e}"))?;
    if response.status().is_success() {
        if let Some(lm) = response.headers().get(reqwest::header::LAST_MODIFIED) {
            let s = lm.to_str().unwrap_or("");
            if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(s) {
                return Ok(dt.timestamp().to_string());
            }
        }
        if let Some(etag) = response.headers().get(reqwest::header::ETAG) {
            let tag = etag.to_str().unwrap_or("").trim_matches('"');
            if !tag.is_empty() {
                let safe: String = tag
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                    .collect();
                return Ok(format!("etag_{safe}"));
            }
        }
    }
    Ok("latest".into())
}

#[derive(Debug, Clone, Copy)]
enum SwfClientKind {
    AirPlus,
    AirBobba,
}

async fn resolve_remote_client(
    id: ClientId,
    hotel_host: &str,
) -> Result<(RemoteClient, Option<SwfClientKind>), String> {
    match id {
        ClientId::Classic => {
            let (version, client_url) = fetch_official_clienturls(hotel_host).await?;
            Ok((
                RemoteClient {
                    version,
                    client_url,
                    patch_url: None,
                },
                None,
            ))
        }
        ClientId::AirPlus => {
            let version = github_swf_version(AIRPLUS_SWF_URL).await?;
            Ok((
                RemoteClient {
                    version,
                    client_url: AIRPLUS_SWF_URL.to_string(),
                    patch_url: None,
                },
                Some(SwfClientKind::AirPlus),
            ))
        }
        ClientId::AirBobba => Ok((fetch_bobba_client_release().await?, Some(SwfClientKind::AirBobba))),
    }
}

/// Latest bobba-client GitHub Release — version includes SWF asset stamp so
/// republished builds on the same tag still trigger a download.
async fn fetch_bobba_client_release() -> Result<RemoteClient, String> {
    let url = format!("https://api.github.com/repos/{AIRBOBBA_GITHUB_REPO}/releases/latest");
    let client = http_client()?;
    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("bobba-client releases request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "bobba-client releases request failed ({})",
            response.status()
        ));
    }

    let release: GhRelease = response
        .json()
        .await
        .map_err(|e| format!("Invalid bobba-client release JSON: {e}"))?;
    let tag = release.tag_name.trim();
    if tag.is_empty() {
        return Err("bobba-client latest release has an empty tag".into());
    }

    let swf = release
        .assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("HabboAir.swf"))
        .ok_or_else(|| "bobba-client latest release is missing HabboAir.swf".to_string())?;
    let patch = release
        .assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("HabboAirBobbaPatch.zip"))
        .ok_or_else(|| "bobba-client latest release is missing HabboAirBobbaPatch.zip".to_string())?;

    let stamp = asset_version_stamp(&swf.updated_at);
    let version = sanitize_version_folder(&format!("{tag}_{stamp}"));

    Ok(RemoteClient {
        version,
        client_url: swf.browser_download_url.clone(),
        patch_url: Some(patch.browser_download_url.clone()),
    })
}

fn asset_version_stamp(updated_at: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(updated_at) {
        return dt.timestamp().to_string();
    }
    sanitize_version_folder(updated_at)
}

fn sanitize_version_folder(raw: &str) -> String {
    let safe: String = raw
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect();
    if safe.is_empty() {
        "latest".into()
    } else {
        safe
    }
}

fn read_installed_version(dir: &Path) -> Option<String> {
    let raw = fs::read_to_string(dir.join("VERSION.txt")).ok()?;
    let v = raw.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// True when the on-disk install matches the remote version identity.
fn is_version_current(dir: &Path, remote_version: &str) -> bool {
    is_install_healthy(dir) && read_installed_version(dir).as_deref() == Some(remote_version)
}

/// Remove a previous install folder after a successful version bump.
pub fn remove_install(root: &Path, id: ClientId, version: &str) {
    if let Ok(dir) = client_dir(root, id, version) {
        if dir.exists() {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

pub async fn ensure_installed(
    app: &AppHandle,
    root: &Path,
    id: ClientId,
    hotel_host: &str,
) -> Result<(String, PathBuf), String> {
    if !id.supported() {
        return Err(format!("{} is not available yet", id.label()));
    }
    if !cfg!(target_os = "windows") {
        return Err("Install pipeline is currently Windows-only".into());
    }

    emit_progress(
        app,
        "check",
        None,
        format!("Verifying latest {} version…", id.label()),
    );

    let (remote, swf_kind) = resolve_remote_client(id, hotel_host).await?;
    let version = remote.version.clone();
    let dest = client_dir(root, id, &version)?;

    // Skip download only when local VERSION.txt matches the remote build id.
    if is_version_current(&dest, &version) {
        let _ = normalize_air_application_xml(&dest);
        emit_progress(
            app,
            "ready",
            Some(100),
            format!("{} is up to date ({version})", id.label()),
        );
        return Ok((version, dest));
    }

    emit_progress(
        app,
        "update",
        Some(0),
        format!("Downloading {} {version}…", id.label()),
    );

    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;

    // 1) Download client payload
    let payload_path = if swf_kind.is_some() {
        let swf = dest.join("HabboAir.swf");
        download_file(app, &remote.client_url, &swf, "HabboAir.swf").await?;
        swf
    } else {
        let zip = dest.join("ClientDownload.zip");
        download_file(app, &remote.client_url, &zip, "official client").await?;
        zip
    };

    // 2) AIR runtime shell
    emit_progress(app, "extract", None, "Extracting AIR runtime…");
    let air_zip = download_asset_zip(app, air_patch_asset(), &dest).await?;
    unzip(&air_zip, &dest, &[])?;
    let _ = fs::remove_file(&air_zip);

    // 3) Client contents
    emit_progress(app, "extract", None, "Extracting client…");
    match swf_kind {
        Some(SwfClientKind::AirPlus) => {
            let plus_zip = download_asset_zip(app, "HabboAirPlusPatch.zip", &dest).await?;
            unzip(&plus_zip, &dest, &[])?;
            let _ = fs::remove_file(&plus_zip);
        }
        Some(SwfClientKind::AirBobba) => {
            let patch_url = remote
                .patch_url
                .as_deref()
                .ok_or_else(|| "Missing HabboAirBobbaPatch.zip URL".to_string())?;
            let bobba_zip = dest.join("HabboAirBobbaPatch.zip");
            download_file(app, patch_url, &bobba_zip, "HabboAirBobbaPatch.zip").await?;
            unzip(&bobba_zip, &dest, &[])?;
            let _ = fs::remove_file(&bobba_zip);
        }
        None => {
            // Official package — keep AIR shell Habbo.exe from patch
            unzip(
                &payload_path,
                &dest,
                &[
                    "Adobe AIR",
                    "META-INF/signatures.xml",
                    "META-INF/AIR/hash",
                    "Habbo.exe",
                ],
            )?;
            let _ = fs::remove_file(&payload_path);
        }
    }

    // Align with working HabboCustomLauncher layout (META xml only, no Discord extensions)
    normalize_air_application_xml(&dest)?;

    let swf = dest.join("HabboAir.swf");
    if !swf.is_file() {
        return Err("Install finished but HabboAir.swf is missing".into());
    }
    if !dest.join("Habbo.exe").is_file() {
        return Err("Install finished but Habbo.exe is missing".into());
    }

    // HabboCustomLauncher forces SWF version 51 on Windows
    set_swf_version(&swf, 51)?;
    fs::write(dest.join("VERSION.txt"), &version).map_err(|e| e.to_string())?;
    emit_progress(app, "ready", Some(100), "Client ready");
    Ok((version, dest))
}

/// Match a working HabboCustomLauncher layout:
/// META-INF/AIR/application.xml only, no Discord `<extensions>`, no root application.xml.
fn normalize_air_application_xml(dest: &Path) -> Result<(), String> {
    let meta = dest.join("META-INF").join("AIR").join("application.xml");
    if !meta.is_file() {
        return Ok(());
    }
    let mut xml = fs::read_to_string(&meta).map_err(|e| e.to_string())?;
    xml = strip_extensions_from_string(&xml);
    if !xml.contains("<encryptedLocalStorage>") {
        xml = insert_encrypted_local_storage(&xml);
    }
    fs::write(&meta, &xml).map_err(|e| e.to_string())?;

    let root = dest.join("application.xml");
    if root.is_file() {
        let _ = fs::remove_file(&root);
    }
    let license = dest.join("license.txt");
    let license_dest = dest.join("META-INF").join("AIR").join("license.txt");
    if license.is_file() {
        let _ = fs::rename(license, license_dest);
    }
    Ok(())
}

fn strip_extensions_from_string(xml: &str) -> String {
    if let (Some(start), Some(end_rel)) = (xml.find("<extensions>"), xml.find("</extensions>")) {
        let end = end_rel + "</extensions>".len();
        let mut out = String::with_capacity(xml.len());
        out.push_str(&xml[..start]);
        out.push_str(xml[end..].trim_start_matches(['\r', '\n', ' ', '\t']));
        out
    } else {
        xml.to_string()
    }
}

fn insert_encrypted_local_storage(xml: &str) -> String {
    let block = "    <encryptedLocalStorage>\n        <fallbackMode>never</fallbackMode>\n        <storageMode>file</storageMode>\n    </encryptedLocalStorage>\n";
    if let Some(idx) = xml.rfind("</application>") {
        let mut out = String::with_capacity(xml.len() + block.len());
        out.push_str(&xml[..idx]);
        out.push_str(block);
        out.push_str(&xml[idx..]);
        out
    } else {
        xml.to_string()
    }
}

/// True when an on-disk install looks complete enough to launch.
pub fn is_install_healthy(dir: &Path) -> bool {
    dir.join("Habbo.exe").is_file()
        && dir.join("HabboAir.swf").is_file()
        && dir
            .join("META-INF")
            .join("AIR")
            .join("application.xml")
            .is_file()
}

/// Resolve an already-installed client directory from settings version.
pub fn resolve_install(root: &Path, id: ClientId, version: &str) -> Result<PathBuf, String> {
    let dir = client_dir(root, id, version)?;
    if is_install_healthy(&dir) {
        Ok(dir)
    } else {
        Err("Client is not installed. Click Install / Update first.".into())
    }
}

/// Repair existing installs to the known-good XML layout before launch.
pub async fn repair_if_needed(_app: &AppHandle, dir: &Path) -> Result<(), String> {
    if !dir.join("Habbo.exe").is_file() || !dir.join("HabboAir.swf").is_file() {
        return Err("Client install is incomplete".into());
    }
    normalize_air_application_xml(dir)?;
    if is_install_healthy(dir) {
        Ok(())
    } else {
        Err("Client install is incomplete".into())
    }
}
