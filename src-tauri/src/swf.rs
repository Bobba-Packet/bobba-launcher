//! Custom SWF handling and per-avatar AIR application id — ported from
//! HabboLauncher's `Launcher.ChangeFlashSwf` / `Launcher.SetFlashApplicationId`
//! and `Updater.DownloadCustomSWF`.
//!
//! Files kept inside a client install directory:
//!   HabboAir.swf                      — the file the AIR runtime loads
//!   HabboAir.original.swf             — pristine backup of the shipped SWF
//!   HabboAir.custom.swf               — the user's custom build
//!   META-INF/AIR/application.xml      — live manifest
//!   META-INF/AIR/application.original.xml — backup of the normalized manifest
//!
//! A version-independent master copy of the custom SWF also lives at the app
//! data root so newly-installed client versions can be re-patched without
//! re-downloading.

use std::fs;
use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::install;

const SWF_LIVE: &str = "HabboAir.swf";
const SWF_BACKUP: &str = "HabboAir.original.swf";
const SWF_CUSTOM: &str = "HabboAir.custom.swf";
const XML_BACKUP: &str = "application.original.xml";

/// HabboCustomLauncher forces SWF header version 51 on Windows; a custom SWF
/// swapped in must get the same treatment or the AIR runtime refuses it.
const FORCED_SWF_VERSION: u8 = 51;

fn air_xml(client_dir: &Path) -> PathBuf {
    client_dir.join("META-INF").join("AIR").join("application.xml")
}

fn air_xml_backup(client_dir: &Path) -> PathBuf {
    client_dir.join("META-INF").join("AIR").join(XML_BACKUP)
}

/// Master custom-SWF cache path, shared across client versions.
pub fn master_custom_swf(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(install::data_root(app)?.join(SWF_CUSTOM))
}

/// Creates the pristine backups if they don't exist yet. Must run before any
/// swap, otherwise the stock SWF/manifest is lost on first custom launch.
pub fn ensure_backups(client_dir: &Path) -> Result<(), String> {
    let live = client_dir.join(SWF_LIVE);
    let backup = client_dir.join(SWF_BACKUP);
    if live.is_file() && !backup.is_file() {
        fs::copy(&live, &backup).map_err(|e| format!("Failed to back up {SWF_LIVE}: {e}"))?;
    }

    let xml = air_xml(client_dir);
    let xml_backup = air_xml_backup(client_dir);
    if xml.is_file() && !xml_backup.is_file() {
        fs::copy(&xml, &xml_backup)
            .map_err(|e| format!("Failed to back up application.xml: {e}"))?;
    }
    Ok(())
}

/// Downloads the custom SWF to the master cache and mirrors it into the client
/// install. Mirrors `Updater.DownloadCustomSWF`.
pub async fn download_custom_swf(
    app: &AppHandle,
    link: &str,
    client_dir: Option<&Path>,
) -> Result<(), String> {
    if link.trim().is_empty() {
        return Err("No custom SWF link configured".into());
    }
    let url = link.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("Custom SWF link must be an http(s) URL".into());
    }

    let master = master_custom_swf(app)?;
    install::download_to(app, url, &master, "custom HabboAir.swf").await?;

    if let Some(dir) = client_dir {
        mirror_master_into(app, dir)?;
    }
    Ok(())
}

/// Copies the master custom SWF into a client install directory, if present.
pub fn mirror_master_into(app: &AppHandle, client_dir: &Path) -> Result<(), String> {
    let master = master_custom_swf(app)?;
    if !master.is_file() {
        return Ok(());
    }
    let dest = client_dir.join(SWF_CUSTOM);
    fs::copy(&master, &dest).map_err(|e| format!("Failed to stage custom SWF: {e}"))?;
    Ok(())
}

/// Selects which SWF the client will load, and resets the manifest when using
/// a custom build.
///
/// Stock  → restore `HabboAir.original.swf`; the caller then rewrites the app
///          id for per-avatar isolation.
/// Custom → restore the original manifest (custom builds are compiled against
///          the stock AIR identity and break when `<id>` is rewritten), then
///          install `HabboAir.custom.swf`.
///
/// Returns true when a custom SWF is now active.
pub fn apply_swf_choice(
    app: &AppHandle,
    client_dir: &Path,
    use_custom: bool,
) -> Result<bool, String> {
    ensure_backups(client_dir)?;

    let live = client_dir.join(SWF_LIVE);
    let backup = client_dir.join(SWF_BACKUP);

    if !use_custom {
        if backup.is_file() {
            fs::copy(&backup, &live)
                .map_err(|e| format!("Failed to restore original {SWF_LIVE}: {e}"))?;
            set_swf_version(&live, FORCED_SWF_VERSION)?;
        }
        return Ok(false);
    }

    // Make sure a per-install copy exists (a fresh client version won't have one).
    let custom = client_dir.join(SWF_CUSTOM);
    if !custom.is_file() {
        mirror_master_into(app, client_dir)?;
    }
    if !custom.is_file() {
        return Err(
            "Custom SWF is enabled but no file has been downloaded yet. Set a link and download it first."
                .into(),
        );
    }

    fs::copy(&custom, &live).map_err(|e| format!("Failed to install custom SWF: {e}"))?;
    set_swf_version(&live, FORCED_SWF_VERSION)?;
    Ok(true)
}

/// Restores the pristine AIR manifest from backup, discarding any per-avatar
/// application id a previous launch wrote.
pub fn restore_manifest(client_dir: &Path) -> Result<(), String> {
    let xml = air_xml(client_dir);
    let backup = air_xml_backup(client_dir);
    if backup.is_file() {
        fs::copy(&backup, &xml)
            .map_err(|e| format!("Failed to restore application.xml: {e}"))?;
    }
    Ok(())
}

/// Rewrites the AIR application id to `com.sulake.habboair.<avatarId>`.
///
/// This is what gives each account its **own machine id**. AIR derives its
/// application storage directory (`%APPDATA%\<appId>`) and encrypted local store
/// from the application id, and the Habbo client keeps its generated machine id
/// there. A distinct id per avatar therefore means a distinct machine id per
/// account, which keeps accounts from being linked by device — and as a bonus
/// lets several clients run side by side.
///
/// Only safe when the bundle carries a DevID license; see
/// [`crate::install::has_developer_license`]. Port of HabboLauncher's
/// `SetFlashApplicationId`.
pub fn set_flash_application_id(client_dir: &Path, ticket: &str) -> Result<String, String> {
    let xml_path = air_xml(client_dir);
    if !xml_path.is_file() {
        return Err("META-INF/AIR/application.xml is missing".into());
    }

    let avatar_id = avatar_id_from_ticket(ticket);
    let app_id = format!("com.sulake.habboair.{avatar_id}");

    let xml = fs::read_to_string(&xml_path).map_err(|e| e.to_string())?;
    let updated = replace_first_element(&xml, "id", &app_id);
    let updated = replace_first_element(&updated, "title", "Habbo");
    fs::write(&xml_path, updated).map_err(|e| e.to_string())?;

    Ok(app_id)
}

/// Extracts the trailing numeric avatar id from an SSO ticket.
///
/// Equivalent to the C# regex `[a-z0-9\-]+-([0-9]+)` with greedy matching: for
/// `abc-123-456` the result is `456`. Implemented by scanning hyphens and
/// keeping the last one followed by a run of digits, which avoids pulling in a
/// regex dependency for a single pattern. Falls back to a generated id when the
/// ticket has no numeric tail, mirroring the original's `Guid.NewGuid("N")`.
fn avatar_id_from_ticket(ticket: &str) -> String {
    let mut found: Option<&str> = None;
    for (i, b) in ticket.bytes().enumerate() {
        if b != b'-' {
            continue;
        }
        let rest = &ticket[i + 1..];
        let end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        if end > 0 {
            found = Some(&rest[..end]);
        }
    }
    found
        .map(str::to_string)
        .unwrap_or_else(crate::settings::new_id)
}

/// Replaces the text content of the first `<tag>…</tag>` occurrence.
/// Leaves the document untouched when the tag isn't present.
fn replace_first_element(xml: &str, tag: &str, value: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let Some(start) = xml.find(&open) else {
        return xml.to_string();
    };
    let content_start = start + open.len();
    let Some(rel_end) = xml[content_start..].find(&close) else {
        return xml.to_string();
    };
    let content_end = content_start + rel_end;

    let mut out = String::with_capacity(xml.len() + value.len());
    out.push_str(&xml[..content_start]);
    out.push_str(value);
    out.push_str(&xml[content_end..]);
    out
}

/// Patches the SWF header version byte (4th byte).
fn set_swf_version(path: &Path, version: u8) -> Result<(), String> {
    let mut data = fs::read(path).map_err(|e| e.to_string())?;
    if data.len() < 4 {
        return Err("HabboAir.swf is too small to be valid".into());
    }
    data[3] = version;
    fs::write(path, data).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The AIR manifest must come back byte-identical to the backup, because
    /// HARMAN's license is bound to the `<id>` inside it.
    #[test]
    fn restore_manifest_reverts_a_rewritten_app_id() {
        let dir = std::env::temp_dir().join(format!("bobba-swf-test-{}", std::process::id()));
        let air = dir.join("META-INF").join("AIR");
        fs::create_dir_all(&air).unwrap();

        let pristine = "<application><id>com.sulake.habboair</id></application>";
        fs::write(air.join("application.original.xml"), pristine).unwrap();
        fs::write(
            air.join("application.xml"),
            "<application><id>com.sulake.habboair.999</id></application>",
        )
        .unwrap();

        restore_manifest(&dir).unwrap();

        let got = fs::read_to_string(air.join("application.xml")).unwrap();
        assert_eq!(got, pristine);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_manifest_is_a_noop_without_a_backup() {
        let dir = std::env::temp_dir().join(format!("bobba-swf-nb-{}", std::process::id()));
        fs::create_dir_all(dir.join("META-INF").join("AIR")).unwrap();
        assert!(restore_manifest(&dir).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Greedy semantics: the *last* hyphen-prefixed digit run wins, matching the
    /// C# regex this replaces.
    #[test]
    fn avatar_id_takes_the_trailing_numeric_group() {
        assert_eq!(avatar_id_from_ticket("abc-123-456"), "456");
        // Shape of a real Habbo SSO ticket.
        assert_eq!(
            avatar_id_from_ticket("b6fa0b5a-268a-46a6-988d-3a684b45067c-92779355.V4"),
            "92779355"
        );
    }

    #[test]
    fn avatar_id_falls_back_when_there_is_no_numeric_tail() {
        assert!(!avatar_id_from_ticket("nodigitshere").is_empty());
        assert!(!avatar_id_from_ticket("").is_empty());
    }

    #[test]
    fn app_id_rewrite_only_touches_id_and_title() {
        let dir = std::env::temp_dir().join(format!("bobba-swf-id-{}", std::process::id()));
        let air = dir.join("META-INF").join("AIR");
        fs::create_dir_all(&air).unwrap();
        fs::write(
            air.join("application.xml"),
            "<application><id>com.sulake.habboair</id><name>Habbo</name>\
             <initialWindow><content>HabboAir.swf</content><title>x</title></initialWindow></application>",
        )
        .unwrap();

        let app_id = set_flash_application_id(&dir, "aaa-bbb-777").unwrap();
        assert_eq!(app_id, "com.sulake.habboair.777");

        let got = fs::read_to_string(air.join("application.xml")).unwrap();
        assert!(got.contains("<id>com.sulake.habboair.777</id>"));
        assert!(got.contains("<title>Habbo</title>"));
        // Untouched elements must survive verbatim.
        assert!(got.contains("<content>HabboAir.swf</content>"));
        assert!(got.contains("<name>Habbo</name>"));

        let _ = fs::remove_dir_all(&dir);
    }
}
