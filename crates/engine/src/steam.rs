//! Steam install discovery: library folders and app manifests.

use std::fs;
use std::path::{Path, PathBuf};

/// Env var that overrides Steam discovery entirely (tests, sandboxes).
pub const STEAM_PATH_ENV: &str = "MODSYNC_STEAM_PATH";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppInstall {
    pub install_dir: PathBuf,
    pub name: String,
    /// `StateFlags == 4`: fully installed, no pending update/download.
    pub fully_installed: bool,
}

/// The Steam install folder. If `MODSYNC_STEAM_PATH` is set, only that is
/// considered. Otherwise: HKCU `SteamPath`, HKLM (WOW6432Node) `InstallPath`,
/// then `C:\Program Files (x86)\Steam`. The folder must exist.
pub fn steam_root() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(STEAM_PATH_ENV).filter(|p| !p.is_empty()) {
        let p = PathBuf::from(p);
        return p.is_dir().then_some(p);
    }
    candidates().into_iter().find(|p| p.is_dir())
}

#[cfg(windows)]
fn candidates() -> Vec<PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;
    let mut out = Vec::new();
    let read = |hive, path: &str, value: &str| -> Option<PathBuf> {
        let s: String = RegKey::predef(hive)
            .open_subkey(path)
            .ok()?
            .get_value(value)
            .ok()?;
        Some(PathBuf::from(s.replace('/', "\\")))
    };
    out.extend(read(
        HKEY_CURRENT_USER,
        r"Software\Valve\Steam",
        "SteamPath",
    ));
    out.extend(read(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\WOW6432Node\Valve\Steam",
        "InstallPath",
    ));
    out.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
    out
}

#[cfg(not(windows))]
fn candidates() -> Vec<PathBuf> {
    Vec::new()
}

/// All library folders: the root first, then those in
/// `steamapps/libraryfolders.vdf` (deduplicated case-insensitively).
pub fn libraries(steam_root: &Path) -> Vec<PathBuf> {
    let mut libs = vec![steam_root.to_path_buf()];
    if let Ok(text) = fs::read_to_string(steam_root.join("steamapps").join("libraryfolders.vdf")) {
        for p in parse_library_folders(&text) {
            let norm = |p: &Path| {
                p.to_string_lossy()
                    .replace('/', "\\")
                    .trim_end_matches('\\')
                    .to_lowercase()
            };
            if !libs.iter().any(|l| norm(l) == norm(&p)) {
                libs.push(p);
            }
        }
    }
    libs
}

/// Finds an installed app by id across all libraries.
pub fn find_app(steam_root: &Path, app_id: u32) -> Option<AppInstall> {
    libraries(steam_root).into_iter().find_map(|lib| {
        let steamapps = lib.join("steamapps");
        let text = fs::read_to_string(steamapps.join(format!("appmanifest_{app_id}.acf"))).ok()?;
        let m = parse_app_manifest(&text)?;
        let install_dir = steamapps.join("common").join(&m.install_dir);
        install_dir.is_dir().then(|| AppInstall {
            install_dir,
            name: m.name,
            fully_installed: m.state_flags == Some(4),
        })
    })
}

/// The fields of an `appmanifest_<id>.acf` we care about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppManifest {
    pub install_dir: String,
    pub name: String,
    pub state_flags: Option<u64>,
}

pub fn parse_app_manifest(text: &str) -> Option<AppManifest> {
    let root = vdf::parse(text)?;
    let state = root.get("AppState")?;
    let install_dir = state.get("installdir")?.as_str()?.to_string();
    if install_dir.is_empty() || install_dir.contains(['/', '\\', ':']) || install_dir == ".." {
        return None;
    }
    Some(AppManifest {
        name: state
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        state_flags: state
            .get("StateFlags")
            .and_then(|v| v.as_str())
            .and_then(|s| s.trim().parse().ok()),
        install_dir,
    })
}

/// Library paths from `libraryfolders.vdf` (new `"0" { "path" ... }` format and
/// the old `"1" "D:\\Lib"` one).
pub fn parse_library_folders(text: &str) -> Vec<PathBuf> {
    let Some(root) = vdf::parse(text) else {
        return Vec::new();
    };
    let Some(vdf::Value::Obj(entries)) = root.get("libraryfolders") else {
        return Vec::new();
    };
    entries
        .iter()
        .filter(|(k, _)| k.chars().all(|c| c.is_ascii_digit()))
        .filter_map(|(_, v)| match v {
            vdf::Value::Str(s) => Some(s.clone()),
            obj => obj.get("path").and_then(|p| p.as_str()).map(str::to_string),
        })
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Minimal Valve KeyValues (VDF) text parser.
pub mod vdf {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Value {
        Str(String),
        Obj(Vec<(String, Value)>),
    }

    impl Value {
        /// Case-insensitive key lookup (first match).
        pub fn get(&self, key: &str) -> Option<&Value> {
            match self {
                Value::Obj(kv) => kv
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(key))
                    .map(|(_, v)| v),
                Value::Str(_) => None,
            }
        }
        pub fn as_str(&self) -> Option<&str> {
            match self {
                Value::Str(s) => Some(s),
                Value::Obj(_) => None,
            }
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Tok {
        Str(String),
        Open,
        Close,
    }

    fn tokenize(text: &str) -> Option<Vec<Tok>> {
        let mut out = Vec::new();
        let mut it = text.chars().peekable();
        while let Some(&c) = it.peek() {
            match c {
                c if c.is_whitespace() || c == '\u{feff}' => {
                    it.next();
                }
                '{' => {
                    it.next();
                    out.push(Tok::Open);
                }
                '}' => {
                    it.next();
                    out.push(Tok::Close);
                }
                '/' => {
                    it.next();
                    if it.peek() == Some(&'/') {
                        for c in it.by_ref() {
                            if c == '\n' {
                                break;
                            }
                        }
                    } else {
                        return None;
                    }
                }
                '"' => {
                    it.next();
                    let mut s = String::new();
                    loop {
                        match it.next()? {
                            '"' => break,
                            '\\' => match it.next()? {
                                'n' => s.push('\n'),
                                't' => s.push('\t'),
                                other => s.push(other), // \\ and \"
                            },
                            c => s.push(c),
                        }
                    }
                    out.push(Tok::Str(s));
                }
                _ => {
                    // Unquoted token (rare; e.g. `#include`, conditionals).
                    let mut s = String::new();
                    while let Some(&c) = it.peek() {
                        if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                            break;
                        }
                        s.push(c);
                        it.next();
                    }
                    out.push(Tok::Str(s));
                }
            }
        }
        Some(out)
    }

    fn parse_obj(toks: &[Tok], i: &mut usize, nested: bool) -> Option<Value> {
        let mut kv = Vec::new();
        loop {
            match toks.get(*i) {
                None if !nested => return Some(Value::Obj(kv)),
                Some(Tok::Close) if nested => {
                    *i += 1;
                    return Some(Value::Obj(kv));
                }
                Some(Tok::Str(k)) => {
                    *i += 1;
                    let v = match toks.get(*i)? {
                        Tok::Str(s) => {
                            *i += 1;
                            Value::Str(s.clone())
                        }
                        Tok::Open => {
                            *i += 1;
                            parse_obj(toks, i, true)?
                        }
                        Tok::Close => return None,
                    };
                    kv.push((k.clone(), v));
                }
                _ => return None,
            }
        }
    }

    /// Parses a whole document into a top-level object.
    pub fn parse(text: &str) -> Option<Value> {
        let toks = tokenize(text)?;
        parse_obj(&toks, &mut 0, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIBRARYFOLDERS: &str = r#""libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
		"contentid"		"1234567890"
		"totalsize"		"0"
		"update_clean_bytes_tally"		"0"
		"time_last_update_verified"		"0"
		"apps"
		{
			"228980"		"612354271"
		}
	}
	"1"
	{
		"path"		"F:\\SteamLibrary"
		"label"		"games \"fast\""
		"apps"
		{
			"1857950"		"20123456789"
		}
	}
}
"#;

    const ACF: &str = r#""AppState"
{
	"appid"		"1857950"
	"universe"		"1"
	"LauncherPath"		"C:\\Program Files (x86)\\Steam\\steam.exe"
	"name"		"VEIN"
	"StateFlags"		"4"
	"installdir"		"Vein"
	"InstalledDepots"
	{
		"1857951"
		{
			"manifest"		"123"
			"size"		"456"
		}
	}
	"UserConfig" { "language" "english" }
}
"#;

    #[test]
    fn parses_library_folders() {
        assert_eq!(
            parse_library_folders(LIBRARYFOLDERS),
            vec![
                PathBuf::from(r"C:\Program Files (x86)\Steam"),
                PathBuf::from(r"F:\SteamLibrary")
            ]
        );
        let root = vdf::parse(LIBRARYFOLDERS).unwrap();
        let label = root
            .get("libraryfolders")
            .unwrap()
            .get("1")
            .unwrap()
            .get("label")
            .unwrap();
        assert_eq!(label.as_str(), Some("games \"fast\""));
    }

    #[test]
    fn parses_old_library_format_and_comments() {
        let text = "// comment\n\"LibraryFolders\"\n{\n\t\"TimeNextStatsReport\"\t\"123\"\n\t\"ContentStatsID\"\t\"-1\"\n\t\"1\"\t\"D:\\\\Games\\\\Steam\"\n}\n";
        assert_eq!(
            parse_library_folders(text),
            vec![PathBuf::from(r"D:\Games\Steam")]
        );
    }

    #[test]
    fn parses_app_manifest() {
        let m = parse_app_manifest(ACF).unwrap();
        assert_eq!(
            m,
            AppManifest {
                install_dir: "Vein".into(),
                name: "VEIN".into(),
                state_flags: Some(4)
            }
        );
        let updating = ACF.replace("\"StateFlags\"\t\t\"4\"", "\"StateFlags\"\t\t\"6\"");
        assert_eq!(parse_app_manifest(&updating).unwrap().state_flags, Some(6));
        assert!(parse_app_manifest(&ACF.replace("\"Vein\"", "\"..\\\\x\"")).is_none());
    }

    #[test]
    fn rejects_malformed() {
        assert!(vdf::parse("\"a\" { \"b\" \"c\" ").is_none());
        assert!(vdf::parse("\"a\" }").is_none());
        assert!(vdf::parse("\"unterminated").is_none());
        assert!(parse_library_folders("garbage {").is_empty());
    }
}
