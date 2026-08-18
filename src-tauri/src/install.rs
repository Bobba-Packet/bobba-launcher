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

use crate::clients::{ClientId, ClientKind};

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

/// Public wrapper so other modules (custom SWF) can reuse the progress-reporting
/// downloader.
pub async fn download_to(
    app: &AppHandle,
    url: &str,
    dest: &Path,
    label: &str,
) -> Result<(), String> {
    download_file(app, url, dest, label).await
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

/// `/gamedata/clienturls` — the same document shape is served by every platform,
/// but each only populates the keys relevant to it: `www.habbo.com` has the flash
/// and unity keys, `www.habbox.game` only unity, `origins.habbo.com` only
/// shockwave. Everything is therefore optional.
#[derive(Debug, Deserialize)]
struct ClientUrlsJson {
    #[serde(rename = "flash-windows-version")]
    flash_windows_version: Option<String>,
    #[serde(rename = "flash-windows")]
    flash_windows: Option<String>,
    #[serde(rename = "unity-windows-version")]
    unity_windows_version: Option<String>,
    #[serde(rename = "unity-windows")]
    unity_windows: Option<String>,
    #[serde(rename = "shockwave-windows-version")]
    shockwave_windows_version: Option<String>,
    #[serde(rename = "shockwave-windows")]
    shockwave_windows: Option<String>,
}

async fn fetch_clienturls(host: &str) -> Result<ClientUrlsJson, String> {
    let host = host
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    let url = format!("https://{host}/gamedata/clienturls");
    let client = http_client()?;
    let text = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("clienturls request failed ({host}): {e}"))?
        .error_for_status()
        .map_err(|e| format!("clienturls HTTP error ({host}): {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("Invalid clienturls JSON from {host}: {e}"))
}

async fn fetch_official_clienturls(hotel_host: &str) -> Result<(String, String), String> {
    let parsed = fetch_clienturls(hotel_host).await?;
    let version = parsed
        .flash_windows_version
        .ok_or_else(|| "clienturls has no flash-windows-version".to_string())?;
    let url = parsed
        .flash_windows
        .ok_or_else(|| "clienturls has no flash-windows URL".to_string())?;
    Ok((version, url))
}

/// Version + zip URL for a Unity build (regular Unity client, or Habbo X).
async fn fetch_unity_clienturls(host: &str) -> Result<(String, String), String> {
    let parsed = fetch_clienturls(host).await?;
    let version = parsed
        .unity_windows_version
        .ok_or_else(|| format!("{host} clienturls has no unity-windows-version"))?;
    let url = parsed
        .unity_windows
        .ok_or_else(|| format!("{host} clienturls has no unity-windows URL"))?;
    Ok((version, url))
}

/// Version + zip URL for the Origins (Shockwave) client.
async fn fetch_shockwave_clienturls(host: &str) -> Result<(String, String), String> {
    let parsed = fetch_clienturls(host).await?;
    let version = parsed
        .shockwave_windows_version
        .ok_or_else(|| format!("{host} clienturls has no shockwave-windows-version"))?;
    let url = parsed
        .shockwave_windows
        .ok_or_else(|| format!("{host} clienturls has no shockwave-windows URL"))?;
    Ok((version, url))
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
        // Unity / Habbo X / Origins never reach here — they install through
        // ensure_zip_installed, which resolves its own URLs.
        other => Err(format!(
            "{} is not an AIR client and has no SWF payload",
            other.label()
        )),
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

/// Bumped whenever the on-disk layout produced by this installer changes in a
/// way that existing installs can't be patched into. A mismatch forces exactly
/// one clean reinstall, which is safer than inferring brokenness from file
/// contents — that risks a reinstall loop when the inference is wrong.
///
/// rev 2: stopped the AIR runtime shell's root `license.txt` from overwriting
///        the patch's DevID license in `META-INF/AIR`.
/// rev 3: the launch path had a duplicate, unguarded copy of that same move and
///        was clobbering the DevID license on first launch. Installs produced
///        before this need rebuilding to recover the license.
const INSTALL_LAYOUT_REVISION: &str = "3";

fn read_installed_version(dir: &Path) -> Option<String> {
    let raw = fs::read_to_string(dir.join("VERSION.txt")).ok()?;
    let v = raw.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

fn read_layout_revision(dir: &Path) -> Option<String> {
    let raw = fs::read_to_string(dir.join("LAYOUT.txt")).ok()?;
    let v = raw.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// True when the on-disk install matches the remote version identity *and* was
/// produced by the current installer layout.
fn is_version_current(dir: &Path, remote_version: &str) -> bool {
    is_install_healthy(dir)
        && read_installed_version(dir).as_deref() == Some(remote_version)
        && read_layout_revision(dir).as_deref() == Some(INSTALL_LAYOUT_REVISION)
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

    // Unity and Shockwave builds ship as a plain zip with nothing to patch, so
    // they skip the whole AIR pipeline (runtime shell, SWF selection, manifest
    // normalisation, licence handling).
    match id.kind() {
        ClientKind::Air => ensure_air_installed(app, root, id, hotel_host).await,
        ClientKind::Unity | ClientKind::Shockwave => ensure_zip_installed(app, root, id).await,
    }
}

/// Install path for Unity / Habbo X / Origins: resolve the version and zip URL
/// from the platform's own `gamedata/clienturls`, download, extract, done.
async fn ensure_zip_installed(
    app: &AppHandle,
    root: &Path,
    id: ClientId,
) -> Result<(String, PathBuf), String> {
    let platform = id.platform();
    let host = platform.gamedata_host();

    emit_progress(
        app,
        "check",
        None,
        format!("Verifying latest {} version…", id.label()),
    );

    let (version, url) = match id.kind() {
        ClientKind::Shockwave => fetch_shockwave_clienturls(host).await?,
        _ => fetch_unity_clienturls(host).await?,
    };
    let version = sanitize_version_folder(&version);
    let dest = client_dir(root, id, &version)?;

    if is_install_healthy_for(id, &dest) && read_installed_version(&dest).as_deref() == Some(&version)
    {
        emit_progress(
            app,
            "ready",
            Some(100),
            format!("{} is up to date ({version})", id.label()),
        );
        return Ok((version, dest));
    }

    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;

    let zip = dest.join("ClientDownload.zip");
    download_file(app, &url, &zip, &format!("{} client", id.label())).await?;

    emit_progress(app, "extract", None, format!("Extracting {}…", id.label()));
    unzip(&zip, &dest, &[])?;
    let _ = fs::remove_file(&zip);

    if !is_install_healthy_for(id, &dest) {
        return Err(format!(
            "{} install finished but the expected executable is missing",
            id.label()
        ));
    }

    fs::write(dest.join("VERSION.txt"), &version).map_err(|e| e.to_string())?;
    emit_progress(app, "ready", Some(100), "Client ready");
    Ok((version, dest))
}

async fn ensure_air_installed(
    app: &AppHandle,
    root: &Path,
    id: ClientId,
    hotel_host: &str,
) -> Result<(String, PathBuf), String> {
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
    fs::write(dest.join("LAYOUT.txt"), INSTALL_LAYOUT_REVISION).map_err(|e| e.to_string())?;

    if has_developer_license(&dest) {
        emit_progress(app, "ready", Some(100), "Client ready");
    } else {
        // Worth surfacing: without a DevID license the AIR app id can't be
        // rewritten, so every account will share one machine id.
        emit_progress(
            app,
            "ready",
            Some(100),
            "Client ready (no developer license — machine-id isolation unavailable)",
        );
    }
    Ok((version, dest))
}

/// Match a working HabboCustomLauncher layout:
/// META-INF/AIR/application.xml only, no Discord `<extensions>`, no root application.xml.
///
/// Public because the launch path must apply the exact same normalisation. It
/// previously had its own copy of this logic, the two drifted, and the launch
/// copy silently overwrote the DevID license on every run.
pub fn normalize_air_application_xml(dest: &Path) -> Result<(), String> {
    let meta = dest.join("META-INF").join("AIR").join("application.xml");
    if !meta.is_file() {
        return Ok(());
    }
    let mut xml = fs::read_to_string(&meta).map_err(|e| e.to_string())?;
    xml = strip_extensions_from_string(&xml);
    xml = clamp_descriptor_namespace(&xml);
    if !xml.contains("<encryptedLocalStorage>") {
        xml = insert_encrypted_local_storage(&xml);
    }
    fs::write(&meta, &xml).map_err(|e| e.to_string())?;

    // The pristine backup is restored over application.xml before every launch,
    // so it needs the same namespace correction or the fix is undone each time.
    let backup = dest.join("META-INF").join("AIR").join("application.original.xml");
    if backup.is_file() {
        if let Ok(raw) = fs::read_to_string(&backup) {
            let fixed = clamp_descriptor_namespace(&raw);
            if fixed != raw {
                let _ = fs::write(&backup, fixed);
            }
        }
    }

    let root = dest.join("application.xml");
    if root.is_file() {
        let _ = fs::remove_file(&root);
    }
    // Stage a root-level license.txt into META-INF/AIR — but never on top of one
    // that's already there.
    //
    // The client patch zips ship `META-INF/AIR/license.txt`, a HARMAN *DevID*
    // license which validates against the developer and therefore permits any
    // application id. The AIR runtime shell zip ships its own root
    // `license.txt`, an app-id-bound `LIC…` blob. Overwriting the former with
    // the latter is what previously made a rewritten `<id>` fail with
    // "invalid license — your application bundle may have been modified",
    // which in turn cost us per-account machine-id isolation.
    let license = dest.join("license.txt");
    let license_dest = dest.join("META-INF").join("AIR").join("license.txt");
    if license.is_file() && !license_dest.is_file() {
        let _ = fs::rename(&license, &license_dest);
    }

    // Snapshot the clean manifest and stock SWF now, before any launch rewrites
    // the AIR application id. Doing it here (rather than at swap time) is what
    // guarantees application.original.xml never contains a stale avatar id.
    let _ = crate::swf::ensure_backups(dest);
    Ok(())
}

/// True when the bundle carries a HARMAN **DevID** license.
///
/// A DevID license is keyed to the developer, so the application id may be
/// rewritten freely — which is what per-account machine-id isolation relies on.
/// An opaque `LIC…` license is bound to one specific application id and must be
/// left alone.
pub fn has_developer_license(dir: &Path) -> bool {
    let path = dir.join("META-INF").join("AIR").join("license.txt");
    match fs::read_to_string(&path) {
        Ok(text) => text.trim_start().starts_with("DevID="),
        // No license at all: nothing binds us to an id, so rewriting is safe.
        Err(_) => !path.exists(),
    }
}

/// Pins the AIR descriptor namespace to `<major>.0`.
///
/// `HabboAirPlusPatch.zip` declares `http://ns.adobe.com/air/application/51.3`,
/// and the AIR runtime it ships alongside (51.3.3.2) *rejects* that namespace:
/// `Habbo.exe` exits with code 5 immediately — no window, no dialog, nothing in
/// the event log. Only `<major>.0` namespaces are reliably recognised, which is
/// what the official Habbo package uses (`51.0`) and why that one works.
///
/// Verified experimentally: changing nothing but this makes an otherwise
/// identical bundle launch.
fn clamp_descriptor_namespace(xml: &str) -> String {
    const MARKER: &str = "air/application/";
    let Some(pos) = xml.find(MARKER) else {
        return xml.to_string();
    };
    let start = pos + MARKER.len();
    let rest = &xml[start..];

    // Version runs until the closing quote of the xmlns attribute.
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(rest.len());
    let version = &rest[..end];

    let Some((major, minor)) = version.split_once('.') else {
        return xml.to_string();
    };
    if minor == "0" || major.is_empty() || !major.chars().all(|c| c.is_ascii_digit()) {
        return xml.to_string();
    }

    format!("{}{}.0{}", &xml[..start], major, &rest[end..])
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

#[cfg(test)]
mod tests {
    use super::*;

    fn write_license(dir: &Path, body: &str) {
        let air = dir.join("META-INF").join("AIR");
        fs::create_dir_all(&air).unwrap();
        fs::write(air.join("license.txt"), body).unwrap();
    }

    #[test]
    fn devid_license_permits_app_id_rewrite() {
        let dir = std::env::temp_dir().join(format!("bobba-lic-dev-{}", std::process::id()));
        write_license(
            &dir,
            "DevID=3f60df2a-47e5-4372-bf1b-48448d750304\nLicense=76b68541bbc084a9",
        );
        assert!(has_developer_license(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn opaque_lic_license_blocks_app_id_rewrite() {
        let dir = std::env::temp_dir().join(format!("bobba-lic-lic-{}", std::process::id()));
        write_license(&dir, "LIC3f4cc9f83ea1e63f10c6c8dc8ddc9fec32825bad");
        assert!(!has_developer_license(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    /// Regression guard. The AIR runtime shell leaves a root `license.txt`
    /// (app-id-bound) next to the patch's `META-INF/AIR/license.txt` (DevID).
    /// Normalisation must never let the former replace the latter — and it must
    /// hold no matter how many times it runs, since it executes on every launch.
    #[test]
    fn normalisation_never_clobbers_the_devid_license() {
        let dir = std::env::temp_dir().join(format!("bobba-lic-clobber-{}", std::process::id()));
        let air = dir.join("META-INF").join("AIR");
        fs::create_dir_all(&air).unwrap();

        fs::write(
            air.join("application.xml"),
            "<application><id>com.sulake.habboair</id></application>",
        )
        .unwrap();
        fs::write(air.join("license.txt"), "DevID=abc\nLicense=def").unwrap();
        fs::write(dir.join("license.txt"), "LIC00000000000000").unwrap();

        for _ in 0..3 {
            normalize_air_application_xml(&dir).unwrap();
            assert!(
                has_developer_license(&dir),
                "root license.txt overwrote the DevID license"
            );
        }

        let _ = fs::remove_dir_all(&dir);
    }

    /// When only the root license exists it *should* be staged into META-INF/AIR.
    #[test]
    fn normalisation_stages_root_license_when_none_present() {
        let dir = std::env::temp_dir().join(format!("bobba-lic-stage-{}", std::process::id()));
        let air = dir.join("META-INF").join("AIR");
        fs::create_dir_all(&air).unwrap();
        fs::write(
            air.join("application.xml"),
            "<application><id>com.sulake.habboair</id></application>",
        )
        .unwrap();
        fs::write(dir.join("license.txt"), "LIC00000000000000").unwrap();

        normalize_air_application_xml(&dir).unwrap();

        assert!(air.join("license.txt").is_file());
        assert!(!dir.join("license.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// The exact failure seen in the field: namespace 51.3 made AIR exit with
    /// code 5, and pinning it to 51.0 was the single change that fixed it.
    #[test]
    fn descriptor_namespace_is_pinned_to_major_zero() {
        let xml = r#"<application xmlns="http://ns.adobe.com/air/application/51.3"><id>x</id></application>"#;
        let out = clamp_descriptor_namespace(xml);
        assert!(out.contains("air/application/51.0"));
        assert!(!out.contains("51.3"));
        // Nothing else may be disturbed.
        assert!(out.contains("<id>x</id>"));
    }

    #[test]
    fn descriptor_namespace_already_major_zero_is_untouched() {
        let xml = r#"<application xmlns="http://ns.adobe.com/air/application/51.0"><id>x</id></application>"#;
        assert_eq!(clamp_descriptor_namespace(xml), xml);
    }

    #[test]
    fn descriptor_namespace_clamp_is_idempotent_and_safe() {
        let xml = r#"<application xmlns="http://ns.adobe.com/air/application/52.7"><id>x</id></application>"#;
        let once = clamp_descriptor_namespace(xml);
        assert_eq!(clamp_descriptor_namespace(&once), once);
        assert!(once.contains("air/application/52.0"));

        // No namespace at all: leave the document alone rather than corrupt it.
        let plain = "<application><id>x</id></application>";
        assert_eq!(clamp_descriptor_namespace(plain), plain);
    }

    /// Builds the on-disk shape a freshly extracted AirPlus install has:
    /// the patch's 51.3 descriptor, the runtime shell's root license.txt and
    /// root application.xml, and no backup yet.
    fn staged_install(dir: &Path, with_backup: bool) {
        let air = dir.join("META-INF").join("AIR");
        fs::create_dir_all(&air).unwrap();

        let shipped = concat!(
            r#"<application xmlns="http://ns.adobe.com/air/application/51.3">"#,
            "<id>com.sulake.habboair</id>",
            "<extensions><extensionID>com.sulake.discord.richpresence</extensionID></extensions>",
            "</application>"
        );
        fs::write(air.join("application.xml"), shipped).unwrap();
        if with_backup {
            fs::write(air.join("application.original.xml"), shipped).unwrap();
        }
        // DevID license from the client patch, plus the runtime shell's root one.
        fs::write(air.join("license.txt"), "DevID=abc\nLicense=def").unwrap();
        fs::write(dir.join("license.txt"), "LIC00000000").unwrap();
        fs::write(dir.join("application.xml"), shipped).unwrap();
        fs::write(dir.join("HabboAir.swf"), b"FWS\x33rest-of-swf").unwrap();
    }

    fn ns_of(path: &Path) -> String {
        let raw = fs::read_to_string(path).unwrap();
        let marker = "air/application/";
        let start = raw.find(marker).expect("namespace present") + marker.len();
        let rest = &raw[start..];
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        rest[..end].to_string()
    }

    /// A full re-download must produce a launchable bundle: corrected namespace
    /// in the manifest *and* in the backup that gets restored before each launch.
    #[test]
    fn fresh_install_produces_a_launchable_descriptor() {
        let dir = std::env::temp_dir().join(format!("bobba-fresh-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        staged_install(&dir, false);

        normalize_air_application_xml(&dir).unwrap();

        let air = dir.join("META-INF").join("AIR");
        assert_eq!(ns_of(&air.join("application.xml")), "51.0");
        // ensure_backups snapshots after the correction, so the backup is clean.
        assert_eq!(ns_of(&air.join("application.original.xml")), "51.0");
        // DevID license preserved, staged root copies removed.
        assert!(has_developer_license(&dir));
        assert!(!dir.join("application.xml").exists());

        let _ = fs::remove_dir_all(&dir);
    }

    /// An install already on disk with the bad namespace in both files gets
    /// repaired in place, with no re-download.
    #[test]
    fn existing_install_is_repaired_in_place_and_stays_repaired() {
        let dir = std::env::temp_dir().join(format!("bobba-existing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        staged_install(&dir, true);

        let air = dir.join("META-INF").join("AIR");
        assert_eq!(ns_of(&air.join("application.original.xml")), "51.3");

        // Runs before every launch, so it must converge and stay converged.
        for _ in 0..3 {
            normalize_air_application_xml(&dir).unwrap();
            assert_eq!(ns_of(&air.join("application.xml")), "51.0");
            assert_eq!(ns_of(&air.join("application.original.xml")), "51.0");
            assert!(has_developer_license(&dir));
        }

        let _ = fs::remove_dir_all(&dir);
    }

    /// Nothing binds us to an id when no license is present at all.
    #[test]
    fn missing_license_is_treated_as_unrestricted() {
        let dir = std::env::temp_dir().join(format!("bobba-lic-none-{}", std::process::id()));
        fs::create_dir_all(dir.join("META-INF").join("AIR")).unwrap();
        assert!(has_developer_license(&dir));
        let _ = fs::remove_dir_all(&dir);
    }
}

/// True when an AIR install looks complete enough to launch.
pub fn is_install_healthy(dir: &Path) -> bool {
    dir.join("Habbo.exe").is_file()
        && dir.join("HabboAir.swf").is_file()
        && dir
            .join("META-INF")
            .join("AIR")
            .join("application.xml")
            .is_file()
}

/// Executable a Unity build is launched through.
pub fn unity_exe(dir: &Path) -> PathBuf {
    dir.join("StandaloneWindows")
        .join("habbo2020-global-prod.exe")
}

/// Origins ships one executable per server (`HabboHotel-ous.exe`, `-oes`, `-obr`),
/// optionally with an `-xl` variant. An install is usable if any of them exist.
pub fn origins_exe(dir: &Path, server_suffix: &str, xl: bool) -> PathBuf {
    let xl = if xl { "-xl" } else { "" };
    dir.join(format!("HabboHotel-o{server_suffix}{xl}.exe"))
}

fn has_any_origins_exe(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        let name = e.file_name().to_string_lossy().to_ascii_lowercase();
        name.starts_with("habbohotel-o") && name.ends_with(".exe")
    })
}

/// Health check appropriate to the client's packaging.
pub fn is_install_healthy_for(id: ClientId, dir: &Path) -> bool {
    match id.kind() {
        ClientKind::Air => is_install_healthy(dir),
        ClientKind::Unity => unity_exe(dir).is_file(),
        ClientKind::Shockwave => has_any_origins_exe(dir),
    }
}

/// Resolve an already-installed client directory from settings version.
pub fn resolve_install(root: &Path, id: ClientId, version: &str) -> Result<PathBuf, String> {
    let dir = client_dir(root, id, version)?;
    if is_install_healthy_for(id, &dir) {
        Ok(dir)
    } else {
        Err("Client is not installed. Click Install / Update first.".into())
    }
}

/// Repair existing installs before launch. Only AIR bundles need work — the
/// manifest normalisation, licence staging and namespace correction all live
/// there. Unity and Shockwave builds are just verified.
pub async fn repair_if_needed(_app: &AppHandle, id: ClientId, dir: &Path) -> Result<(), String> {
    if id.kind() == ClientKind::Air {
        if !dir.join("Habbo.exe").is_file() || !dir.join("HabboAir.swf").is_file() {
            return Err("Client install is incomplete".into());
        }
        normalize_air_application_xml(dir)?;
    }

    if is_install_healthy_for(id, dir) {
        Ok(())
    } else {
        Err(format!("{} install is incomplete", id.label()))
    }
}
