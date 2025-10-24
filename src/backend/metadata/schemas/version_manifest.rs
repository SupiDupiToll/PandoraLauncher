
use chrono::{DateTime, Utc};
use gpui::SharedString;
use serde::Deserialize;

pub const MOJANG_VERSION_MANIFEST_URL: &'static str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Deserialize, Clone, Debug)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub struct MinecraftVersionManifest {
    pub latest: LatestMinecraftVersions,
    pub versions: Vec<MinecraftVersionLink>
}

#[derive(Deserialize, Clone, Debug)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub struct LatestMinecraftVersions {
    pub release: SharedString,
    pub snapshot: SharedString,
}

#[derive(Deserialize, Clone, Debug)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
#[serde(rename_all = "camelCase")]
pub struct MinecraftVersionLink {
    pub id: SharedString,
    pub r#type: MinecraftVersionType,
    pub url: SharedString,
    pub time: DateTime<Utc>,
    pub release_time: DateTime<Utc>,
    pub sha1: SharedString,
    pub compliance_level: u32
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MinecraftVersionType {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
}


