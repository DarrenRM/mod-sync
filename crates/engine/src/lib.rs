//! Install engine for Mod Sync.
//!
//! Installs an extracted package into a game folder under an undo log, records
//! everything it touched in an [`InstallRecord`] (written in the same
//! transaction), and can replay that record backwards so the game folder ends
//! up byte for byte as it was, apart from files matching the package's
//! `preserve` list that the player made themselves.

mod archive;
mod error;
mod fsx;
mod install;
mod journal;
pub mod lines;
mod process;
pub mod steam;
mod types;

pub use archive::extract_zip;
pub use error::{Error, Result, UninstallError};
pub use fsx::{normalize_rel, sha256_file};
pub use install::{apply, uninstall, verify, VerifyReport};
pub use journal::recover;
pub use process::running_processes;
pub use types::{Edit, EditEntry, FileEntry, InstallRecord, PackageSpec};
