//! Login ticket parsing — HabboCustomLauncher-compatible formats.

use serde::{Deserialize, Serialize};

use crate::hotels;
use crate::platforms::{platform_for_server_id, Platform};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginTicket {
    pub server_id: String,
    pub sso_ticket: String,
    pub server_host: String,
    pub username: Option<String>,
    /// Platform this ticket belongs to, derived from the server id. Lets the UI
    /// switch itself to the right product when a ticket is pasted.
    pub platform: Platform,
}

/// Parse clipboard / paste content:
/// - `habbo://hab?server=hhes&token=<uuid>.V4[.username]`
/// - `hhes.<uuid>.V4[.username]`
///
/// Habbo X tickets use the `hhxp` / `hhxd` server ids and are recognised the same
/// way; Origins produces no tickets at all.
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
    // Accept Habbo X ids as well as regular hotels, otherwise a Habbo X ticket
    // would be rejected as unparseable.
    let host = hotels::any_host_for_server_id(&server_id)?;
    let platform = platform_for_server_id(&server_id)?;
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
        platform,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_regular_hotel_ticket() {
        let t = parse_ticket("hhbr.abc-123.V4.carol").expect("parsed");
        assert_eq!(t.server_id, "hhbr");
        assert_eq!(t.sso_ticket, "abc-123.V4");
        assert_eq!(t.server_host, "www.habbo.com.br");
        assert_eq!(t.username.as_deref(), Some("carol"));
        assert_eq!(t.platform, Platform::HabboHotel);
    }

    #[test]
    fn parses_a_habbo_x_ticket_and_tags_the_platform() {
        let t = parse_ticket("hhxp.abc-123.V4").expect("parsed");
        assert_eq!(t.platform, Platform::HabboX);
        assert_eq!(t.server_host, "www.habbox.game");
    }

    #[test]
    fn parses_the_deep_link_form() {
        let t = parse_ticket("habbo://hab?server=hhes&token=abc-9.V4").expect("parsed");
        assert_eq!(t.server_id, "hhes");
        assert_eq!(t.sso_ticket, "abc-9.V4");
        assert_eq!(t.platform, Platform::HabboHotel);
    }

    #[test]
    fn rejects_junk_and_unknown_servers() {
        assert!(parse_ticket("").is_none());
        assert!(parse_ticket("just some text").is_none());
        // Well-formed shape but an unknown hotel must not be guessed at.
        assert!(parse_ticket("hhzz.abc-1.V4").is_none());
    }
}
