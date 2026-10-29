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
    /// A piece players don't use directly (the mod loader): listed on the game page, left out of
    /// the library row's short list.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub support: bool,
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

impl Catalog {
    /// Drops games whose catalog values would be unsafe as file paths (see [`GameEntry::check_paths`]).
    pub fn checked(mut self) -> Self {
        self.games.retain(|g| match g.check_paths() {
            Ok(()) => true,
            Err(e) => {
                crate::util::log(format!("catalog: skipping a game: {e}"));
                false
            }
        });
        self
    }
}

impl GameEntry {
    /// `id` names files and folders in the app's data folder, and `target` is joined onto the game
    /// folder, so neither may be absolute, climb out with `..`, or name a Windows device.
    pub fn check_paths(&self) -> Result<(), String> {
        if !is_slug(&self.id) || is_device_name(&self.id) {
            return Err(format!("bad id {:?}", self.id));
        }
        if !self.target.is_empty() {
            let ok = modsync_engine::normalize_rel(&self.target)
                .is_ok_and(|t| !t.split('/').any(is_device_name));
            if !ok {
                return Err(format!("bad target {:?} for {}", self.target, self.id));
            }
        }
        Ok(())
    }

    pub fn info(&self) -> Result<PackageInfo, String> {
        serde_json::from_value(self.package.clone()).map_err(|e| format!("Bad catalog entry for {}: {e}", self.id))
    }

    pub fn spec(&self) -> Result<modsync_engine::PackageSpec, String> {
        serde_json::from_value(self.package.clone()).map_err(|e| format!("Bad package spec for {}: {e}", self.id))
    }
}

/// A game id: ASCII letters, digits, `-` and `_`, starting with a letter or digit.
fn is_slug(s: &str) -> bool {
    s.len() <= 64
        && s.starts_with(|c: char| c.is_ascii_alphanumeric())
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A Windows device name (`con`, `nul`, `com1`…), which opens the device rather than a file, even
/// with an extension (`nul.json`) or trailing spaces.
fn is_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").trim_end_matches(' ').to_ascii_lowercase();
    matches!(stem.as_str(), "con" | "prn" | "aux" | "nul" | "conin$" | "conout$")
        || (stem.len() == 4
            && (stem.starts_with("com") || stem.starts_with("lpt"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

/// A catalog value used inside a file name (a package version): ASCII letters, digits, `.`, `-`,
/// `_` and `+`, not starting with `.`.
pub fn is_file_name_safe(s: &str) -> bool {
    s.len() <= 64
        && !s.is_empty()
        && !s.starts_with('.')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
}

/// Resolve a package URL relative to the catalog base.
pub fn resolve_url(base: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("{}/{}", base.trim_end_matches('/'), url.trim_start_matches('/'))
    }
}

#[cfg(test)]
mod tests {
    use super::{is_file_name_safe, Catalog, ModInfo};

    #[test]
    fn catalog_values_that_become_paths_are_checked() {
        let game = |id: &str, target: &str| {
            serde_json::json!({ "id": id, "name": id, "steamAppId": 1, "target": target, "package": {} })
        };
        let cat: Catalog = serde_json::from_value(serde_json::json!({
            "schema": 1,
            "games": [
                game("vein", "Vein/Binaries/Win64"),
                game("vein-server", ""),
                game("../escape", "Vein"),
                game("abs", "C:/Windows"),
                game("rooted", "/Windows"),
                game("climb", "Vein/../../.."),
                game("unc", "\\\\server\\share"),
                game("nul", "Vein"),
                game("COM1", "Vein"),
                game("device-file", "Vein/CON.txt"),
                game("device-dir", "Vein/aux /Binaries"),
                game("console", "Vein/com10/lpt/conf.d"),
            ]
        }))
        .unwrap();
        let ids: Vec<_> = cat.checked().games.into_iter().map(|g| g.id).collect();
        assert_eq!(ids, ["vein", "vein-server", "console"]);

        assert!(is_file_name_safe("0.5.0-r1"));
        assert!(!is_file_name_safe("../0.5.0"));
        assert!(!is_file_name_safe("0.5.0\\..\\x"));
        assert!(!is_file_name_safe(""));
    }

    #[test]
    fn support_flag_reaches_the_ui_only_when_set() {
        let loader: ModInfo = serde_json::from_str(r#"{"name":"Mod Loader","support":true}"#).unwrap();
        assert!(loader.support);
        assert!(serde_json::to_string(&loader).unwrap().contains(r#""support":true"#));
        let m: ModInfo = serde_json::from_str(r#"{"name":"Shared Crafting"}"#).unwrap();
        assert!(!m.support);
        assert!(!serde_json::to_string(&m).unwrap().contains("support"));
    }
}
