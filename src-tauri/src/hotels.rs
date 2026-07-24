//! Habbo hotel server ids — aligned with HabboCustomLauncher.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hotel {
    pub id: String,
    pub host: String,
}

pub fn hotels() -> Vec<Hotel> {
    vec![
        Hotel { id: "hhus".into(), host: "www.habbo.com".into() },
        Hotel { id: "hhbr".into(), host: "www.habbo.com.br".into() },
        Hotel { id: "hhes".into(), host: "www.habbo.es".into() },
        Hotel { id: "hhfr".into(), host: "www.habbo.fr".into() },
        Hotel { id: "hhde".into(), host: "www.habbo.de".into() },
        Hotel { id: "hhit".into(), host: "www.habbo.it".into() },
        Hotel { id: "hhnl".into(), host: "www.habbo.nl".into() },
        Hotel { id: "hhfi".into(), host: "www.habbo.fi".into() },
        Hotel { id: "hhtr".into(), host: "www.habbo.com.tr".into() },
        Hotel { id: "hhs2".into(), host: "sandbox.habbo.com".into() },
    ]
}

pub fn host_for_server_id(server_id: &str) -> Option<String> {
    hotels()
        .into_iter()
        .find(|h| h.id.eq_ignore_ascii_case(server_id))
        .map(|h| h.host)
}
