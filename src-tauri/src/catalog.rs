//! The catalog served by the Worker: which games have mods and the latest package for each.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub schema: u32,
    #[serde(default)]
    pub min_app: Option<String>,
    pub games: Vec<GameEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameEntry {
    pub id: String,
    pub name: String,
    pub steam_app_id: u32,
    #[serde(default)]
    pub hide_if_missing: bool,
    /// Package root, relative to the game's install dir.
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub process_names: Vec<String>,
    #[serde(default)]
    pub logs: Vec<String>,
    #[serde(default)]
    pub crash_globs: Vec<String>,
    #[serde(default)]
    pub art: Option<Art>,
    /// Kept raw so the engine can read its own fields (preserve, ownedDirs, edits).
    pub package: Value,
}

/// Game art, relative to the catalog URL (taken from Steam at publish time).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Art {
    #[serde(default)]
    pub hero: Option<String>,
    #[serde(default)]
    pub logo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInfo {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub summary: String,
}

/// The display/download half of a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageInfo {
    pub version: String,
    #[serde(default, skip_serializing)]
    pub url: String,
    #[serde(default, skip_serializing)]
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub released: String,
    #[serde(default)]
    pub mods: Vec<ModInfo>,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl GameEntry {
    pub fn info(&self) -> Result<PackageInfo, String> {
        serde_json::from_value(self.package.clone()).map_err(|e| format!("Bad catalog entry for {}: {e}", self.id))
    }

    pub fn spec(&self) -> Result<modsync_engine::PackageSpec, String> {
        serde_json::from_value(self.package.clone()).map_err(|e| format!("Bad package spec for {}: {e}", self.id))
    }
}

/// Resolve a package URL relative to the catalog base.
pub fn resolve_url(base: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("{}/{}", base.trim_end_matches('/'), url.trim_start_matches('/'))
    }
}
