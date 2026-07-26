//! Login ticket parsing — HabboCustomLauncher-compatible formats.

use serde::{Deserialize, Serialize};

use crate::hotels;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginTicket {
    pub server_id: String,
    pub sso_ticket: String,
    pub server_host: String,
    pub username: Option<String>,
}

/// Parse clipboard / paste content:
/// - `habbo://hab?server=hhes&token=<uuid>.V4[.username]`
/// - `hhes.<uuid>.V4[.username]`
pub fn parse_ticket(raw: &str) -> Option<LoginTicket> {
    let mut code = raw.trim().to_string();
    if code.is_empty() {
        return None;
    }

    if code.starts_with("habbo://") && code.contains("server=") {
        let idx = code.find("?server=")?;
        code = code[idx + "?server=".len()..].to_string();
        code = code.replace("&token=", ".");
    }

    let parts: Vec<&str> = code.split('.').collect();
    if parts.len() < 3 {
        return None;
    }

    let server_id = parts[0].to_string();
    let host = hotels::host_for_server_id(&server_id)?;
    let sso_ticket = format!("{}.{}", parts[1], parts[2]);
    // Skip empty segments, e.g. `…V4..carol` → "carol"
    let username = parts
        .get(3..)
        .map(|rest| {
            rest.iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(".")
        })
        .filter(|name| !name.is_empty());

    Some(LoginTicket {
        server_id,
        sso_ticket,
        server_host: host,
        username,
    })
}
