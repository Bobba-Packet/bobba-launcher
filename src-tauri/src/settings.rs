use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::clients::ClientId;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub selected: ClientId,
    /// Default hotel host for Classic clienturls when no ticket yet (e.g. www.habbo.com).
    pub default_hotel_host: String,
    /// Last known installed version id per client key.
    pub versions: HashMap<String, String>,
    /// When true, download and install launcher updates from GitHub Releases automatically.
    #[serde(default = "default_true")]
    pub auto_download_updates: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            selected: ClientId::AirPlus,
            default_hotel_host: "www.habbo.com".into(),
            versions: HashMap::new(),
            auto_download_updates: true,
        }
    }
}

impl Settings {
    fn key(id: ClientId) -> &'static str {
        match id {
            ClientId::Classic => "classic",
            ClientId::AirPlus => "airPlus",
            ClientId::AirBobba => "airBobba",
        }
    }

    pub fn version_of(&self, id: ClientId) -> Option<String> {
        self.versions.get(Self::key(id)).cloned()
    }

    pub fn set_version(&mut self, id: ClientId, version: String) {
        self.versions.insert(Self::key(id).into(), version);
    }

    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let raw = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, raw).map_err(|e| e.to_string())
    }
}
