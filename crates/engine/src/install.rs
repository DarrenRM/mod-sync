use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use crate::error::{Error, IoCtx, Result, UninstallError};
use crate::fsx::{self, join_rel, key, normalize_rel, rel_parent, safe_join};
use crate::journal::{Journal, Undo};
use crate::lines::{self, LineChange};
use crate::types::{Edit, EditEntry, FileEntry, InstallRecord, PackageSpec};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyReport {
    pub missing: Vec<String>,
    pub modified: Vec<String>,
}

impl VerifyReport {
    pub fn is_ok(&self) -> bool {
        self.missing.is_empty() && self.modified.is_empty()
    }
}

/// Checks every recorded file still exists with the recorded hash. Preserved
/// files are only checked for presence. Unreadable (locked) files are skipped.
pub fn verify(record: &InstallRecord) -> VerifyReport {
    let mut report = VerifyReport::default();
    for f in &record.files {
        let Some(p) = safe_join(&record.target, &f.path) else {
            continue;
        };
        if !p.is_file() {
            report.missing.push(f.path.clone());
        } else if !f.preserved {
            if let Ok(sha) = fsx::sha256_file(&p) {
                if !sha.eq_ignore_ascii_case(&f.sha256) {
                    report.modified.push(f.path.clone());
                }
            }
        }
    }
    report
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    norm(a) == norm(b)
}

fn build_preserve(patterns: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        let glob = GlobBuilder::new(p)
            .case_insensitive(true)
            .literal_separator(true)
            .build()
            .map_err(|e| Error::Pattern {
                pattern: p.clone(),
                message: e.to_string(),
            })?;
        b.add(glob);
    }
    b.build().map_err(|e| Error::Pattern {
        pattern: patterns.join(", "),
        message: e.to_string(),
    })
}

/// One install/update transaction: every disk change goes through the journal.
struct Tx<'a> {
    target: &'a Path,
    backup_root: &'a Path,
    j: Journal,
    created_dirs: Vec<String>,
    created_keys: HashSet<String>,
    known_dirs: HashSet<PathBuf>,
}

impl Tx<'_> {
    /// Creates missing parent dirs of `target/rel`, recording the ones we create.
    fn ensure_parents(&mut self, rel: &str) -> Result<()> {
        let Some(parent) = rel_parent(rel) else {
            return Ok(());
        };
        let mut acc = String::new();
        for comp in parent.split('/') {
            if !acc.is_empty() {
                acc.push('/');
            }
            acc.push_str(comp);
            let abs = join_rel(self.target, &acc);
            if self.known_dirs.contains(&abs) {
                continue;
            }
            match fs::metadata(&abs) {
                Ok(m) if m.is_dir() => {}
                Ok(_) => {
                    return Err(Error::Invalid(format!(
                        "{} is a file, expected a folder",
                        abs.display()
                    )))
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    self.j.push(Undo::RemoveDir { path: abs.clone() })?;
                    fs::create_dir(&abs).ctx("create dir", &abs)?;
                    if self.created_keys.insert(key(&acc)) {
                        self.created_dirs.push(acc.clone());
                    }
                }
                Err(e) => {
                    return Err(Error::Io {
                        op: "stat",
                        path: abs,
                        source: e,
                    })
                }
            }
            self.known_dirs.insert(abs);
        }
        Ok(())
    }

    /// Creates missing ancestors of `dir` (outside the game folder, untracked
    /// by the record but undone on rollback).
    fn make_dirs(&mut self, dir: &Path) -> Result<()> {
        let mut missing = Vec::new();
        let mut p = Some(dir);
        while let Some(d) = p {
            if fsx::exists(d) {
                break;
            }
            missing.push(d.to_path_buf());
            p = d.parent();
        }
        for d in missing.into_iter().rev() {
            self.j.push(Undo::RemoveDir { path: d.clone() })?;
            fs::create_dir(&d).ctx("create dir", &d)?;
        }
        Ok(())
    }

    /// Journals `path` (saved copy if it exists, else "remove on undo").
    fn guard(&mut self, path: &Path) -> Result<()> {
        if fsx::exists(path) {
            self.j.save(path)
        } else {
            self.j.push(Undo::RemoveFile {
                path: path.to_path_buf(),
            })
        }
    }

    fn write(&mut self, src: &Path, dest: &Path) -> Result<String> {
        let tmp = fsx::temp_path(dest);
        self.j.push(Undo::RemoveFile { path: tmp.clone() })?;
        fsx::install_file(src, dest, &tmp)
    }

    /// Picks a free backup path for `rel` (a stale backup is never overwritten).
    fn backup_rel(&self, rel: &str) -> String {
        let mut candidate = rel.to_string();
        let mut n = 1;
        while fsx::exists(&join_rel(self.backup_root, &candidate)) {
            candidate = format!("{rel}.~{n}");
            n += 1;
        }
        candidate
    }

    /// Reverts a file the previous install owned. Returns a backup to delete
    /// once the transaction commits.
    fn revert_owned(&mut self, f: &FileEntry) -> Result<Option<PathBuf>> {
        let Some(dest) = safe_join(self.target, &f.path) else {
            return Ok(None);
        };
        let backup = f
            .backup
            .as_deref()
            .and_then(|b| safe_join(self.backup_root, b));
        match backup {
            Some(b) if b.is_file() => {
                self.ensure_parents(&normalize_rel(&f.path)?)?;
                self.guard(&dest)?;
                fsx::copy_atomic(&b, &dest)?;
                Ok(Some(b))
            }
            _ => {
                if removable(&dest, f) {
                    self.j.save(&dest)?;
                    fs::remove_file(&dest).ctx("delete", &dest)?;
                }
                Ok(None)
            }
        }
    }
}

/// Whether a recorded file without a usable backup should be deleted: it must
/// exist, and if a backup was expected but is lost, only when it is still ours.
fn removable(dest: &Path, f: &FileEntry) -> bool {
    if !dest.is_file() {
        return false;
    }
    f.backup.is_none() || fsx::sha256_file(dest).is_ok_and(|s| s.eq_ignore_ascii_case(&f.sha256))
}

struct NewEdit<'a> {
    file: String,
    key: &'a str,
    line: &'a str,
    before: Option<&'a str>,
}

fn edit_key(file: &str, k: &str) -> (String, String) {
    (key(file), k.trim().to_lowercase())
}

/// Combines a previous edit entry with what ensure_line just did, so revert
/// still goes back to the state before the *first* install.
fn merge_change(prev: Option<&EditEntry>, now: LineChange) -> LineChange {
    let Some(prev) = prev else { return now };
    if now.is_noop() || (now.replaced.is_some() && !prev.is_noop()) {
        prev.change()
    } else {
        now
    }
}

/// Installs (`prev` = None) or updates the extracted package in `staged` into
/// `target`. Runs under an undo log in `work_dir`; on any error everything this
/// call changed is rolled back and the error returned. `progress(done, total)`
/// is called with staged bytes copied.
#[allow(clippy::too_many_arguments)]
pub fn apply(
    target: &Path,
    staged: &Path,
    spec: &PackageSpec,
    game_id: &str,
    prev: Option<&InstallRecord>,
    backup_root: &Path,
    work_dir: &Path,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<InstallRecord> {
    if !target.is_dir() {
        return Err(Error::Invalid(format!(
            "game folder {} does not exist",
            target.display()
        )));
    }
    if let Some(p) = prev {
        if !same_dir(&p.target, target) {
            return Err(Error::Invalid(format!(
                "previous install is in {}, not {}; uninstall it first",
                p.target.display(),
                target.display()
            )));
        }
    }
    let preserve = build_preserve(&spec.preserve)?;
    let mut staged_files = Vec::new();
    for rel in fsx::list_files(staged)? {
        let src = join_rel(staged, &rel);
        let size = fs::metadata(&src).ctx("stat", &src)?.len();
        staged_files.push((normalize_rel(&rel)?, src, size));
    }
    let mut edits = Vec::new();
    let mut seen_edits = HashSet::new();
    for Edit::Line {
        file,
        key: k,
        line,
        before,
    } in &spec.edits
    {
        let file = normalize_rel(file)?;
        if seen_edits.insert(edit_key(&file, k)) {
            edits.push(NewEdit {
                file,
                key: k,
                line,
                before: before.as_deref(),
            });
        }
    }

    let prev_dirs = prev.map(|p| p.created_dirs.clone()).unwrap_or_default();
    let mut tx = Tx {
        target,
        backup_root,
        j: Journal::begin(work_dir)?,
        created_keys: prev_dirs.iter().map(|d| key(d)).collect(),
        created_dirs: prev_dirs,
        known_dirs: HashSet::new(),
    };
    let result = run(&mut tx, &staged_files, &edits, &preserve, prev, progress);
    let Tx {
        j, created_dirs, ..
    } = tx;
    match result {
        Ok((files, edit_entries, used_backups)) => {
            j.commit();
            for b in used_backups {
                delete_backup(backup_root, &b);
            }
            Ok(InstallRecord {
                game_id: game_id.to_string(),
                version: spec.version.clone(),
                target: target.to_path_buf(),
                installed_at: now_rfc3339(),
                files,
                created_dirs,
                edits: edit_entries,
                owned_dirs: spec
                    .owned_dirs
                    .iter()
                    .filter_map(|d| normalize_rel(d).ok())
                    .collect(),
            })
        }
        Err(e) => match j.rollback() {
            Ok(()) => Err(e),
            Err(rollback) => Err(Error::RollbackFailed {
                cause: Box::new(e),
                rollback,
            }),
        },
    }
}

type RunOutput = (Vec<FileEntry>, Vec<EditEntry>, Vec<PathBuf>);

fn run(
    tx: &mut Tx,
    staged_files: &[(String, PathBuf, u64)],
    edits: &[NewEdit],
    preserve: &GlobSet,
    prev: Option<&InstallRecord>,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<RunOutput> {
    let prev_files: HashMap<String, &FileEntry> = prev
        .map(|p| p.files.iter().map(|f| (key(&f.path), f)).collect())
        .unwrap_or_default();
    let total: u64 = staged_files.iter().map(|f| f.2).sum();
    let mut done = 0;
    progress(0, total);

    // 1. Package files.
    let mut files = Vec::new();
    let mut new_keys = HashSet::new();
    for (rel, src, size) in staged_files {
        let k = key(rel);
        new_keys.insert(k.clone());
        let dest = join_rel(tx.target, rel);
        let owned = prev_files.get(&k).copied();
        let preserved = preserve.is_match(rel.as_str());
        let dest_exists = fsx::exists(&dest);

        if preserved && dest_exists {
            // Never overwrite; stays ours only if we put it there.
            if let Some(o) = owned {
                let sha = fsx::sha256_file(&dest).unwrap_or_else(|_| o.sha256.clone());
                files.push(FileEntry {
                    path: o.path.clone(),
                    sha256: sha,
                    backup: o.backup.clone(),
                    preserved,
                });
            }
        } else if let Some(o) = owned {
            let unchanged = dest.is_file()
                && fsx::sha256_file(&dest).is_ok_and(|s| s.eq_ignore_ascii_case(&o.sha256))
                && fsx::sha256_file(src)?.eq_ignore_ascii_case(&o.sha256);
            let sha = if unchanged {
                o.sha256.clone()
            } else {
                tx.ensure_parents(rel)?;
                tx.guard(&dest)?;
                tx.write(src, &dest)?
            };
            files.push(FileEntry {
                path: rel.clone(),
                sha256: sha,
                backup: o.backup.clone(),
                preserved,
            });
        } else if dest_exists {
            let backup_rel = tx.backup_rel(rel);
            let backup = join_rel(tx.backup_root, &backup_rel);
            if let Some(parent) = backup.parent() {
                tx.make_dirs(parent)?;
            }
            tx.j.push(Undo::RemoveFile {
                path: backup.clone(),
            })?;
            fs::copy(&dest, &backup).ctx("back up", &dest)?;
            tx.j.push(Undo::Restore {
                path: dest.clone(),
                saved: backup,
            })?;
            let sha = tx.write(src, &dest)?;
            files.push(FileEntry {
                path: rel.clone(),
                sha256: sha,
                backup: Some(backup_rel),
                preserved,
            });
        } else {
            tx.ensure_parents(rel)?;
            tx.j.push(Undo::RemoveFile { path: dest.clone() })?;
            let sha = tx.write(src, &dest)?;
            files.push(FileEntry {
                path: rel.clone(),
                sha256: sha,
                backup: None,
                preserved,
            });
        }
        done += size;
        progress(done, total);
    }

    // 2. Files the previous version had but this one doesn't.
    let mut used_backups = Vec::new();
    if let Some(p) = prev {
        for f in p.files.iter().filter(|f| !new_keys.contains(&key(&f.path))) {
            used_backups.extend(tx.revert_owned(f)?);
        }
    }

    // 3. Edits: revert dropped ones, then apply the new set.
    let new_edit_keys: HashSet<_> = edits.iter().map(|e| edit_key(&e.file, e.key)).collect();
    let prev_edits: HashMap<_, &EditEntry> = prev
        .map(|p| {
            p.edits
                .iter()
                .map(|e| (edit_key(&e.file, &e.key), e))
                .collect()
        })
        .unwrap_or_default();
    if let Some(p) = prev {
        for e in p.edits.iter().rev() {
            if e.is_noop() || new_edit_keys.contains(&edit_key(&e.file, &e.key)) {
                continue;
            }
            let Some(abs) = safe_join(tx.target, &e.file) else {
                continue;
            };
            if abs.is_file() {
                tx.j.save(&abs)?;
                lines::revert_line(&abs, &e.key, &e.change())?;
            }
        }
    }
    let mut edit_entries = Vec::new();
    let mut edited = HashSet::new();
    for e in edits {
        let abs = join_rel(tx.target, &e.file);
        tx.ensure_parents(&e.file)?;
        tx.guard(&abs)?;
        let now = lines::ensure_line(&abs, e.key, e.line, e.before)?;
        let ch = merge_change(prev_edits.get(&edit_key(&e.file, e.key)).copied(), now);
        edited.insert(key(&e.file));
        edit_entries.push(EditEntry {
            file: e.file.clone(),
            key: e.key.to_string(),
            line: e.line.to_string(),
            before: e.before.map(str::to_string),
            added: ch.added,
            replaced: ch.replaced,
            created_file: ch.created_file,
            trailing_newline: ch.trailing_newline,
        });
    }
    // Owned files we just edited: record the hash after the edit.
    for f in files.iter_mut().filter(|f| edited.contains(&key(&f.path))) {
        f.sha256 = fsx::sha256_file(&join_rel(tx.target, &f.path))?;
    }

    // 4. Drop created dirs that are now gone or empty (left over from dropped files).
    let mut dirs = tx.created_dirs.clone();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.matches('/').count()));
    let mut gone = HashSet::new();
    for d in dirs {
        let Some(abs) = safe_join(tx.target, &d) else {
            gone.insert(key(&d));
            continue;
        };
        if !fsx::exists(&abs) {
            gone.insert(key(&d));
        } else if fs::read_dir(&abs).is_ok_and(|mut r| r.next().is_none()) {
            tx.j.push(Undo::MakeDir { path: abs.clone() })?;
            fs::remove_dir(&abs).ctx("remove dir", &abs)?;
            gone.insert(key(&d));
        }
    }
    tx.created_dirs.retain(|d| !gone.contains(&key(d)));

    files.sort_by_key(|f| key(&f.path));
    Ok((files, edit_entries, used_backups))
}

/// Deletes a used backup and prunes now-empty folders up to and including `backup_root`.
fn delete_backup(backup_root: &Path, backup: &Path) {
    if fs::remove_file(backup).is_err() {
        return;
    }
    let mut dir = backup.parent();
    while let Some(d) = dir {
        if !d.starts_with(backup_root) || fs::remove_dir(d).is_err() {
            break;
        }
        if d == backup_root {
            break;
        }
        dir = d.parent();
    }
}

/// Reverts one recorded file for uninstall. Returns the backup to delete.
fn uninstall_file(target: &Path, backup_root: &Path, f: &FileEntry) -> Result<Option<PathBuf>> {
    let Some(dest) = safe_join(target, &f.path) else {
        return Ok(None);
    };
    let backup = f.backup.as_deref().and_then(|b| safe_join(backup_root, b));
    match backup {
        Some(b) if b.is_file() => {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).ctx("create dir", parent)?;
            }
            fsx::copy_atomic(&b, &dest)?;
            Ok(Some(b))
        }
        _ => {
            if removable(&dest, f) {
                match fs::remove_file(&dest) {
                    Err(e) if e.kind() != io::ErrorKind::NotFound => {
                        return Err(Error::Io {
                            op: "delete",
                            path: dest,
                            source: e,
                        })
                    }
                    _ => {}
                }
            }
            Ok(None)
        }
    }
}

fn is_under(path_key: &str, dir_key: &str) -> bool {
    path_key.len() > dir_key.len()
        && path_key.starts_with(dir_key)
        && path_key.as_bytes()[dir_key.len()] == b'/'
}

/// Removes everything `record` installed: reverts edits, restores backups or
/// deletes files, sweeps owned dirs we created, then removes our empty created
/// dirs. Keeps going past failures; on any failure returns the part of the
/// record that is still installed so a retry can finish.
#[allow(clippy::result_large_err)] // shape fixed by the app API
pub fn uninstall(record: &InstallRecord, backup_root: &Path) -> Result<(), UninstallError> {
    let target = &record.target;
    let mut errors = Vec::new();

    // 1. Edits, newest first.
    let mut rem_edits = Vec::new();
    for e in record.edits.iter().rev() {
        if e.is_noop() {
            continue;
        }
        let Some(abs) = safe_join(target, &e.file) else {
            continue;
        };
        if let Err(err) = lines::revert_line(&abs, &e.key, &e.change()) {
            errors.push(err.to_string());
            rem_edits.insert(0, e.clone());
        }
    }

    // 2. Files.
    let mut rem_files = Vec::new();
    let mut used_backups = Vec::new();
    for f in &record.files {
        match uninstall_file(target, backup_root, f) {
            Ok(b) => used_backups.extend(b),
            Err(err) => {
                errors.push(err.to_string());
                rem_files.push(f.clone());
            }
        }
    }

    // 3. Owned dirs we created: remove whole (sweeps runtime logs, dumps...).
    let created: HashSet<String> = record.created_dirs.iter().map(|d| key(d)).collect();
    let mut failed_owned = Vec::new();
    for od in &record.owned_dirs {
        let Ok(rel) = normalize_rel(od) else { continue };
        if !created.contains(&key(&rel)) {
            continue;
        }
        let abs = join_rel(target, &rel);
        match fs::remove_dir_all(&abs) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                errors.push(format!("remove {}: {e}", abs.display()));
                failed_owned.push(key(&rel));
            }
            _ => {}
        }
    }

    // 4. Remaining created dirs, deepest first, only if empty.
    let mut still_needed: Vec<String> = rem_files.iter().map(|f| key(&f.path)).collect();
    still_needed.extend(rem_edits.iter().map(|e| key(&e.file)));
    let mut dirs: Vec<&String> = record.created_dirs.iter().collect();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.matches('/').count()));
    let mut rem_dir_keys = HashSet::new();
    for d in dirs {
        let Ok(rel) = normalize_rel(d) else { continue };
        let k = key(&rel);
        let needed = still_needed.iter().any(|p| is_under(p, &k))
            || failed_owned.iter().any(|o| *o == k || is_under(o, &k));
        match fs::remove_dir(join_rel(target, &rel)) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            // Not empty: user content we leave alone, unless our leftovers are inside.
            Err(e) if e.kind() == io::ErrorKind::DirectoryNotEmpty => {
                if needed {
                    rem_dir_keys.insert(k);
                }
            }
            Err(e) => {
                errors.push(format!("remove {}: {e}", join_rel(target, &rel).display()));
                rem_dir_keys.insert(k);
            }
        }
    }

    for b in used_backups {
        delete_backup(backup_root, &b);
    }

    if errors.is_empty() {
        return Ok(());
    }
    let remaining = InstallRecord {
        files: rem_files,
        edits: rem_edits,
        created_dirs: record
            .created_dirs
            .iter()
            .filter(|d| rem_dir_keys.contains(&key(d)))
            .cloned()
            .collect(),
        ..record.clone()
    };
    let message = format!(
        "could not remove everything (is the game running?): {}",
        errors.join("; ")
    );
    Err(UninstallError { remaining, message })
}
