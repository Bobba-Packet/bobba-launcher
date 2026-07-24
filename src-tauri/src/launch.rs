//! Process launch — HabboCustomLauncher args: `-server <id> -ticket <sso>`

use std::fs;
use std::path::Path;
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::ticket::LoginTicket;

/// Windows: create a fully detached process so the client outlives the launcher command.
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x00000008;

pub fn launch(client_dir: &Path, ticket: &LoginTicket) -> Result<(), String> {
    let exe = client_dir.join("Habbo.exe");
    if !exe.is_file() {
        return Err(format!("Habbo.exe not found in {}", client_dir.display()));
    }

    let meta = client_dir.join("META-INF").join("AIR").join("application.xml");
    if !meta.is_file() {
        return Err("META-INF/AIR/application.xml is missing from the client install".into());
    }

    // HabboCustomLauncher LaunchClient + UpdateAirApplicationXML end state:
    // - META-INF/AIR/application.xml is the source of truth
    // - root application.xml is deleted before Habbo.exe starts
    finalize_xml_for_launch(client_dir)?;

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

    cmd.spawn()
        .map_err(|e| format!("Failed to start Habbo.exe: {e}"))?;

    Ok(())
}

fn finalize_xml_for_launch(client_dir: &Path) -> Result<(), String> {
    let meta = client_dir.join("META-INF").join("AIR").join("application.xml");
    let root = client_dir.join("application.xml");

    // Normalize META to match a known-good AirPlus/Classic AIR profile
    let mut xml = fs::read_to_string(&meta).map_err(|e| e.to_string())?;
    xml = strip_extensions_block(&xml);
    xml = ensure_encrypted_local_storage(&xml);
    fs::write(&meta, xml).map_err(|e| e.to_string())?;

    // Capture runtime reads META-INF/AIR; HCL removes the staged root copy
    if root.is_file() {
        let _ = fs::remove_file(&root);
    }

    // HCL moves license.txt into META-INF/AIR when present
    let license = client_dir.join("license.txt");
    let license_dest = client_dir.join("META-INF").join("AIR").join("license.txt");
    if license.is_file() {
        let _ = fs::rename(&license, &license_dest);
    }

    Ok(())
}

fn strip_extensions_block(xml: &str) -> String {
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

fn ensure_encrypted_local_storage(xml: &str) -> String {
    if xml.contains("<encryptedLocalStorage>") {
        return xml.to_string();
    }
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
