use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub auto_update: bool,
    /// Start with Windows (hidden, in the tray) and keep running when the window closes.
    pub background: bool,
    pub share_logs: bool,
    /// Manually located game folders, by game id.
    pub paths: BTreeMap<String, String>,
    /// Unix seconds of the last crash-dump scan.
    pub last_crash_scan: u64,
    /// The first-launch Settings sheet has been shown and closed.
    pub welcomed: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_update: true,
            background: false,
            share_logs: true,
            paths: BTreeMap::new(),
            last_crash_scan: 0,
            welcomed: false,
        }
    }
}

/// What the UI sees and edits.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicSettings {
    pub auto_update: bool,
    pub background: bool,
    pub share_logs: bool,
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        crate::util::write_json_atomic(path, self)
    }

    pub fn public(&self) -> PublicSettings {
        PublicSettings { auto_update: self.auto_update, background: self.background, share_logs: self.share_logs }
    }
}
