//! Key/value line edits on text files like UE4SS `mods.txt` (`ModName : 1`,
//! comments start with `;`). Edits preserve BOM, line endings and final-newline
//! presence, and revert byte-exactly.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, IoCtx, Result};
use crate::fsx;

const BOM: &str = "\u{feff}";
/// Line ending for files we create or files without any line ending yet.
const DEFAULT_EOL: &str = "\r\n";

/// What [`ensure_line`] did, i.e. what [`revert_line`] has to undo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineChange {
    pub added: bool,
    pub replaced: Option<String>,
    pub created_file: bool,
    pub trailing_newline: bool,
}

impl LineChange {
    pub fn noop() -> Self {
        LineChange {
            added: false,
            replaced: None,
            created_file: false,
            trailing_newline: true,
        }
    }
    pub fn is_noop(&self) -> bool {
        !self.added && self.replaced.is_none()
    }
}

struct Line {
    text: String,
    eol: &'static str, // "\r\n", "\n" or "" (last line without newline)
}

struct Doc {
    bom: bool,
    lines: Vec<Line>,
}

impl Doc {
    fn parse(bytes: &[u8]) -> std::result::Result<Doc, std::str::Utf8Error> {
        let mut s = std::str::from_utf8(bytes)?;
        let bom = s.starts_with(BOM);
        if bom {
            s = &s[BOM.len()..];
        }
        let mut lines = Vec::new();
        let mut rest = s;
        while !rest.is_empty() {
            match rest.find('\n') {
                Some(i) => {
                    let (text, eol) = if i > 0 && rest.as_bytes()[i - 1] == b'\r' {
                        (&rest[..i - 1], "\r\n")
                    } else {
                        (&rest[..i], "\n")
                    };
                    lines.push(Line {
                        text: text.to_string(),
                        eol,
                    });
                    rest = &rest[i + 1..];
                }
                None => {
                    lines.push(Line {
                        text: rest.to_string(),
                        eol: "",
                    });
                    rest = "";
                }
            }
        }
        Ok(Doc { bom, lines })
    }

    fn render(&self) -> Vec<u8> {
        let mut s = String::new();
        if self.bom {
            s.push_str(BOM);
        }
        for l in &self.lines {
            s.push_str(&l.text);
            s.push_str(l.eol);
        }
        s.into_bytes()
    }

    /// Most common line ending; CRLF on ties or when there is none.
    fn eol(&self) -> &'static str {
        let crlf = self.lines.iter().filter(|l| l.eol == "\r\n").count();
        let lf = self.lines.iter().filter(|l| l.eol == "\n").count();
        if lf > crlf {
            "\n"
        } else if crlf > 0 {
            "\r\n"
        } else {
            DEFAULT_EOL
        }
    }

    fn find(&self, key: &str) -> Option<usize> {
        self.lines.iter().position(|l| key_matches(&l.text, key))
    }

    fn ends_with_newline(&self) -> bool {
        self.lines.last().is_none_or(|l| !l.eol.is_empty())
    }
}

fn is_comment(text: &str) -> bool {
    text.trim_start().starts_with(';')
}

/// The text before `:` (trimmed); `None` for comments and blank keys.
pub fn line_key(text: &str) -> Option<&str> {
    if is_comment(text) {
        return None;
    }
    let k = text.split(':').next().unwrap_or("").trim();
    (!k.is_empty()).then_some(k)
}

fn key_matches(text: &str, key: &str) -> bool {
    line_key(text).is_some_and(|k| k.to_lowercase() == key.trim().to_lowercase())
}

fn not_utf8(path: &Path) -> Error {
    Error::Invalid(format!("{} is not UTF-8 text", path.display()))
}

/// Pure version of [`ensure_line`]: `None` input means the file is missing.
/// Returns the new contents (`None` when nothing changes) and the change.
pub fn ensure_in_text(
    bytes: Option<&[u8]>,
    key: &str,
    line: &str,
    before: Option<&str>,
) -> std::result::Result<(Option<Vec<u8>>, LineChange), std::str::Utf8Error> {
    let Some(bytes) = bytes else {
        let change = LineChange {
            added: true,
            replaced: None,
            created_file: true,
            trailing_newline: true,
        };
        return Ok((Some(format!("{line}{DEFAULT_EOL}").into_bytes()), change));
    };
    let mut doc = Doc::parse(bytes)?;
    let trailing_newline = doc.ends_with_newline();

    if let Some(i) = doc.find(key) {
        if doc.lines[i].text.trim() == line.trim() {
            return Ok((None, LineChange::noop()));
        }
        let original = std::mem::replace(&mut doc.lines[i].text, line.to_string());
        let change = LineChange {
            added: false,
            replaced: Some(original),
            created_file: false,
            trailing_newline,
        };
        return Ok((Some(doc.render()), change));
    }

    let eol = doc.eol();
    let anchor = before.and_then(|b| doc.find(b)).map(|mut i| {
        // Keep the comment block that introduces the anchor attached to it.
        while i > 0 && is_comment(&doc.lines[i - 1].text) {
            i -= 1;
        }
        i
    });
    match anchor {
        Some(i) => doc.lines.insert(
            i,
            Line {
                text: line.to_string(),
                eol,
            },
        ),
        None => {
            let new_eol = match doc.lines.last_mut() {
                Some(last) if last.eol.is_empty() => {
                    last.eol = eol;
                    ""
                }
                _ => eol,
            };
            doc.lines.push(Line {
                text: line.to_string(),
                eol: new_eol,
            });
        }
    }
    let change = LineChange {
        added: true,
        replaced: None,
        created_file: false,
        trailing_newline,
    };
    Ok((Some(doc.render()), change))
}

/// What reverting does to the file.
#[derive(Debug, PartialEq, Eq)]
pub enum RevertOutcome {
    Unchanged,
    Write(Vec<u8>),
    Delete,
}

/// Pure version of [`revert_line`].
pub fn revert_in_text(
    bytes: &[u8],
    key: &str,
    change: &LineChange,
) -> std::result::Result<RevertOutcome, std::str::Utf8Error> {
    if change.is_noop() {
        return Ok(RevertOutcome::Unchanged);
    }
    let mut doc = Doc::parse(bytes)?;
    let Some(i) = doc.find(key) else {
        return Ok(RevertOutcome::Unchanged); // someone already removed it
    };
    if let Some(original) = &change.replaced {
        if doc.lines[i].text == *original {
            return Ok(RevertOutcome::Unchanged);
        }
        doc.lines[i].text = original.clone();
    } else {
        let was_last = i + 1 == doc.lines.len();
        let removed = doc.lines.remove(i);
        if was_last && removed.eol.is_empty() && !change.trailing_newline {
            if let Some(last) = doc.lines.last_mut() {
                last.eol = "";
            }
        }
        if change.created_file && !doc.bom && doc.lines.is_empty() {
            return Ok(RevertOutcome::Delete);
        }
    }
    Ok(RevertOutcome::Write(doc.render()))
}

fn read_opt(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::Io {
            op: "read",
            path: path.to_path_buf(),
            source: e,
        }),
    }
}

/// Makes sure a line with `key` reads `line`: no-op if it already does,
/// replaces a differing one, otherwise inserts it before the line keyed
/// `before` (above that line's comment block) or appends. A missing file is
/// created with just the line. The parent directory must exist.
pub fn ensure_line(path: &Path, key: &str, line: &str, before: Option<&str>) -> Result<LineChange> {
    let bytes = read_opt(path)?;
    let (new, change) =
        ensure_in_text(bytes.as_deref(), key, line, before).map_err(|_| not_utf8(path))?;
    if let Some(new) = new {
        fsx::write_atomic(path, &new)?;
    }
    Ok(change)
}

/// Reverses an [`ensure_line`]: removes the line we added or restores the line
/// we replaced. Other lines are left alone; a missing file is fine.
pub fn revert_line(path: &Path, key: &str, change: &LineChange) -> Result<()> {
    if change.is_noop() {
        return Ok(());
    }
    let Some(bytes) = read_opt(path)? else {
        return Ok(());
    };
    match revert_in_text(&bytes, key, change).map_err(|_| not_utf8(path))? {
        RevertOutcome::Unchanged => Ok(()),
        RevertOutcome::Write(new) => fsx::write_atomic(path, &new),
        RevertOutcome::Delete => fs::remove_file(path).ctx("delete", path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ensure(text: &str, key: &str, line: &str, before: Option<&str>) -> (String, LineChange) {
        let (new, ch) = ensure_in_text(Some(text.as_bytes()), key, line, before).unwrap();
        (
            new.map(|b| String::from_utf8(b).unwrap())
                .unwrap_or_else(|| text.to_string()),
            ch,
        )
    }

    fn revert(text: &str, key: &str, ch: &LineChange) -> String {
        match revert_in_text(text.as_bytes(), key, ch).unwrap() {
            RevertOutcome::Unchanged => text.to_string(),
            RevertOutcome::Write(b) => String::from_utf8(b).unwrap(),
            RevertOutcome::Delete => panic!("unexpected delete"),
        }
    }

    /// ensure then revert must round-trip byte-exactly.
    fn roundtrip(text: &str, key: &str, line: &str, before: Option<&str>) -> (String, LineChange) {
        let (after, ch) = ensure(text, key, line, before);
        assert_eq!(
            revert(&after, key, &ch),
            text,
            "revert of {text:?} -> {after:?}"
        );
        // Idempotent: ensuring again is a no-op.
        let (again, ch2) = ensure(&after, key, line, before);
        assert_eq!(again, after);
        assert!(ch2.is_noop());
        (after, ch)
    }

    const MODS: &str = "CheatManagerEnablerMod : 1\r\nConsoleCommandsMod : 1\r\nLineTraceMod : 0\r\n\r\n; Built-in keybinds, do not move up!\r\nKeybinds : 0\r\n";

    #[test]
    fn inserts_above_keybinds_comment() {
        let (after, ch) = roundtrip(
            MODS,
            "SharedCraftingClient",
            "SharedCraftingClient : 1",
            Some("Keybinds"),
        );
        assert!(ch.added && ch.replaced.is_none());
        assert_eq!(
            after,
            "CheatManagerEnablerMod : 1\r\nConsoleCommandsMod : 1\r\nLineTraceMod : 0\r\n\r\nSharedCraftingClient : 1\r\n; Built-in keybinds, do not move up!\r\nKeybinds : 0\r\n"
        );
    }

    #[test]
    fn inserts_directly_before_anchor_without_comment() {
        let (after, _) = roundtrip("A : 1\nKeybinds : 1\n", "X", "X : 1", Some("keybinds"));
        assert_eq!(after, "A : 1\nX : 1\nKeybinds : 1\n");
    }

    #[test]
    fn appends_when_anchor_missing_or_none() {
        let (after, _) = roundtrip("A : 1\r\nB : 0\r\n", "X", "X : 1", Some("Keybinds"));
        assert_eq!(after, "A : 1\r\nB : 0\r\nX : 1\r\n");
        let (after, _) = roundtrip("A : 1\nB : 0\n", "X", "X : 1", None);
        assert_eq!(after, "A : 1\nB : 0\nX : 1\n");
    }

    #[test]
    fn no_final_newline_preserved_and_reverted() {
        let (after, ch) = roundtrip("A : 1\r\nB : 0", "X", "X : 1", None);
        assert_eq!(after, "A : 1\r\nB : 0\r\nX : 1");
        assert!(!ch.trailing_newline);
        // Single line, no newline at all -> default CRLF.
        let (after, _) = roundtrip("A : 1", "X", "X : 1", None);
        assert_eq!(after, "A : 1\r\nX : 1");
        // LF file without final newline.
        let (after, _) = roundtrip("A : 1\nB : 0", "X", "X : 1", None);
        assert_eq!(after, "A : 1\nB : 0\nX : 1");
    }

    #[test]
    fn no_final_newline_insert_in_middle() {
        let (after, _) = roundtrip("A : 1\nKeybinds : 1", "X", "X : 1", Some("Keybinds"));
        assert_eq!(after, "A : 1\nX : 1\nKeybinds : 1");
    }

    #[test]
    fn bom_preserved() {
        let text = "\u{feff}A : 1\r\n; Built-in keybinds, do not move up!\r\nKeybinds : 0";
        let (after, _) = roundtrip(text, "X", "X : 1", Some("Keybinds"));
        assert_eq!(
            after,
            "\u{feff}A : 1\r\nX : 1\r\n; Built-in keybinds, do not move up!\r\nKeybinds : 0"
        );
        let (after, _) = roundtrip("\u{feff}", "X", "X : 1", None);
        assert_eq!(after, "\u{feff}X : 1\r\n");
    }

    #[test]
    fn mixed_line_endings() {
        let text = "A : 1\r\nB : 1\nC : 1\n; c\r\nKeybinds : 1\r\nD : 1";
        let (after, _) = roundtrip(text, "X", "X : 1", Some("Keybinds"));
        // CRLF is the majority (3 vs 2).
        assert_eq!(
            after,
            "A : 1\r\nB : 1\nC : 1\nX : 1\r\n; c\r\nKeybinds : 1\r\nD : 1"
        );
        let (after, _) = roundtrip("A : 1\nB : 1\r\nC : 1\n", "X", "X : 1", None);
        assert_eq!(after, "A : 1\nB : 1\r\nC : 1\nX : 1\n");
        roundtrip(text, "X", "X : 1", None);
    }

    #[test]
    fn empty_file() {
        let (after, _) = roundtrip("", "X", "X : 1", Some("Keybinds"));
        assert_eq!(after, "X : 1\r\n");
        roundtrip("\n", "X", "X : 1", None);
        roundtrip("\r\n\r\n", "X", "X : 1", None);
    }

    #[test]
    fn replaces_differing_line_and_restores_exactly() {
        let text = "A : 1\r\n  sharedcraftingclient  :   0  \r\nKeybinds : 1\r\n";
        let (after, ch) = roundtrip(
            text,
            "SharedCraftingClient",
            "SharedCraftingClient : 1",
            Some("Keybinds"),
        );
        assert_eq!(
            after,
            "A : 1\r\nSharedCraftingClient : 1\r\nKeybinds : 1\r\n"
        );
        assert_eq!(
            ch.replaced.as_deref(),
            Some("  sharedcraftingclient  :   0  ")
        );
        assert!(!ch.added);
        // Last line without newline.
        roundtrip("A : 1\nX : 0", "X", "X : 1", None);
    }

    #[test]
    fn matching_line_is_noop() {
        let text = "A : 1\n  X : 1  \n";
        let (after, ch) = ensure(text, "x", "X : 1", None);
        assert_eq!(after, text);
        assert!(ch.is_noop());
        assert_eq!(revert(text, "x", &ch), text);
    }

    #[test]
    fn comments_never_match() {
        let text = "; X : 0\n;X : 1\nA : 1\n";
        let (after, ch) = roundtrip(text, "X", "X : 1", None);
        assert!(ch.added);
        assert_eq!(after, "; X : 0\n;X : 1\nA : 1\nX : 1\n");
    }

    #[test]
    fn user_lines_added_later_survive_revert() {
        let (after, ch) = ensure(MODS, "SC", "SC : 1", Some("Keybinds"));
        // User appends a line at the end and another right after ours.
        let edited = after.replace("SC : 1\r\n", "SC : 1\r\nUserMod : 1\r\n") + "Other : 1";
        let reverted = revert(&edited, "SC", &ch);
        assert_eq!(
            reverted,
            MODS.replace("\r\n; Built", "\r\nUserMod : 1\r\n; Built") + "Other : 1"
        );
        // We added at the end of a no-newline file, user appended after us.
        let (after, ch) = ensure("A : 1", "SC", "SC : 1", None);
        let edited = after + "\r\nU : 1";
        assert_eq!(revert(&edited, "SC", &ch), "A : 1\r\nU : 1");
    }

    #[test]
    fn user_toggled_line_still_reverted() {
        let (after, ch) = ensure("A : 1\n", "SC", "SC : 1", None);
        let toggled = after.replace("SC : 1", "SC : 0");
        assert_eq!(revert(&toggled, "SC", &ch), "A : 1\n");
        let (after, ch) = ensure("SC : 0\n", "SC", "SC : 1", None);
        let toggled = after.replace("SC : 1", "SC:1");
        assert_eq!(revert(&toggled, "SC", &ch), "SC : 0\n");
    }

    #[test]
    fn missing_file_created_and_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("mods.txt");
        let ch = ensure_line(&p, "SC", "SC : 1", Some("Keybinds")).unwrap();
        assert!(ch.created_file && ch.added);
        assert_eq!(fs::read_to_string(&p).unwrap(), "SC : 1\r\n");
        revert_line(&p, "SC", &ch).unwrap();
        assert!(!p.exists());
        // If the user added content meanwhile, the file stays.
        let ch = ensure_line(&p, "SC", "SC : 1", None).unwrap();
        fs::write(&p, "SC : 1\r\nMine : 1\r\n").unwrap();
        revert_line(&p, "SC", &ch).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "Mine : 1\r\n");
        // Reverting against a missing file is fine.
        fs::remove_file(&p).unwrap();
        revert_line(&p, "SC", &ch).unwrap();
    }

    #[test]
    fn file_level_roundtrip_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("mods.txt");
        for original in [MODS, "A : 1", "\u{feff}A : 1\nB : 0", "X : 0\r\n", ""] {
            fs::write(&p, original).unwrap();
            let ch = ensure_line(&p, "X", "X : 1", Some("Keybinds")).unwrap();
            revert_line(&p, "X", &ch).unwrap();
            assert_eq!(fs::read(&p).unwrap(), original.as_bytes());
        }
    }

    #[test]
    fn non_utf8_is_an_error_not_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("mods.txt");
        fs::write(&p, b"A : 1\n\xff\xfe\n").unwrap();
        assert!(ensure_line(&p, "X", "X : 1", None).is_err());
        assert_eq!(fs::read(&p).unwrap(), b"A : 1\n\xff\xfe\n");
    }
}
