#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use modsync_engine::{apply, Edit, InstallRecord, PackageSpec, Result};
use tempfile::TempDir;

/// A directory tree as relative path -> file bytes (`None` for directories).
pub type Snapshot = BTreeMap<String, Option<Vec<u8>>>;

pub fn snapshot(root: &Path) -> Snapshot {
    fn walk(dir: &Path, prefix: &str, out: &mut Snapshot) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd {
            let e = e.unwrap();
            let name = e.file_name().to_string_lossy().into_owned();
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if e.file_type().unwrap().is_dir() {
                out.insert(rel.clone(), None);
                walk(&e.path(), &rel, out);
            } else {
                out.insert(rel, Some(fs::read(e.path()).unwrap()));
            }
        }
    }
    let mut out = Snapshot::new();
    walk(root, "", &mut out);
    out
}

/// Asserts two snapshots are equal, with a readable diff of paths.
pub fn assert_same(before: &Snapshot, after: &Snapshot) {
    let mut diffs = Vec::new();
    for (k, v) in before {
        match after.get(k) {
            None => diffs.push(format!("missing: {k}")),
            Some(v2) if v2 != v => diffs.push(format!("changed: {k}")),
            _ => {}
        }
    }
    for k in after.keys().filter(|k| !before.contains_key(*k)) {
        diffs.push(format!("extra: {k}"));
    }
    assert!(diffs.is_empty(), "trees differ:\n  {}", diffs.join("\n  "));
}

pub fn write(base: &Path, rel: &str, bytes: impl AsRef<[u8]>) {
    let p = base.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, bytes).unwrap();
}

pub fn read(base: &Path, rel: &str) -> Vec<u8> {
    fs::read(base.join(rel)).unwrap()
}

pub struct Env {
    pub tmp: TempDir,
    pub game: PathBuf,
    pub backups: PathBuf,
    pub work: PathBuf,
    /// Where `apply` writes the install record.
    pub record: PathBuf,
    staged_count: std::cell::Cell<u32>,
}

impl Env {
    pub fn new() -> Env {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp
            .path()
            .join("game")
            .join("Vein")
            .join("Binaries")
            .join("Win64");
        fs::create_dir_all(&game).unwrap();
        let backups = tmp.path().join("appdata").join("backups").join("vein");
        let work = tmp.path().join("appdata").join("work").join("vein");
        let record = tmp.path().join("appdata").join("installs").join("vein.json");
        Env {
            game,
            backups,
            work,
            record,
            tmp,
            staged_count: Default::default(),
        }
    }

    /// Writes a staged (extracted) package and returns its folder.
    pub fn stage(&self, files: &[(&str, &[u8])]) -> PathBuf {
        let n = self.staged_count.get() + 1;
        self.staged_count.set(n);
        let dir = self.tmp.path().join(format!("staged{n}"));
        fs::create_dir_all(&dir).unwrap();
        for (rel, bytes) in files {
            write(&dir, rel, bytes);
        }
        dir
    }

    pub fn try_apply(
        &self,
        staged: &Path,
        spec: &PackageSpec,
        prev: Option<&InstallRecord>,
    ) -> Result<InstallRecord> {
        apply(
            &self.game,
            staged,
            spec,
            "vein",
            prev,
            &self.backups,
            &self.work,
            &self.record,
            &mut |_, _| {},
        )
    }

    pub fn apply(
        &self,
        staged: &Path,
        spec: &PackageSpec,
        prev: Option<&InstallRecord>,
    ) -> InstallRecord {
        let rec = self.try_apply(staged, spec, prev).expect("apply");
        // Undo data is gone after success.
        assert!(!self.work.join("undo.jsonl").exists());
        // The record on disk is the one returned.
        assert_eq!(self.saved_record().as_ref(), Some(&rec));
        rec
    }

    /// The install record `apply` last wrote, if any.
    pub fn saved_record(&self) -> Option<InstallRecord> {
        let bytes = fs::read(&self.record).ok()?;
        Some(serde_json::from_slice(&bytes).unwrap())
    }

    pub fn game_snap(&self) -> Snapshot {
        snapshot(&self.game)
    }
    pub fn backup_snap(&self) -> Snapshot {
        snapshot(&self.backups)
    }
}

pub fn spec(version: &str, preserve: &[&str], owned: &[&str], edits: Vec<Edit>) -> PackageSpec {
    PackageSpec {
        version: version.into(),
        preserve: preserve.iter().map(|s| s.to_string()).collect(),
        owned_dirs: owned.iter().map(|s| s.to_string()).collect(),
        edits,
    }
}

pub fn line_edit(file: &str, key: &str, line: &str, before: Option<&str>) -> Edit {
    Edit::Line {
        file: file.into(),
        key: key.into(),
        line: line.into(),
        before: before.map(Into::into),
    }
}

pub const MODS_TXT: &str = "\u{feff}CheatManagerEnablerMod : 1\r\nConsoleCommandsMod : 1\r\nBPML_GenericFunctions : 1\r\nLineTraceMod : 0\r\n\r\n; Built-in keybinds, do not move up!\r\nKeybinds : 0";

/// Holds a file open for reading without FILE_SHARE_WRITE/DELETE, so it can't be
/// replaced or deleted (like a DLL loaded by a running game).
#[cfg(windows)]
pub fn lock(path: &Path) -> fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 1;
    fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
        .unwrap()
}
