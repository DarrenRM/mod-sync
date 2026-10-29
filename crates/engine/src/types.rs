use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The install-relevant part of a catalog `package` object (extra fields ignored).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageSpec {
    pub version: String,
    /// Globs (case-insensitive, `/` separators): never overwritten if present.
    #[serde(default)]
    pub preserve: Vec<String>,
    /// Swept on uninstall (keeping files that match `preserve`), but only if
    /// the install created them.
    #[serde(default)]
    pub owned_dirs: Vec<String>,
    #[serde(default)]
    pub edits: Vec<Edit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Edit {
    Line {
        file: String,
        key: String,
        line: String,
        #[serde(default)]
        before: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRecord {
    pub game_id: String,
    pub version: String,
    pub target: PathBuf,
    /// RFC 3339, UTC.
    pub installed_at: String,
    pub files: Vec<FileEntry>,
    /// Directories this install created, relative to `target`, parents first.
    pub created_dirs: Vec<String>,
    pub edits: Vec<EditEntry>,
    /// Copied from the spec so uninstall knows which created dirs to sweep.
    #[serde(default)]
    pub owned_dirs: Vec<String>,
    /// Copied from the spec so that sweep keeps the user's own config files
    /// (e.g. `config_local.lua`). Records from older apps don't have it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preserve: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    /// Relative to `target`, forward slashes.
    pub path: String,
    pub sha256: String,
    /// Original file we replaced, relative to `backup_root`.
    pub backup: Option<String>,
    /// Matched a `preserve` glob: user edits are expected, so `verify` doesn't
    /// report it as modified.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub preserved: bool,
}

/// One applied line edit and what is needed to reverse it exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditEntry {
    /// Relative to `target`, forward slashes.
    pub file: String,
    pub key: String,
    pub line: String,
    #[serde(default)]
    pub before: Option<String>,
    /// We inserted the line (revert removes it).
    pub added: bool,
    /// We replaced this original line text (no line terminator); revert restores it.
    #[serde(default)]
    pub replaced: Option<String>,
    /// The file did not exist; revert deletes it if nothing else is left in it.
    #[serde(default)]
    pub created_file: bool,
    /// Whether the file ended with a newline before we appended. Revert strips the
    /// newline we had to add to the previous last line when this is false.
    #[serde(default = "yes")]
    pub trailing_newline: bool,
}

fn yes() -> bool {
    true
}

impl EditEntry {
    pub fn change(&self) -> crate::lines::LineChange {
        crate::lines::LineChange {
            added: self.added,
            replaced: self.replaced.clone(),
            created_file: self.created_file,
            trailing_newline: self.trailing_newline,
        }
    }

    /// Whether reverting this entry changes anything.
    pub fn is_noop(&self) -> bool {
        !self.added && self.replaced.is_none()
    }
}
