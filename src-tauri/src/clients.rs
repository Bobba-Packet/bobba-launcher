use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ClientId {
    Classic,
    AirPlus,
    AirBobba,
}

impl ClientId {
    pub const ALL: [ClientId; 3] = [ClientId::Classic, ClientId::AirBobba, ClientId::AirPlus];

    pub fn label(self) -> &'static str {
        match self {
            ClientId::Classic => "Classic",
            ClientId::AirPlus => "AirPlus",
            ClientId::AirBobba => "Bobba Client",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            ClientId::Classic => "Official Habbo AIR client (hotel gamedata).",
            ClientId::AirPlus => "HabboAirPlus — enhanced AIR client.",
            ClientId::AirBobba => "Bobba Client — branded AirPlus build from Bobba Packet.",
        }
    }

    /// Folder name under downloads/ (HabboCustomLauncher convention).
    pub fn download_dir_name(self) -> Option<&'static str> {
        match self {
            ClientId::Classic => Some("air"),
            ClientId::AirPlus => Some("airplus"),
            ClientId::AirBobba => Some("airbobba"),
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
    pub label: String,
    pub blurb: String,
    pub supported: bool,
    pub ready: bool,
    pub version: Option<String>,
    pub install_path: Option<String>,
}

pub fn statuses(root: &std::path::Path, selected_versions: &std::collections::HashMap<String, String>) -> Vec<ClientStatus> {
    ClientId::ALL
        .iter()
        .copied()
        .map(|id| status_of(id, root, selected_versions))
        .collect()
}

pub fn status_of(
    id: ClientId,
    root: &std::path::Path,
    selected_versions: &std::collections::HashMap<String, String>,
) -> ClientStatus {
    let key = match id {
        ClientId::Classic => "classic",
        ClientId::AirPlus => "airPlus",
        ClientId::AirBobba => "airBobba",
    };
    let version = selected_versions.get(key).cloned();
    let install_path = version.as_ref().and_then(|v| {
        id.download_dir_name().map(|dir| root.join("downloads").join(dir).join(v))
    });
    let ready = install_path
        .as_ref()
        .map(|p| {
            p.join("Habbo.exe").is_file()
                && p.join("HabboAir.swf").is_file()
                && p
                    .join("META-INF")
                    .join("AIR")
                    .join("application.xml")
                    .is_file()
        })
        .unwrap_or(false);

    ClientStatus {
        id,
        label: id.label().into(),
        blurb: id.blurb().into(),
        supported: id.supported(),
        ready,
        version,
        install_path: install_path.map(|p| p.display().to_string()),
    }
}
