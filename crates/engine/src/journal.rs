//! Write-ahead undo log. Each step is logged, and flushed to disk, *before* it
//! touches the game folder, so a failed call (or a killed process or a power
//! cut, via [`recover`]) can be rolled back. A transaction commits by logging
//! a final marker, so a log that outlives its commit is never rolled back.

use std::collections::{HashMap, HashSet};
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
    /// The transaction committed: nothing in this log is undone.
    Committed,
}

pub(crate) struct Journal {
    dir: PathBuf,
    log: File,
    ops: Vec<Undo>,
}

impl Journal {
    /// Starts a new log in `work_dir`. Run [`recover`] first: this replaces any
    /// log already there.
    pub fn begin(work_dir: &Path) -> Result<Self> {
        let saved = work_dir.join(SAVED);
        // Stale copies stay behind when a finished log couldn't be deleted
        // (see `cleanup`); that log is replaced below.
        let _ = fs::remove_dir_all(&saved);
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
        // On disk before the step it undoes runs, so a power cut can't lose it.
        self.log.sync_data().ctx("write", &path)?;
        self.ops.push(op);
        Ok(())
    }

    /// Copies `path` aside so undo can put it back. Call before modifying it.
    pub fn save(&mut self, path: &Path) -> Result<()> {
        let saved = self.dir.join(SAVED).join(self.ops.len().to_string());
        fsx::copy_durable(path, &saved).ctx("save copy of", path)?;
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

    /// The commit point: once the marker is on disk, [`recover`] keeps the
    /// changes. On error the caller rolls back, so a marker that may have
    /// been written is cut off again.
    pub fn commit(&mut self) -> Result<()> {
        let len = self.log.metadata().map(|m| m.len());
        let res = self.push(Undo::Committed);
        if res.is_err() {
            if let Ok(len) = len {
                let _ = self.log.set_len(len);
                let _ = self.log.sync_data();
            }
        }
        res
    }

    /// After [`Journal::commit`]: forget the undo data.
    pub fn finish(self) {
        let Journal { dir, log, .. } = self;
        drop(log);
        cleanup(&dir);
    }
}

/// Deletes the log, then the saved copies. If the log can't be deleted (another
/// program holds it open), the copies stay: a log that is replayed later still
/// needs them.
fn cleanup(dir: &Path) {
    match fs::remove_file(dir.join(LOG)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => {}
        _ => {
            let _ = fs::remove_dir_all(dir.join(SAVED));
        }
    }
}

fn undo_one(op: &Undo) -> io::Result<()> {
    let ignore_missing = |r: io::Result<()>| match r {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        r => r,
    };
    match op {
        Undo::RemoveFile { path } => ignore_missing(fs::remove_file(path)),
        Undo::RemoveDir { path } => match fs::remove_dir(path) {
            // Something else has written into a folder we created (a game that
            // ran before recovery, say): leave the folder and its contents.
            Err(e) if e.kind() == io::ErrorKind::DirectoryNotEmpty => Ok(()),
            r => ignore_missing(r),
        },
        Undo::MakeDir { path } => fs::create_dir_all(path),
        Undo::Restore { path, saved } => {
            // A copy is only deleted after it was put back (see `undo_all`), so
            // a missing one means an earlier, interrupted pass got this far.
            if matches!(fs::symlink_metadata(saved), Err(e) if e.kind() == io::ErrorKind::NotFound) {
                return Ok(());
            }
            // Skip if never changed: the step may have failed on a locked file
            // that we could not replace now either.
            if same_contents(path, saved) {
                return Ok(());
            }
            fsx::copy_atomic(saved, path).map_err(io::Error::other)
        }
        Undo::Committed => Ok(()),
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
/// Replaying it again is safe, so a failed or interrupted pass can be retried.
fn undo_all(ops: &[Undo]) -> std::result::Result<(), String> {
    let mut errors = Vec::new();
    // A backup is logged for deletion before the restore that uses it, so
    // replay reaches the deletion next. Keep the copy if it wasn't put back:
    // it may be the only one of the player's original file.
    let mut not_restored: HashSet<&Path> = HashSet::new();
    // A file this transaction created and then changed (a package file it
    // also edits) is removed anyway, so its restore is skipped. Once a pass
    // has removed the file's new folder, that restore could never succeed.
    let mut first_removal: HashMap<&Path, usize> = HashMap::new();
    for (i, op) in ops.iter().enumerate() {
        if let Undo::RemoveFile { path } = op {
            first_removal.entry(path.as_path()).or_insert(i);
        }
    }
    for (i, op) in ops.iter().enumerate().rev() {
        match op {
            Undo::RemoveFile { path } if not_restored.contains(path.as_path()) => continue,
            Undo::Restore { path, .. }
                if first_removal.get(path.as_path()).is_some_and(|&j| j < i) =>
            {
                continue
            }
            _ => {}
        }
        if let Err(e) = undo_one(op) {
            if let Undo::Restore { saved, .. } = op {
                not_restored.insert(saved);
            }
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
/// something to roll back; a log that ends in a commit marker is only cleaned
/// up. On failure the log is kept, so calling it again retries. Run it before
/// reading the install record: rolling back can put the previous record back.
/// `apply` checks for an interrupted log too.
pub fn recover(work_dir: &Path) -> Result<bool> {
    let path = work_dir.join(LOG);
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => {
            return Err(Error::Io {
                op: "read",
                path,
                source: e,
            })
        }
    };
    // A torn last line (crash mid-write, perhaps mid-character) is skipped:
    // its step never started.
    let ops: Vec<Undo> = bytes
        .split(|&b| b == b'\n')
        .filter_map(|l| serde_json::from_slice(l).ok())
        .collect();
    if matches!(ops.last(), Some(Undo::Committed)) {
        cleanup(work_dir);
        return Ok(false);
    }
    undo_all(&ops)
        .map_err(|e| Error::Invalid(format!("rolling back an interrupted install failed: {e}")))?;
    cleanup(work_dir);
    Ok(true)
}
