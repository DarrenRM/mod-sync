mod common;

use std::fs;

use common::*;
use modsync_engine::{recover, sha256_file, uninstall, verify, Error, InstallRecord};

fn sha(b: &[u8]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("x");
    fs::write(&p, b).unwrap();
    sha256_file(&p).unwrap()
}

fn entry<'a>(rec: &'a InstallRecord, path: &str) -> Option<&'a modsync_engine::FileEntry> {
    rec.files.iter().find(|f| f.path.eq_ignore_ascii_case(path))
}

fn seed_game(env: &Env) {
    write(&env.game, "Vein-Win64-Shipping.exe", b"game exe");
    write(&env.game, "dwmapi.dll", b"original dwmapi");
    write(&env.game, "other/readme.txt", b"hello");
}

const V1: &[(&str, &[u8])] = &[
    ("dwmapi.dll", b"ue4ss proxy"),
    ("ue4ss/UE4SS.dll", b"ue4ss core"),
    ("ue4ss/UE4SS-settings.ini", b"[General]\r\n"),
    ("ue4ss/Mods/SharedCrafting/Scripts/main.lua", b"print('hi')"),
    ("other/new.txt", b"new"),
];

#[test]
fn fresh_install_then_uninstall_restores_tree() {
    let env = Env::new();
    seed_game(&env);
    let before = env.game_snap();

    let staged = env.stage(V1);
    let rec = env.apply(&staged, &spec("1.0.0", &[], &["ue4ss"], vec![]), None);

    assert_eq!(rec.game_id, "vein");
    assert_eq!(rec.version, "1.0.0");
    assert_eq!(rec.files.len(), 5);
    let dw = entry(&rec, "dwmapi.dll").unwrap();
    assert_eq!(dw.backup.as_deref(), Some("dwmapi.dll"));
    assert_eq!(dw.sha256, sha(b"ue4ss proxy"));
    assert_eq!(read(&env.backups, "dwmapi.dll"), b"original dwmapi");
    assert_eq!(entry(&rec, "other/new.txt").unwrap().backup, None);
    assert_eq!(
        rec.created_dirs,
        vec![
            "ue4ss",
            "ue4ss/Mods",
            "ue4ss/Mods/SharedCrafting",
            "ue4ss/Mods/SharedCrafting/Scripts"
        ]
    );
    assert_eq!(rec.owned_dirs, vec!["ue4ss"]);
    assert_eq!(read(&env.game, "dwmapi.dll"), b"ue4ss proxy");
    assert!(
        rec.installed_at.contains('T') && rec.installed_at.ends_with('Z'),
        "{}",
        rec.installed_at
    );
    assert!(verify(&rec).is_ok());

    uninstall(&rec, &env.backups).unwrap();
    assert_same(&before, &env.game_snap());
    assert!(
        env.backup_snap().is_empty(),
        "backups left: {:?}",
        env.backup_snap().keys()
    );
}

#[test]
fn verify_reports_missing_and_modified() {
    let env = Env::new();
    let staged = env.stage(V1);
    let rec = env.apply(
        &staged,
        &spec("1", &["ue4ss/UE4SS-settings.ini"], &[], vec![]),
        None,
    );
    fs::remove_file(env.game.join("other/new.txt")).unwrap();
    fs::write(env.game.join("ue4ss/UE4SS.dll"), b"tampered").unwrap();
    fs::write(env.game.join("ue4ss/UE4SS-settings.ini"), b"user tweak").unwrap(); // preserved: fine
    let r = verify(&rec);
    assert_eq!(r.missing, vec!["other/new.txt"]);
    assert_eq!(r.modified, vec!["ue4ss/UE4SS.dll"]);
}

#[test]
fn owned_dir_created_by_us_is_swept_with_runtime_junk() {
    let env = Env::new();
    seed_game(&env);
    let before = env.game_snap();
    let rec = env.apply(&env.stage(V1), &spec("1", &[], &["ue4ss"], vec![]), None);

    write(&env.game, "ue4ss/UE4SS.log", b"log");
    write(&env.game, "ue4ss/crash_1.dmp", b"dump");
    write(&env.game, "ue4ss/Mods/SharedCrafting/cache/x.bin", b"junk");

    uninstall(&rec, &env.backups).unwrap();
    assert_same(&before, &env.game_snap());
}

#[test]
fn owned_dir_that_preexisted_keeps_user_files() {
    let env = Env::new();
    seed_game(&env);
    write(&env.game, "ue4ss/user-notes.txt", b"mine");
    write(
        &env.game,
        "ue4ss/Mods/UserMod/Scripts/main.lua",
        b"user mod",
    );
    let before = env.game_snap();

    let rec = env.apply(&env.stage(V1), &spec("1", &[], &["ue4ss"], vec![]), None);
    assert!(!rec
        .created_dirs
        .iter()
        .any(|d| d == "ue4ss" || d == "ue4ss/Mods"));
    assert!(rec
        .created_dirs
        .contains(&"ue4ss/Mods/SharedCrafting".to_string()));
    write(&env.game, "ue4ss/UE4SS.log", b"log");

    uninstall(&rec, &env.backups).unwrap();
    let mut after = env.game_snap();
    // We never owned ue4ss/, so the runtime log stays; everything else is as before.
    assert_eq!(after.remove("ue4ss/UE4SS.log"), Some(Some(b"log".to_vec())));
    assert_same(&before, &after);
}

#[test]
fn preserved_mods_txt_created_when_missing() {
    let env = Env::new();
    seed_game(&env);
    let before = env.game_snap();
    let staged = env.stage(&[
        ("ue4ss/UE4SS.dll", b"core"),
        ("ue4ss/Mods/mods.txt", b"BPML_GenericFunctions : 1\r\n\r\n; Built-in keybinds, do not move up!\r\nKeybinds : 1\r\n"),
    ]);
    let s = spec(
        "1",
        &["ue4ss/Mods/mods.txt", "**/config_local.lua"],
        &["ue4ss"],
        vec![line_edit(
            "ue4ss/Mods/mods.txt",
            "SharedCraftingClient",
            "SharedCraftingClient : 1",
            Some("Keybinds"),
        )],
    );
    let rec = env.apply(&staged, &s, None);

    let mods = entry(&rec, "ue4ss/Mods/mods.txt").expect("created preserved file is owned");
    assert!(mods.preserved && mods.backup.is_none());
    let text = read(&env.game, "ue4ss/Mods/mods.txt");
    assert_eq!(
        String::from_utf8(text.clone()).unwrap(),
        "BPML_GenericFunctions : 1\r\n\r\nSharedCraftingClient : 1\r\n; Built-in keybinds, do not move up!\r\nKeybinds : 1\r\n"
    );
    assert_eq!(mods.sha256, sha(&text), "hash recorded after the edit");
    assert!(rec.edits[0].added);
    assert!(verify(&rec).is_ok());

    uninstall(&rec, &env.backups).unwrap();
    assert_same(&before, &env.game_snap());
}

#[test]
fn preserved_mods_txt_kept_when_present_and_edit_reverted_exactly() {
    let env = Env::new();
    seed_game(&env);
    write(&env.game, "ue4ss/Mods/mods.txt", MODS_TXT);
    let before = env.game_snap();

    let staged = env.stage(&[
        ("ue4ss/UE4SS.dll", b"core"),
        ("ue4ss/Mods/mods.txt", b"Keybinds : 1\r\n"),
    ]);
    let s = spec(
        "1",
        &["UE4SS/mods/MODS.TXT"], // case-insensitive
        &["ue4ss"],
        vec![line_edit(
            "ue4ss/Mods/mods.txt",
            "SharedCraftingClient",
            "SharedCraftingClient : 1",
            Some("Keybinds"),
        )],
    );
    let rec = env.apply(&staged, &s, None);
    assert!(entry(&rec, "ue4ss/Mods/mods.txt").is_none(), "not ours");
    assert_eq!(
        String::from_utf8(read(&env.game, "ue4ss/Mods/mods.txt")).unwrap(),
        MODS_TXT.replace("\r\n; Built", "\r\nSharedCraftingClient : 1\r\n; Built")
    );

    uninstall(&rec, &env.backups).unwrap();
    assert_same(&before, &env.game_snap());
}

#[test]
fn edit_replacing_disabled_line_is_restored() {
    let env = Env::new();
    let original = MODS_TXT.replace(
        "LineTraceMod : 0",
        "LineTraceMod : 0\r\nSharedCraftingClient : 0",
    );
    write(&env.game, "ue4ss/Mods/mods.txt", &original);
    let before = env.game_snap();
    let s = spec(
        "1",
        &[],
        &[],
        vec![line_edit(
            "ue4ss/Mods/mods.txt",
            "SharedCraftingClient",
            "SharedCraftingClient : 1",
            Some("Keybinds"),
        )],
    );
    let rec = env.apply(&env.stage(&[("ue4ss/x.dll", b"x")]), &s, None);
    assert_eq!(
        rec.edits[0].replaced.as_deref(),
        Some("SharedCraftingClient : 0")
    );
    assert!(!rec.edits[0].added);
    uninstall(&rec, &env.backups).unwrap();
    assert_same(&before, &env.game_snap());
}

#[test]
fn edit_on_missing_file_creates_and_removes_it() {
    let env = Env::new();
    seed_game(&env);
    let before = env.game_snap();
    let s = spec(
        "1",
        &[],
        &[],
        vec![line_edit("ue4ss/Mods/mods.txt", "SC", "SC : 1", None)],
    );
    let rec = env.apply(&env.stage(&[("a.dll", b"a")]), &s, None);
    assert!(rec.edits[0].created_file);
    assert_eq!(read(&env.game, "ue4ss/Mods/mods.txt"), b"SC : 1\r\n");
    assert_eq!(rec.created_dirs, vec!["ue4ss", "ue4ss/Mods"]);
    uninstall(&rec, &env.backups).unwrap();
    assert_same(&before, &env.game_snap());
}

fn v2_scenario() -> (Env, Snapshot, InstallRecord) {
    let env = Env::new();
    seed_game(&env);
    write(&env.game, "old_overwrite.txt", b"original old");
    write(&env.game, "ue4ss/Mods/mods.txt", MODS_TXT);
    let before = env.game_snap();

    let v1 = env.stage(&[
        ("dwmapi.dll", b"proxy v1"),
        ("old_overwrite.txt", b"mod old"),
        ("ue4ss/a.lua", b"a v1"),
        ("ue4ss/old.lua", b"old v1"),
        ("ue4ss/Mods/OldMod/Scripts/x.lua", b"old mod"),
        ("ue4ss/Mods/Shared/config_local.lua", b"-- defaults v1"),
        ("ue4ss/Mods/mods.txt", b"Keybinds : 1\r\n"),
    ]);
    let s1 = spec(
        "1.0.0",
        &["ue4ss/Mods/mods.txt", "**/config_local.lua"],
        &["ue4ss"],
        vec![
            line_edit(
                "ue4ss/Mods/mods.txt",
                "SharedCraftingClient",
                "SharedCraftingClient : 1",
                Some("Keybinds"),
            ),
            line_edit(
                "ue4ss/Mods/mods.txt",
                "OldMod",
                "OldMod : 1",
                Some("Keybinds"),
            ),
        ],
    );
    let r1 = env.apply(&v1, &s1, None);
    assert!(entry(&r1, "old_overwrite.txt").unwrap().backup.is_some());
    // The user tweaks their local config.
    write(
        &env.game,
        "ue4ss/Mods/Shared/config_local.lua",
        b"-- user tweaks",
    );
    (env, before, r1)
}

fn v2_spec() -> modsync_engine::PackageSpec {
    spec(
        "2.0.0",
        &["ue4ss/Mods/mods.txt", "**/config_local.lua"],
        &["ue4ss"],
        vec![line_edit(
            "ue4ss/Mods/mods.txt",
            "SharedCraftingClient",
            "SharedCraftingClient : 1",
            Some("Keybinds"),
        )],
    )
}

const V2: &[(&str, &[u8])] = &[
    ("dwmapi.dll", b"proxy v1"),
    ("ue4ss/a.lua", b"a v2"),
    ("ue4ss/new.lua", b"new in v2"),
    ("ue4ss/Mods/Shared/config_local.lua", b"-- defaults v2"),
    ("ue4ss/Mods/mods.txt", b"Keybinds : 1\r\n"),
];

#[test]
fn update_v1_to_v2_then_uninstall() {
    let (env, before, r1) = v2_scenario();
    let r2 = env.apply(&env.stage(V2), &v2_spec(), Some(&r1));

    assert_eq!(r2.version, "2.0.0");
    // Dropped files reverted: overwritten one restored, created ones deleted.
    assert_eq!(read(&env.game, "old_overwrite.txt"), b"original old");
    assert!(entry(&r2, "old_overwrite.txt").is_none());
    assert!(!env.game.join("ue4ss/old.lua").exists());
    assert!(
        !env.game.join("ue4ss/Mods/OldMod").exists(),
        "empty created dir removed"
    );
    assert!(!r2.created_dirs.iter().any(|d| d.contains("OldMod")));
    // Changed file updated, new file added.
    assert_eq!(read(&env.game, "ue4ss/a.lua"), b"a v2");
    assert_eq!(entry(&r2, "ue4ss/a.lua").unwrap().sha256, sha(b"a v2"));
    assert_eq!(read(&env.game, "ue4ss/new.lua"), b"new in v2");
    // Unchanged overwrite keeps its original backup.
    assert_eq!(
        entry(&r2, "dwmapi.dll").unwrap().backup,
        entry(&r1, "dwmapi.dll").unwrap().backup
    );
    // User-edited preserved file kept, still ours.
    assert_eq!(
        read(&env.game, "ue4ss/Mods/Shared/config_local.lua"),
        b"-- user tweaks"
    );
    let cfg = entry(&r2, "ue4ss/Mods/Shared/config_local.lua").unwrap();
    assert!(cfg.preserved);
    // Dropped edit reverted, kept edit still there with its original state.
    let mods = String::from_utf8(read(&env.game, "ue4ss/Mods/mods.txt")).unwrap();
    assert!(!mods.contains("OldMod"));
    assert_eq!(
        mods,
        MODS_TXT.replace("\r\n; Built", "\r\nSharedCraftingClient : 1\r\n; Built")
    );
    assert_eq!(r2.edits.len(), 1);
    assert!(r2.edits[0].added);
    // Used backup for the dropped file is cleaned up.
    assert!(!env.backups.join("old_overwrite.txt").exists());
    assert!(verify(&r2).is_ok());

    uninstall(&r2, &env.backups).unwrap();
    assert_same(&before, &env.game_snap());
    assert!(env.backup_snap().is_empty());
}

#[test]
fn reapplying_same_version_is_a_noop() {
    let (env, before, r1) = v2_scenario();
    let mid = env.game_snap();
    let s1 = spec(
        "1.0.0",
        &["ue4ss/Mods/mods.txt", "**/config_local.lua"],
        &["ue4ss"],
        vec![
            line_edit(
                "ue4ss/Mods/mods.txt",
                "SharedCraftingClient",
                "SharedCraftingClient : 1",
                Some("Keybinds"),
            ),
            line_edit(
                "ue4ss/Mods/mods.txt",
                "OldMod",
                "OldMod : 1",
                Some("Keybinds"),
            ),
        ],
    );
    let v1 = env.stage(&[
        ("dwmapi.dll", b"proxy v1"),
        ("old_overwrite.txt", b"mod old"),
        ("ue4ss/a.lua", b"a v1"),
        ("ue4ss/old.lua", b"old v1"),
        ("ue4ss/Mods/OldMod/Scripts/x.lua", b"old mod"),
        ("ue4ss/Mods/Shared/config_local.lua", b"-- defaults v1"),
        ("ue4ss/Mods/mods.txt", b"Keybinds : 1\r\n"),
    ]);
    let again = env.apply(&v1, &s1, Some(&r1));
    assert_same(&mid, &env.game_snap());
    assert_eq!(again.edits, r1.edits);
    assert_eq!(again.created_dirs, r1.created_dirs);
    let backups: Vec<_> = again.files.iter().map(|f| (&f.path, &f.backup)).collect();
    let backups1: Vec<_> = r1.files.iter().map(|f| (&f.path, &f.backup)).collect();
    assert_eq!(backups, backups1);
    uninstall(&again, &env.backups).unwrap();
    assert_same(&before, &env.game_snap());
}

#[test]
fn rollback_on_fresh_install_failure() {
    let env = Env::new();
    seed_game(&env);
    // A directory sits where the package wants a file; it sorts last, so the
    // backup of dwmapi.dll and the new folders must all be undone.
    write(&env.game, "zz/blocked.txt/inner.txt", b"x");
    let before = env.game_snap();

    let mut files = V1.to_vec();
    files.push(("zz/blocked.txt", b"mod"));
    let s = spec(
        "1",
        &[],
        &["ue4ss"],
        vec![line_edit("ue4ss/Mods/mods.txt", "SC", "SC : 1", None)],
    );
    let err = env.try_apply(&env.stage(&files), &s, None).unwrap_err();
    assert!(!matches!(err, Error::RollbackFailed { .. }), "{err}");

    assert_same(&before, &env.game_snap());
    assert!(
        env.backup_snap().is_empty(),
        "backups left: {:?}",
        env.backup_snap().keys()
    );
    assert!(!env.work.join("undo.jsonl").exists());
}

#[test]
fn rollback_on_update_failure_late_in_edits() {
    let (env, before, r1) = v2_scenario();
    let mid_game = env.game_snap();
    let mid_backups = env.backup_snap();

    // Everything succeeds (files written, dropped files reverted, dropped edit
    // reverted) until an edit targets a path that is a directory.
    fs::create_dir_all(env.game.join("ue4ss/blocker.txt")).unwrap();
    let mid_game_with_blocker = env.game_snap();
    let mut s = v2_spec();
    s.edits
        .push(line_edit("ue4ss/blocker.txt", "X", "X : 1", None));
    let err = env.try_apply(&env.stage(V2), &s, Some(&r1)).unwrap_err();
    assert!(!matches!(err, Error::RollbackFailed { .. }), "{err}");

    assert_same(&mid_game_with_blocker, &env.game_snap());
    assert_same(&mid_backups, &env.backup_snap());
    fs::remove_dir(env.game.join("ue4ss/blocker.txt")).unwrap();
    assert_same(&mid_game, &env.game_snap());

    // The untouched v1 record still uninstalls cleanly.
    uninstall(&r1, &env.backups).unwrap();
    assert_same(&before, &env.game_snap());
}

#[cfg(windows)]
#[test]
fn rollback_when_file_locked_on_fresh_install() {
    let env = Env::new();
    seed_game(&env);
    write(&env.game, "zz.dll", b"original zz");
    let before = env.game_snap();
    let mut files = V1.to_vec();
    files.push(("zz.dll", b"mod zz"));
    let _lock = lock(&env.game.join("zz.dll"));
    let err = env
        .try_apply(
            &env.stage(&files),
            &spec("1", &[], &["ue4ss"], vec![]),
            None,
        )
        .unwrap_err();
    assert!(err.is_file_in_use(), "{err}");
    assert!(!matches!(err, Error::RollbackFailed { .. }), "{err}");
    assert_same(&before, &env.game_snap());
    assert!(env.backup_snap().is_empty());
}

#[cfg(windows)]
#[test]
fn rollback_when_file_locked_on_update() {
    let (env, _before, r1) = v2_scenario();
    let mid_game = env.game_snap();
    let mid_backups = env.backup_snap();
    let mut files = V2.to_vec();
    files.push(("ue4ss/zz.dll", b"zz v2"));
    write(&env.game, "ue4ss/zz.dll", b"user file in the way");
    let mid_game = {
        let mut m = mid_game;
        m.insert(
            "ue4ss/zz.dll".into(),
            Some(b"user file in the way".to_vec()),
        );
        m
    };
    let _lock = lock(&env.game.join("ue4ss/zz.dll"));
    let err = env
        .try_apply(&env.stage(&files), &v2_spec(), Some(&r1))
        .unwrap_err();
    assert!(err.is_file_in_use(), "{err}");
    assert!(!matches!(err, Error::RollbackFailed { .. }), "{err}");
    assert_same(&mid_game, &env.game_snap());
    assert_same(&mid_backups, &env.backup_snap());
}

#[cfg(windows)]
#[test]
fn uninstall_partial_failure_then_retry() {
    let env = Env::new();
    seed_game(&env);
    let before = env.game_snap();
    let s = spec(
        "1",
        &[],
        &["ue4ss"],
        vec![line_edit("other/mods.txt", "SC", "SC : 1", None)],
    );
    let rec = env.apply(&env.stage(V1), &s, None);
    write(&env.game, "ue4ss/UE4SS.log", b"log");

    let lock_handle = lock(&env.game.join("ue4ss/UE4SS.dll"));
    let err = uninstall(&rec, &env.backups).unwrap_err();
    let rem = &err.remaining;
    assert_eq!(
        rem.files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>(),
        vec!["ue4ss/UE4SS.dll"]
    );
    assert_eq!(rem.created_dirs, vec!["ue4ss"]);
    assert!(rem.edits.is_empty());
    assert_eq!(rem.owned_dirs, vec!["ue4ss"]);
    assert!(err.message.contains("UE4SS.dll"), "{}", err.message);
    // Everything else is already back.
    assert_eq!(read(&env.game, "dwmapi.dll"), b"original dwmapi");
    assert!(!env.game.join("other/new.txt").exists());
    assert!(!env.game.join("other/mods.txt").exists());

    drop(lock_handle);
    let json = serde_json::to_string(rem).unwrap();
    let rem: InstallRecord = serde_json::from_str(&json).unwrap();
    uninstall(&rem, &env.backups).unwrap();
    assert_same(&before, &env.game_snap());
    assert!(env.backup_snap().is_empty());
}

#[test]
fn uninstall_tolerates_already_missing_files() {
    let env = Env::new();
    seed_game(&env);
    let before = env.game_snap();
    let rec = env.apply(&env.stage(V1), &spec("1", &[], &[], vec![]), None);
    fs::remove_dir_all(env.game.join("ue4ss")).unwrap();
    fs::remove_file(env.game.join("other/new.txt")).unwrap();
    uninstall(&rec, &env.backups).unwrap();
    assert_same(&before, &env.game_snap());
    // Running it twice is harmless.
    uninstall(&rec, &env.backups).unwrap();
    assert_same(&before, &env.game_snap());
}

#[test]
fn stale_backup_is_never_overwritten() {
    let env = Env::new();
    seed_game(&env);
    write(&env.backups, "dwmapi.dll", b"stale from somewhere");
    let before = env.game_snap();
    let rec = env.apply(&env.stage(V1), &spec("1", &[], &[], vec![]), None);
    assert_eq!(
        entry(&rec, "dwmapi.dll").unwrap().backup.as_deref(),
        Some("dwmapi.dll.~1")
    );
    uninstall(&rec, &env.backups).unwrap();
    assert_same(&before, &env.game_snap());
    assert_eq!(read(&env.backups, "dwmapi.dll"), b"stale from somewhere");
}

#[test]
fn progress_reports_bytes() {
    let env = Env::new();
    let staged = env.stage(V1);
    let total: u64 = V1.iter().map(|(_, b)| b.len() as u64).sum();
    let mut calls = Vec::new();
    modsync_engine::apply(
        &env.game,
        &staged,
        &spec("1", &[], &[], vec![]),
        "vein",
        None,
        &env.backups,
        &env.work,
        &mut |d, t| calls.push((d, t)),
    )
    .unwrap();
    assert_eq!(calls.first(), Some(&(0, total)));
    assert_eq!(calls.last(), Some(&(total, total)));
    assert!(calls.windows(2).all(|w| w[0].0 <= w[1].0));
}

#[test]
fn crash_mid_apply_is_recovered() {
    let env = Env::new();
    seed_game(&env);
    let before = env.game_snap();
    let staged = env.stage(V1);
    let mut n = 0;
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = modsync_engine::apply(
            &env.game,
            &staged,
            &spec("1", &[], &[], vec![]),
            "vein",
            None,
            &env.backups,
            &env.work,
            &mut |_, _| {
                n += 1;
                if n == 4 {
                    panic!("simulated crash");
                }
            },
        );
    }));
    assert!(res.is_err());
    assert!(
        env.work.join("undo.jsonl").exists(),
        "undo log survives the crash"
    );
    assert_ne!(env.game_snap(), before);

    assert!(recover(&env.work).unwrap());
    assert_same(&before, &env.game_snap());
    assert!(env.backup_snap().is_empty());
    assert!(!recover(&env.work).unwrap());
}

#[test]
fn apply_rejects_bad_inputs() {
    let env = Env::new();
    let staged = env.stage(V1);
    // Unsafe edit path.
    let s = spec(
        "1",
        &[],
        &[],
        vec![line_edit("../escape.txt", "X", "X : 1", None)],
    );
    assert!(matches!(
        env.try_apply(&staged, &s, None),
        Err(Error::UnsafePath(_))
    ));
    // Bad glob.
    assert!(matches!(
        env.try_apply(&staged, &spec("1", &["[oops"], &[], vec![]), None),
        Err(Error::Pattern { .. })
    ));
    // Missing game folder.
    let r = modsync_engine::apply(
        &env.tmp.path().join("nope"),
        &staged,
        &spec("1", &[], &[], vec![]),
        "vein",
        None,
        &env.backups,
        &env.work,
        &mut |_, _| {},
    );
    assert!(r.is_err());
    assert!(env.game_snap().is_empty());
}

#[test]
fn package_spec_deserializes_from_catalog_json() {
    let json = r#"{
      "version": "0.5.0", "url": "packages/vein/vein-0.5.0.zip", "sha256": "abc", "size": 123,
      "released": "2026-10-26", "mods": [{ "name": "Shared Crafting" }], "notes": ["x"],
      "preserve": ["ue4ss/Mods/mods.txt", "**/config_local.lua"],
      "ownedDirs": ["ue4ss"],
      "edits": [{ "type": "line", "file": "ue4ss/Mods/mods.txt", "key": "SharedCraftingClient", "line": "SharedCraftingClient : 1", "before": "Keybinds" }]
    }"#;
    let s: modsync_engine::PackageSpec = serde_json::from_str(json).unwrap();
    assert_eq!(s.owned_dirs, vec!["ue4ss"]);
    assert_eq!(
        s.edits[0],
        line_edit(
            "ue4ss/Mods/mods.txt",
            "SharedCraftingClient",
            "SharedCraftingClient : 1",
            Some("Keybinds")
        )
    );
    let minimal: modsync_engine::PackageSpec = serde_json::from_str(r#"{"version":"1"}"#).unwrap();
    assert!(minimal.preserve.is_empty() && minimal.edits.is_empty());
}
