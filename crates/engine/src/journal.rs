//! Write-ahead undo log. Each step is logged *before* it touches the disk, so a
//! failed call (or a crashed process, via [`recover`]) can be rolled back.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, IoCtx, Result};
use crate::fsx;

const LOG: &str = "undo.jsonl";
const SAVED: &str = "saved";

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub(crate) enum Undo {
    /// We are creating this file: undo deletes it.
    RemoveFile { path: PathBuf },
    /// We are creating this directory: undo removes it.
    RemoveDir { path: PathBuf },
    /// `saved` holds what `path` contained before: undo copies it back.
    Restore { path: PathBuf, saved: PathBuf },
    /// We are removing this empty directory: undo recreates it.
    MakeDir { path: PathBuf },
}

pub(crate) struct Journal {
    dir: PathBuf,
    log: File,
    ops: Vec<Undo>,
}

impl Journal {
    /// Starts a new log in `work_dir`, first rolling back any interrupted one.
    pub fn begin(work_dir: &Path) -> Result<Self> {
        recover(work_dir)?;
        let saved = work_dir.join(SAVED);
        fs::create_dir_all(&saved).ctx("create dir", &saved)?;
        let path = work_dir.join(LOG);
        let log = File::create(&path).ctx("create", &path)?;
        Ok(Journal {
            dir: work_dir.to_path_buf(),
            log,
            ops: Vec::new(),
        })
    }

    pub fn push(&mut self, op: Undo) -> Result<()> {
        let mut line = serde_json::to_vec(&op).expect("serialize undo op");
        line.push(b'\n');
        let path = self.dir.join(LOG);
        self.log.write_all(&line).ctx("write", &path)?;
        self.ops.push(op);
        Ok(())
    }

    /// Copies `path` aside so undo can put it back. Call before modifying it.
    pub fn save(&mut self, path: &Path) -> Result<()> {
        let saved = self.dir.join(SAVED).join(self.ops.len().to_string());
        fs::copy(path, &saved).ctx("save copy of", path)?;
        self.push(Undo::Restore {
            path: path.to_path_buf(),
            saved,
        })
    }

    /// Undoes everything logged so far. On success the log is deleted; on
    /// failure it is kept for [`recover`].
    pub fn rollback(self) -> std::result::Result<(), String> {
        let Journal { dir, log, ops } = self;
        drop(log);
        undo_all(&ops)?;
        cleanup(&dir);
        Ok(())
    }

    /// Success: forget the undo data.
    pub fn commit(self) {
        let Journal { dir, log, .. } = self;
        drop(log);
        cleanup(&dir);
    }
}

fn cleanup(dir: &Path) {
    let _ = fs::remove_file(dir.join(LOG));
    let _ = fs::remove_dir_all(dir.join(SAVED));
}

fn undo_one(op: &Undo) -> io::Result<()> {
    let ignore_missing = |r: io::Result<()>| match r {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        r => r,
    };
    match op {
        Undo::RemoveFile { path } => ignore_missing(fs::remove_file(path)),
        Undo::RemoveDir { path } => ignore_missing(fs::remove_dir(path)),
        Undo::MakeDir { path } => fs::create_dir_all(path),
        Undo::Restore { path, saved } => {
            // Skip if never changed: the step may have failed on a locked file
            // that we could not replace now either.
            if same_contents(path, saved) {
                return Ok(());
            }
            fsx::copy_atomic(saved, path).map_err(io::Error::other)
        }
    }
}

fn same_contents(a: &Path, b: &Path) -> bool {
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(ma), Ok(mb)) if ma.is_file() && mb.is_file() && ma.len() == mb.len() => {
            matches!((fs::read(a), fs::read(b)), (Ok(x), Ok(y)) if x == y)
        }
        _ => false,
    }
}

/// Undoes in reverse order, continuing past failures; returns all failures.
fn undo_all(ops: &[Undo]) -> std::result::Result<(), String> {
    let mut errors = Vec::new();
    for op in ops.iter().rev() {
        if let Err(e) = undo_one(op) {
            errors.push(format!("{op:?}: {e}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

/// Rolls back an install/update that was interrupted (process killed, power
/// loss) using the undo log left in `work_dir`. Returns `Ok(true)` if there was
/// something to roll back. `apply` calls this automatically before starting.
pub fn recover(work_dir: &Path) -> Result<bool> {
    let path = work_dir.join(LOG);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => {
            return Err(Error::Io {
                op: "read",
                path,
                source: e,
            })
        }
    };
    // A torn last line (crash mid-write) is skipped: its step never started.
    let ops: Vec<Undo> = text
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    undo_all(&ops)
        .map_err(|e| Error::Invalid(format!("rolling back an interrupted install failed: {e}")))?;
    cleanup(work_dir);
    Ok(true)
}
