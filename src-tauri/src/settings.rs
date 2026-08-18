use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::clients::ClientId;
use crate::platforms::{OriginsServer, Platform};

fn default_platform() -> Platform {
    Platform::HabboHotel
}

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn new_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}{seq:x}")
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GEarthSettings {
    pub enabled: bool,
    pub path: String,
    #[serde(default)]
    pub origins_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CustomSwfSettings {
    pub enabled: bool,
    pub link: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub selected: ClientId,
    #[serde(default = "default_platform")]
    pub platform: Platform,
    #[serde(default)]
    pub origins_server: OriginsServer,
    #[serde(default)]
    pub origins_xl: bool,
    pub default_hotel_host: String,
    pub versions: HashMap<String, String>,
    #[serde(default = "default_true")]
    pub auto_download_updates: bool,
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
    /// Machine-id isolation (per-avatar AIR app id). Disable if it causes issues.
    #[serde(default = "default_true")]
    pub machine_id_isolation: bool,
    /// Seconds before auto-launching when a ticket is detected. 0 = disabled.
    #[serde(default = "default_launch_delay")]
    pub auto_launch_delay: u32,
    #[serde(default)]
    pub g_earth: GEarthSettings,
    #[serde(default)]
    pub custom_swf: CustomSwfSettings,
    /// Platforms the user has hidden from the selector.
    #[serde(default)]
    pub hidden_platforms: Vec<Platform>,
    /// Origins servers hidden from the selector.
    #[serde(default)]
    pub hidden_origins_servers: Vec<OriginsServer>,
    /// Clients that should not auto-update on launch.
    #[serde(default)]
    pub skip_update_clients: Vec<ClientId>,
}

fn default_true() -> bool {
    true
}

fn default_launch_delay() -> u32 {
    5
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            selected: ClientId::AirPlus,
            platform: Platform::HabboHotel,
            origins_server: OriginsServer::default(),
            origins_xl: false,
            default_hotel_host: "www.habbo.com".into(),
            versions: HashMap::new(),
            auto_download_updates: true,
            minimize_to_tray: true,
            machine_id_isolation: true,
            auto_launch_delay: 5,
            g_earth: GEarthSettings::default(),
            custom_swf: CustomSwfSettings::default(),
            hidden_platforms: Vec::new(),
            hidden_origins_servers: Vec::new(),
            skip_update_clients: Vec::new(),
        }
    }
}

impl Settings {
    pub fn version_of(&self, id: ClientId) -> Option<String> {
        self.versions.get(id.version_key()).cloned()
    }

    pub fn set_version(&mut self, id: ClientId, version: String) {
        self.versions.insert(id.version_key().into(), version);
    }

    pub fn set_platform(&mut self, platform: Platform) {
        self.platform = platform;
        if !platform.owns_client(self.selected) {
            self.selected = platform.default_client();
        }
    }

    pub fn set_selected_client(&mut self, id: ClientId) {
        self.selected = id;
        self.platform = id.platform();
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
