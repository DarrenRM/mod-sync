//! Small filesystem helpers: safe relative paths, hashing, atomic writes.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use crate::error::{Error, IoCtx, Result};

/// Normalizes a package-relative path to `a/b/c` form. Rejects absolute paths,
/// drive prefixes, `..`, and `:` anywhere (drives, alternate data streams).
pub fn normalize_rel(raw: &str) -> Result<String> {
    let unsafe_path = || Error::UnsafePath(raw.to_string());
    let s = raw.replace('\\', "/");
    if s.starts_with('/') || s.contains(':') || s.contains('\0') {
        return Err(unsafe_path());
    }
    let mut parts = Vec::new();
    for c in s.split('/') {
        match c {
            "" | "." => {}
            ".." => return Err(unsafe_path()),
            c => parts.push(c),
        }
    }
    if parts.is_empty() {
        return Err(unsafe_path());
    }
    Ok(parts.join("/"))
}

/// Joins a normalized relative path onto `base`.
pub(crate) fn join_rel(base: &Path, rel: &str) -> PathBuf {
    let mut p = base.to_path_buf();
    for c in rel.split('/') {
        p.push(c);
    }
    p
}

/// Like [`join_rel`] but validates first (for paths read back from a record).
pub(crate) fn safe_join(base: &Path, rel: &str) -> Option<PathBuf> {
    normalize_rel(rel).ok().map(|r| join_rel(base, &r))
}

/// Case-insensitive comparison key for a relative path.
pub(crate) fn key(rel: &str) -> String {
    rel.replace('\\', "/").to_lowercase()
}

/// Parent of a relative path (`a/b/c` -> `a/b`), `None` at the top.
pub(crate) fn rel_parent(rel: &str) -> Option<&str> {
    rel.rfind('/').map(|i| &rel[..i])
}

/// Lowercase hex SHA-256 of a file.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = File::open(path).ctx("open", path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).ctx("read", path)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// A temp file name next to `dest` (same directory, so rename is atomic).
pub(crate) fn temp_path(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    dest.with_file_name(format!(".{name}.{}-{seq}.modsync-tmp", std::process::id()))
}

/// Renames `tmp` over `dest` (replacing it); removes `tmp` on failure.
fn commit_temp(tmp: &Path, dest: &Path) -> Result<()> {
    fs::rename(tmp, dest).map_err(|e| {
        let _ = fs::remove_file(tmp);
        Error::Io {
            op: "replace",
            path: dest.to_path_buf(),
            source: e,
        }
    })
}

/// Writes `bytes` to `dest` via a temp file + rename.
pub(crate) fn write_atomic(dest: &Path, bytes: &[u8]) -> Result<()> {
    write_atomic_via(dest, &temp_path(dest), bytes)
}

/// [`write_atomic`] through a given temp file (see [`temp_path`]), so the
/// caller can journal it first.
pub(crate) fn write_atomic_via(dest: &Path, tmp: &Path, bytes: &[u8]) -> Result<()> {
    let res = (|| -> io::Result<()> {
        let mut f = File::create(tmp)?;
        f.write_all(bytes)?;
        f.sync_all()
    })();
    if let Err(e) = res {
        let _ = fs::remove_file(tmp);
        return Err(Error::Io {
            op: "write",
            path: tmp.to_path_buf(),
            source: e,
        });
    }
    commit_temp(tmp, dest)
}

/// Plain copy (keeps timestamps on Windows), flushed to disk before returning,
/// so an undo step or backup that points at the copy survives a power cut. A
/// read-only copy can't be opened to flush; the OS writes that one back itself.
pub(crate) fn copy_durable(src: &Path, dest: &Path) -> io::Result<()> {
    fs::copy(src, dest)?;
    match fs::OpenOptions::new().write(true).open(dest) {
        Ok(f) => f.sync_all(),
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => Ok(()),
        Err(e) => Err(e),
    }
}

/// Copies `src` over `dest` via the temp file `tmp` (see [`temp_path`]) + rename,
/// returning the SHA-256 of what was written.
pub(crate) fn install_file(src: &Path, dest: &Path, tmp: &Path) -> Result<String> {
    let tmp = tmp.to_path_buf();
    let res = (|| -> Result<String> {
        let mut input = File::open(src).ctx("open", src)?;
        let mut out = File::create(&tmp).ctx("create", &tmp)?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = input.read(&mut buf).ctx("read", src)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
            out.write_all(&buf[..n]).ctx("write", &tmp)?;
        }
        out.sync_all().ctx("write", &tmp)?;
        Ok(hex::encode(h.finalize()))
    })();
    match res {
        Ok(sha) => commit_temp(&tmp, dest).map(|_| sha),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Plain copy (keeps timestamps on Windows) into a temp file, then rename over `dest`.
pub(crate) fn copy_atomic(src: &Path, dest: &Path) -> Result<()> {
    let tmp = temp_path(dest);
    if let Err(e) = copy_durable(src, &tmp) {
        let _ = fs::remove_file(&tmp);
        return Err(Error::Io {
            op: "copy",
            path: src.to_path_buf(),
            source: e,
        });
    }
    commit_temp(&tmp, dest)
}

/// Lists all files under `root` as normalized relative paths, sorted.
pub(crate) fn list_files(root: &Path) -> Result<Vec<String>> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<()> {
        for entry in fs::read_dir(dir).ctx("list", dir)? {
            let entry = entry.ctx("list", dir)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let ft = entry.file_type().ctx("stat", &entry.path())?;
            if ft.is_dir() {
                walk(&entry.path(), &rel, out)?;
            } else if ft.is_file() {
                out.push(rel);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, "", &mut out)?;
    out.sort_by_key(|a| key(a));
    Ok(out)
}

/// Exists in any form (file, dir, broken link).
pub(crate) fn exists(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok()
}
