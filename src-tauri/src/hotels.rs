//! Habbo hotel server ids — aligned with HabboCustomLauncher.

use serde::{Deserialize, Serialize};

use crate::platforms::Platform;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hotel {
    pub id: String,
    pub host: String,
    /// Short display label, e.g. `COM`, `BR`.
    pub label: String,
}

fn hotel(id: &str, host: &str, label: &str) -> Hotel {
    Hotel {
        id: id.into(),
        host: host.into(),
        label: label.into(),
    }
}

/// Regular Habbo Hotel hotels — these are the ids that appear in SSO tickets.
pub fn hotels() -> Vec<Hotel> {
    vec![
        hotel("hhus", "www.habbo.com", "COM"),
        hotel("hhbr", "www.habbo.com.br", "BR"),
        hotel("hhes", "www.habbo.es", "ES"),
        hotel("hhfr", "www.habbo.fr", "FR"),
        hotel("hhde", "www.habbo.de", "DE"),
        hotel("hhit", "www.habbo.it", "IT"),
        hotel("hhnl", "www.habbo.nl", "NL"),
        hotel("hhfi", "www.habbo.fi", "FI"),
        hotel("hhtr", "www.habbo.com.tr", "TR"),
        hotel("hhs2", "sandbox.habbo.com", "Sandbox"),
    ]
}

/// Habbo X server ids. HabboLauncher keys off exactly these to recognise a
/// Habbo X ticket: `hhxp` is production, `hhxd` the dev/test shard.
pub fn habbo_x_hotels() -> Vec<Hotel> {
    vec![
        hotel("hhxp", "www.habbox.game", "Live"),
        hotel("hhxd", "www.habbox.game", "Dev"),
    ]
}

/// Hotels selectable for a platform. Origins returns none — it uses a server
/// choice instead (see [`crate::platforms::OriginsServer`]).
pub fn for_platform(platform: Platform) -> Vec<Hotel> {
    match platform {
        Platform::HabboHotel => hotels(),
        Platform::HabboX => habbo_x_hotels(),
        Platform::Origins => Vec::new(),
    }
}

/// Every known hotel across platforms, for id lookups.
fn all_hotels() -> Vec<Hotel> {
    let mut v = hotels();
    v.extend(habbo_x_hotels());
    v
}

pub fn host_for_server_id(server_id: &str) -> Option<String> {
    hotels()
        .into_iter()
        .find(|h| h.id.eq_ignore_ascii_case(server_id))
        .map(|h| h.host)
}

/// Like [`host_for_server_id`] but also resolves Habbo X ids.
pub fn any_host_for_server_id(server_id: &str) -> Option<String> {
    all_hotels()
        .into_iter()
        .find(|h| h.id.eq_ignore_ascii_case(server_id))
        .map(|h| h.host)
}

/// Reverse lookup used when a session is captured from a hotel host and we need
/// the `-server` value for the client.
pub fn server_id_for_host(host: &str) -> Option<String> {
    let needle = host
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    hotels()
        .into_iter()
        .find(|h| h.host.eq_ignore_ascii_case(needle))
        .map(|h| h.id)
}

/// `.habbo.com` style cookie domain for a hotel host (leading dot, `www.` removed).
pub fn cookie_domain_for_host(host: &str) -> String {
    let bare = host.trim().trim_start_matches("www.");
    format!(".{bare}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn habbo_x_ids_resolve_only_through_the_any_lookup() {
        // Deliberate: the regular-hotel lookup must not claim Habbo X ids, or
        // ticket platform detection would misfile them.
        assert!(host_for_server_id("hhxp").is_none());
        assert_eq!(
            any_host_for_server_id("hhxp").as_deref(),
            Some("www.habbox.game")
        );
    }

    #[test]
    fn origins_has_no_hotels_to_pick() {
        assert!(for_platform(Platform::Origins).is_empty());
        assert!(!for_platform(Platform::HabboHotel).is_empty());
        assert!(!for_platform(Platform::HabboX).is_empty());
    }

    #[test]
    fn host_and_id_round_trip() {
        for h in hotels() {
            assert_eq!(server_id_for_host(&h.host).as_deref(), Some(h.id.as_str()));
        }
    }
}
