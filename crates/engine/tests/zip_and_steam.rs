use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use modsync_engine::{extract_zip, sha256_file, steam, Error};
use zip::write::SimpleFileOptions;

fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut w = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in entries {
        if name.ends_with('/') {
            w.add_directory(*name, opts).unwrap();
        } else {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
    }
    w.finish().unwrap();
}

#[test]
fn extracts_nested_package() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = tmp.path().join("p.zip");
    make_zip(
        &zip,
        &[
            ("ue4ss/", b""),
            ("dwmapi.dll", b"proxy"),
            ("ue4ss/Mods/x/main.lua", b"lua"),
            ("./ue4ss/a.txt", b"a"),
        ],
    );
    let out = tmp.path().join("out");
    let files = extract_zip(&zip, &out).unwrap();
    assert_eq!(
        files,
        vec!["dwmapi.dll", "ue4ss/Mods/x/main.lua", "ue4ss/a.txt"]
    );
    assert_eq!(fs::read(out.join("ue4ss/Mods/x/main.lua")).unwrap(), b"lua");
    assert_eq!(fs::read(out.join("ue4ss/a.txt")).unwrap(), b"a");
}

#[test]
fn zip_slip_is_rejected_before_writing_anything() {
    let tmp = tempfile::tempdir().unwrap();
    for (i, bad) in [
        "../evil.txt",
        "a/../../evil.txt",
        "/abs.txt",
        "\\abs.txt",
        "C:/evil.txt",
        "c:evil.txt",
        "a\\..\\..\\evil.txt",
        "file.txt:stream",
    ]
    .iter()
    .enumerate()
    {
        let zip = tmp.path().join(format!("bad{i}.zip"));
        make_zip(&zip, &[("good.txt", b"ok"), (bad, b"evil")]);
        let out = tmp.path().join(format!("out{i}"));
        let err = extract_zip(&zip, &out).unwrap_err();
        assert!(matches!(err, Error::UnsafePath(_)), "{bad}: {err}");
        assert!(!out.join("good.txt").exists(), "{bad}: nothing extracted");
    }
    assert!(!tmp.path().join("evil.txt").exists());
}

#[test]
fn duplicate_entries_differing_by_case_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = tmp.path().join("dup.zip");
    make_zip(&zip, &[("A.txt", b"1"), ("a.txt", b"2")]);
    assert!(extract_zip(&zip, &tmp.path().join("out")).is_err());
}

#[test]
fn sha256_of_known_content() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("abc");
    fs::write(&p, b"abc").unwrap();
    assert_eq!(
        sha256_file(&p).unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

// Tests that touch MODSYNC_STEAM_PATH must not run concurrently.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn vdf_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "\\\\")
}

#[test]
fn steam_env_override_with_fake_library() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Steam");
    let lib = tmp.path().join("SteamLibrary");
    fs::create_dir_all(root.join("steamapps")).unwrap();
    fs::create_dir_all(lib.join("steamapps/common/Vein")).unwrap();
    fs::write(
        root.join("steamapps/libraryfolders.vdf"),
        format!(
            "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t\t\"apps\" {{ }}\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t\t\"apps\"\n\t\t{{\n\t\t\t\"1857950\"\t\t\"1\"\n\t\t}}\n\t}}\n}}\n",
            vdf_path(&root),
            vdf_path(&lib)
        ),
    )
    .unwrap();
    fs::write(
        lib.join("steamapps/appmanifest_1857950.acf"),
        "\"AppState\"\n{\n\t\"appid\"\t\t\"1857950\"\n\t\"name\"\t\t\"VEIN\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"Vein\"\n}\n",
    )
    .unwrap();
    // Manifest for an app whose folder is gone.
    fs::write(
        lib.join("steamapps/appmanifest_42.acf"),
        "\"AppState\" { \"name\" \"Gone\" \"StateFlags\" \"4\" \"installdir\" \"Gone\" }",
    )
    .unwrap();

    std::env::set_var(steam::STEAM_PATH_ENV, &root);
    let found_root = steam::steam_root();
    std::env::set_var(steam::STEAM_PATH_ENV, tmp.path().join("missing"));
    let missing_root = steam::steam_root();
    std::env::remove_var(steam::STEAM_PATH_ENV);

    assert_eq!(found_root.as_deref(), Some(root.as_path()));
    assert_eq!(missing_root, None, "env override is exclusive");
    assert_eq!(steam::libraries(&root), vec![root.clone(), lib.clone()]);
    let app = steam::find_app(&root, 1857950).unwrap();
    assert_eq!(
        app,
        steam::AppInstall {
            install_dir: lib.join("steamapps").join("common").join("Vein"),
            name: "VEIN".into(),
            fully_installed: true
        }
    );
    assert_eq!(steam::find_app(&root, 42), None);
    assert_eq!(steam::find_app(&root, 7), None);
}

#[test]
fn steam_libraries_without_vdf_is_just_root() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(
        steam::libraries(tmp.path()),
        vec![PathBuf::from(tmp.path())]
    );
}

#[test]
#[ignore = "needs the real Steam install with VEIN (run with --ignored)"]
fn steam_finds_real_vein() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = steam::steam_root().expect("steam root");
    let app = steam::find_app(&root, 1857950).expect("VEIN installed");
    println!("{root:?} -> {app:?}");
    assert!(app
        .install_dir
        .to_string_lossy()
        .to_lowercase()
        .ends_with(r"steamapps\common\vein"));
    assert_eq!(app.name, "VEIN");
}
