//! Game platforms — Habbo Hotel, Habbo Hotel: Origins, and Habbo X.

use serde::{Deserialize, Serialize};

use crate::clients::ClientId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Platform {
    HabboHotel,
    Origins,
    HabboX,
}

impl Platform {
    pub const ALL: [Platform; 3] = [Platform::HabboHotel, Platform::Origins, Platform::HabboX];

    pub fn label(self) -> &'static str {
        match self {
            Platform::HabboHotel => "Habbo Hotel",
            Platform::Origins => "Habbo Hotel: Origins",
            Platform::HabboX => "Habbo X",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Platform::HabboHotel => "The modern hotel — Flash/AIR or Unity.",
            Platform::Origins => "The 2000s revival. Pick a server; no ticket needed.",
            Platform::HabboX => "Habbo X.",
        }
    }

    pub fn clients(self) -> &'static [ClientId] {
        match self {
            Platform::HabboHotel => &[
                ClientId::Classic,
                ClientId::AirBobba,
                ClientId::AirPlus,
                ClientId::Unity,
            ],
            Platform::Origins => &[ClientId::Origins],
            Platform::HabboX => &[ClientId::Habbox],
        }
    }

    pub fn needs_ticket(self) -> bool {
        !matches!(self, Platform::Origins)
    }

    pub fn needs_server_choice(self) -> bool {
        matches!(self, Platform::Origins)
    }

    pub fn gamedata_host(self) -> &'static str {
        match self {
            Platform::HabboHotel => "www.habbo.com",
            Platform::Origins => "origins.habbo.com",
            Platform::HabboX => "www.habbox.game",
        }
    }

    pub fn default_client(self) -> ClientId {
        match self {
            Platform::HabboHotel => ClientId::AirBobba,
            Platform::Origins => ClientId::Origins,
            Platform::HabboX => ClientId::Habbox,
        }
    }

    pub fn owns_client(self, id: ClientId) -> bool {
        self.clients().contains(&id)
    }

    pub fn of_client(id: ClientId) -> Platform {
        Platform::ALL
            .into_iter()
            .find(|p| p.owns_client(id))
            .unwrap_or(Platform::HabboHotel)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OriginsServer {
    Com,
    Es,
    Br,
}

impl OriginsServer {
    pub const ALL: [OriginsServer; 3] = [OriginsServer::Com, OriginsServer::Es, OriginsServer::Br];

    pub fn exe_suffix(self) -> &'static str {
        match self {
            OriginsServer::Com => "us",
            OriginsServer::Es => "es",
            OriginsServer::Br => "br",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            OriginsServer::Com => "COM",
            OriginsServer::Es => "ES",
            OriginsServer::Br => "BR",
        }
    }

    pub fn host(self) -> &'static str {
        match self {
            OriginsServer::Com => "origins.habbo.com",
            OriginsServer::Es => "origins.habbo.es",
            OriginsServer::Br => "origins.habbo.com.br",
        }
    }
}

impl Default for OriginsServer {
    fn default() -> Self {
        OriginsServer::Com
    }
}

/// Detect which platform a ticket's server id belongs to.
/// `hhxd`/`hhxp` → Habbo X, other known ids → Habbo Hotel.
pub fn platform_for_server_id(server_id: &str) -> Option<Platform> {
    let id = server_id.trim().to_ascii_lowercase();
    if id.is_empty() {
        return None;
    }
    if matches!(id.as_str(), "hhxd" | "hhxp") {
        return Some(Platform::HabboX);
    }
    if crate::hotels::host_for_server_id(&id).is_some() {
        return Some(Platform::HabboHotel);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn habbo_x_detection() {
        assert_eq!(platform_for_server_id("hhxd"), Some(Platform::HabboX));
        assert_eq!(platform_for_server_id("hhxp"), Some(Platform::HabboX));
        assert_eq!(platform_for_server_id("HHXP"), Some(Platform::HabboX));
    }

    #[test]
    fn regular_hotel_detection() {
        for id in ["hhus", "hhbr", "hhes", "hhfi", "hhs2"] {
            assert_eq!(platform_for_server_id(id), Some(Platform::HabboHotel));
        }
    }

    #[test]
    fn unknown_ids() {
        assert_eq!(platform_for_server_id("nonsense"), None);
        assert_eq!(platform_for_server_id(""), None);
    }

    #[test]
    fn platform_ticket_requirements() {
        assert!(!Platform::Origins.needs_ticket());
        assert!(Platform::Origins.needs_server_choice());
        assert!(Platform::HabboHotel.needs_ticket());
        assert!(Platform::HabboX.needs_ticket());
    }

    #[test]
    fn every_client_has_one_platform() {
        for id in ClientId::ALL {
            let owners: Vec<_> = Platform::ALL.into_iter().filter(|p| p.owns_client(id)).collect();
            assert_eq!(owners.len(), 1);
        }
    }
}
