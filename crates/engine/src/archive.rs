use std::collections::HashSet;
use std::fs::{self, File};
use std::io;
use std::path::Path;

use crate::error::{Error, IoCtx, Result};
use crate::fsx::{join_rel, key, normalize_rel};

/// Extracts `zip_path` into `dest` (created if missing). Every entry name is
/// validated before anything is written: absolute paths, drive prefixes, `..`,
/// symlinks and case-insensitive duplicates reject the whole archive.
/// Returns the extracted files as relative `a/b/c` paths.
pub fn extract_zip(zip_path: &Path, dest: &Path) -> Result<Vec<String>> {
    let file = File::open(zip_path).ctx("open", zip_path)?;
    let mut zip = zip::ZipArchive::new(io::BufReader::new(file))?;

    // Pass 1: validate.
    let mut plan = Vec::with_capacity(zip.len());
    let mut seen = HashSet::new();
    for i in 0..zip.len() {
        let entry = zip.by_index_raw(i)?;
        let raw = entry.name().to_string();
        if entry.is_symlink() {
            return Err(Error::UnsafePath(raw));
        }
        let is_dir = entry.is_dir();
        let rel = match normalize_rel(&raw) {
            Ok(r) => r,
            // A "./" directory entry is harmless; anything else is not.
            Err(_) if is_dir && (raw == "./" || raw == ".\\") => continue,
            Err(e) => return Err(e),
        };
        if !is_dir && !seen.insert(key(&rel)) {
            return Err(Error::Invalid(format!("duplicate zip entry {raw:?}")));
        }
        plan.push((i, rel, is_dir));
    }

    // Pass 2: extract.
    fs::create_dir_all(dest).ctx("create dir", dest)?;
    let mut files = Vec::new();
    for (i, rel, is_dir) in plan {
        let out = join_rel(dest, &rel);
        if is_dir {
            fs::create_dir_all(&out).ctx("create dir", &out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).ctx("create dir", parent)?;
        }
        let mut entry = zip.by_index(i)?;
        let mut w = File::create(&out).ctx("create", &out)?;
        io::copy(&mut entry, &mut w).ctx("extract", &out)?;
        files.push(rel);
    }
    Ok(files)
}
