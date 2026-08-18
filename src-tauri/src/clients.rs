use serde::{Deserialize, Serialize};

use crate::platforms::Platform;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ClientId {
    Classic,
    AirPlus,
    AirBobba,
    Unity,
    Origins,
    Habbox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientKind {
    Air,
    Unity,
    Shockwave,
}

impl ClientId {
    pub const ALL: [ClientId; 6] = [
        ClientId::Classic,
        ClientId::AirBobba,
        ClientId::AirPlus,
        ClientId::Unity,
        ClientId::Origins,
        ClientId::Habbox,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ClientId::Classic => "Classic",
            ClientId::AirPlus => "AirPlus",
            ClientId::AirBobba => "Bobba Client",
            ClientId::Unity => "Unity",
            ClientId::Origins => "Origins",
            ClientId::Habbox => "Habbo X",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            ClientId::Classic => "Official Habbo AIR client.",
            ClientId::AirPlus => "HabboAirPlus — enhanced AIR client.",
            ClientId::AirBobba => "Bobba Client — branded AirPlus build.",
            ClientId::Unity => "Official Unity client.",
            ClientId::Origins => "Habbo Hotel: Origins.",
            ClientId::Habbox => "Habbo X Unity client.",
        }
    }

    pub fn kind(self) -> ClientKind {
        match self {
            ClientId::Classic | ClientId::AirPlus | ClientId::AirBobba => ClientKind::Air,
            ClientId::Unity | ClientId::Habbox => ClientKind::Unity,
            ClientId::Origins => ClientKind::Shockwave,
        }
    }

    pub fn platform(self) -> Platform {
        Platform::of_client(self)
    }

    pub fn download_dir_name(self) -> Option<&'static str> {
        match self {
            ClientId::Classic => Some("air"),
            ClientId::AirPlus => Some("airplus"),
            ClientId::AirBobba => Some("airbobba"),
            ClientId::Unity => Some("unity"),
            ClientId::Origins => Some("shockwave"),
            ClientId::Habbox => Some("habbox"),
        }
    }

    pub fn version_key(self) -> &'static str {
        match self {
            ClientId::Classic => "classic",
            ClientId::AirPlus => "airPlus",
            ClientId::AirBobba => "airBobba",
            ClientId::Unity => "unity",
            ClientId::Origins => "origins",
            ClientId::Habbox => "habbox",
        }
    }

    pub fn supported(self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientStatus {
    pub id: ClientId,
    pub platform: Platform,
    pub label: String,
    pub blurb: String,
    pub supported: bool,
    pub ready: bool,
    pub version: Option<String>,
    pub install_path: Option<String>,
}

pub fn statuses(
    root: &std::path::Path,
    selected_versions: &std::collections::HashMap<String, String>,
) -> Vec<ClientStatus> {
    ClientId::ALL.iter().copied().map(|id| status_of(id, root, selected_versions)).collect()
}

pub fn status_of(
    id: ClientId,
    root: &std::path::Path,
    selected_versions: &std::collections::HashMap<String, String>,
) -> ClientStatus {
    let version = selected_versions.get(id.version_key()).cloned();
    let install_path = version.as_ref().and_then(|v| {
        id.download_dir_name().map(|dir| root.join("downloads").join(dir).join(v))
    });
    let ready = install_path
        .as_ref()
        .map(|p| crate::install::is_install_healthy_for(id, p))
        .unwrap_or(false);

    ClientStatus {
        id,
        platform: id.platform(),
        label: id.label().into(),
        blurb: id.blurb().into(),
        supported: id.supported(),
        ready,
        version,
        install_path: install_path.map(|p| p.display().to_string()),
    }
}
