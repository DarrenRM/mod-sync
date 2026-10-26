use std::io;
use std::path::{Path, PathBuf};

use crate::types::InstallRecord;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{op} {}: {source}", path.display())]
    Io {
        op: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("unsafe path {0:?}")]
    UnsafePath(String),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("bad preserve pattern {pattern:?}: {message}")]
    Pattern { pattern: String, message: String },
    #[error("{0}")]
    Invalid(String),
    /// The operation failed and undoing it failed too. The undo log is kept in
    /// `work_dir`; [`crate::recover`] (or the next `apply`) retries the rollback.
    #[error("{cause}; rollback also failed: {rollback}")]
    RollbackFailed { cause: Box<Error>, rollback: String },
}

impl Error {
    /// True when the failure looks like a file held open by another process
    /// (typically the game is running).
    pub fn is_file_in_use(&self) -> bool {
        match self {
            Error::Io { source, .. } => is_in_use(source),
            Error::RollbackFailed { cause, .. } => cause.is_file_in_use(),
            _ => false,
        }
    }
}

pub(crate) fn is_in_use(e: &io::Error) -> bool {
    // ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION, ERROR_USER_MAPPED_FILE
    matches!(e.raw_os_error(), Some(5 | 32 | 33 | 1224))
}

pub(crate) trait IoCtx<T> {
    fn ctx(self, op: &'static str, path: &Path) -> Result<T>;
}

impl<T> IoCtx<T> for io::Result<T> {
    fn ctx(self, op: &'static str, path: &Path) -> Result<T> {
        self.map_err(|source| Error::Io {
            op,
            path: path.to_path_buf(),
            source,
        })
    }
}

/// Uninstall could not remove everything. `remaining` is a record of only what
/// is still installed; persist it and call `uninstall` again to finish.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct UninstallError {
    pub remaining: InstallRecord,
    pub message: String,
}
