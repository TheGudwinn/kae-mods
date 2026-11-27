use deadlock_mm_lib::install::error::InstallError;
use deadlock_mm_lib::install::extract::Limits;
use deadlock_mm_lib::install::installer::{install_mod, plan_reorder, reorder_files, set_enabled, update_files, Ctx, ReorderItem};
use deadlock_mm_lib::install::journal::Journal;
use deadlock_mm_lib::install::vpk::build_vpk;
use deadlock_mm_lib::steam::locate::validate_game_dir;
use std::io::Write;
use std::path::{Path, PathBuf};

const GAMEINFO: &str = "GameInfo\r\n{\r\n\tSearchPaths\r\n\t{\r\n\t\tMod                 citadel\r\n\t\tWrite               citadel\r\n\t\tGame                citadel\r\n\t\tGame                core\r\n\t}\r\n}\r\n";

struct Env {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    staging: PathBuf,
    backups: PathBuf,
    journal: Journal,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Deadlock");
    let citadel = root.join("game/citadel");
    std::fs::create_dir_all(&citadel).unwrap();
    std::fs::write(citadel.join("gameinfo.gi"), GAMEINFO).unwrap();
    Env {
        staging: tmp.path().join("staging"),
        backups: tmp.path().join("backups"),
        journal: Journal::open(tmp.path().join("journal.jsonl")),
        root,
        _tmp: tmp,
    }
}

fn zip_of(path: &Path, files: &[(&str, Vec<u8>)]) {
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (n, d) in files {
        z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(d).unwrap();
    }
    z.finish().unwrap();
}

fn ctx<'a>(e: &'a Env, game: &'a deadlock_mm_lib::steam::locate::GameInstall) -> Ctx<'a> {
    Ctx { game, staging: &e.staging, backups: &e.backups, journal: &e.journal, limits: Limits::default() }
}

#[test]
fn install_toggle_and_rollback() {
    let e = env();
    let game = validate_game_dir(&e.root).unwrap();
    let zip = e._tmp.path().join("mod.zip");
    zip_of(&zip, &[("readme.txt", b"hi".to_vec()), ("sub/pak01_dir.vpk", build_vpk(&[("models/x.vmdl_c", b"data")]))]);

    let c = ctx(&e, &game);
    let out = install_mod(&c, &zip, 42, 1).unwrap();
    assert!(out.gameinfo_patched);
    assert_eq!(out.files.len(), 1);
    assert_eq!(out.files[0].addon_name, "pak01_dir.vpk");
    assert_eq!(out.files[0].entries[0].path, "models/x.vmdl_c");
    assert!(game.addons_dir.join("pak01_dir.vpk").is_file());
    assert!(std::fs::read_to_string(&game.gameinfo).unwrap().contains("citadel/addons"));
    assert!(std::fs::read_dir(&e.staging).unwrap().next().is_none(), "staging cleaned");

    // Disable -> file is no longer *_dir.vpk; enable -> back.
    set_enabled(&c, "pak01_dir.vpk", false).unwrap();
    assert!(!game.addons_dir.join("pak01_dir.vpk").exists());
    assert!(game.addons_dir.join("pak01_dir.vpk.disabled").exists());
    set_enabled(&c, "pak01_dir.vpk", true).unwrap();
    assert!(game.addons_dir.join("pak01_dir.vpk").exists());

    // One call undoes the whole installation, including the gameinfo.gi edit.
    e.journal.rollback_batch(&out.batch).unwrap();
    assert!(!game.addons_dir.join("pak01_dir.vpk").exists());
    assert_eq!(std::fs::read_to_string(&game.gameinfo).unwrap(), GAMEINFO);
    // Rolling back twice is harmless.
    assert_eq!(e.journal.rollback_batch(&out.batch).unwrap(), 0);
}

#[test]
fn bare_vpk_install_and_duplicate_is_refused() {
    let e = env();
    let game = validate_game_dir(&e.root).unwrap();
    let vpk = e._tmp.path().join("pak05_dir.vpk");
    std::fs::write(&vpk, build_vpk(&[("a.txt", b"1")])).unwrap();
    let c = ctx(&e, &game);
    install_mod(&c, &vpk, 7, 3).unwrap();
    assert!(game.addons_dir.join("pak03_dir.vpk").exists());
    let err = install_mod(&c, &vpk, 7, 3).unwrap_err();
    assert!(matches!(err, InstallError::AlreadyInstalled(_)));
    // The failed attempt must not have removed the first install.
    assert!(game.addons_dir.join("pak03_dir.vpk").exists());
}

fn expect_rejected(files: &[(&str, Vec<u8>)], limits: Option<Limits>) -> InstallError {
    let e = env();
    let game = validate_game_dir(&e.root).unwrap();
    let zip = e._tmp.path().join("bad.zip");
    zip_of(&zip, files);
    let mut c = ctx(&e, &game);
    if let Some(l) = limits {
        c.limits = l;
    }
    let err = install_mod(&c, &zip, 1, 1).unwrap_err();
    // Nothing may have changed in the game folder.
    assert_eq!(std::fs::read_to_string(&game.gameinfo).unwrap(), GAMEINFO);
    assert!(!game.addons_dir.exists());
    err
}

#[test]
fn rejects_zip_slip() {
    let v = build_vpk(&[("a", b"1")]);
    assert!(matches!(expect_rejected(&[("../evil_dir.vpk", v.clone())], None), InstallError::UnsafePath(_)));
    assert!(matches!(expect_rejected(&[("..\\evil_dir.vpk", v.clone())], None), InstallError::UnsafePath(_)));
    assert!(matches!(expect_rejected(&[("C:/x/evil_dir.vpk", v)], None), InstallError::UnsafePath(_)));
}

#[test]
fn rejects_executables_and_empty_and_oversized() {
    let v = build_vpk(&[("a", b"1")]);
    assert!(matches!(expect_rejected(&[("ok_dir.vpk", v.clone()), ("run.exe", vec![1])], None), InstallError::ForbiddenFile(_)));
    assert!(matches!(expect_rejected(&[("readme.txt", vec![1])], None), InstallError::NoVpk));
    let small = Limits { max_entries: 5_000, max_total_bytes: 100, max_file_bytes: 50 };
    assert!(matches!(expect_rejected(&[("big_dir.vpk", v)], Some(small)), InstallError::TooLarge));
    let few = Limits { max_entries: 2, ..Limits::default() };
    let many: Vec<(&str, Vec<u8>)> = vec![("a.txt", vec![]), ("b.txt", vec![]), ("c.txt", vec![])];
    assert!(matches!(expect_rejected(&many, Some(few)), InstallError::TooManyEntries(3)));
}

#[test]
fn rejects_invalid_vpk() {
    assert!(matches!(expect_rejected(&[("x_dir.vpk", b"garbage garbage garbage".to_vec())], None), InstallError::BadVpk(..)));
}

#[test]
fn refuses_to_edit_unrecognized_gameinfo() {
    let e = env();
    std::fs::write(e.root.join("game/citadel/gameinfo.gi"), "totally different format").unwrap();
    let game = validate_game_dir(&e.root).unwrap();
    let vpk = e._tmp.path().join("p_dir.vpk");
    std::fs::write(&vpk, build_vpk(&[("a", b"1")])).unwrap();
    let err = install_mod(&ctx(&e, &game), &vpk, 1, 1).unwrap_err();
    assert!(matches!(err, InstallError::GameinfoUnrecognized));
    assert!(!game.addons_dir.exists());
}

#[test]
fn detects_already_installed_mods_from_other_managers() {
    let e = env();
    let game = validate_game_dir(&e.root).unwrap();
    std::fs::create_dir_all(&game.addons_dir).unwrap();
    // Files as left by another manager: prefixed VPK + a manifest that must be ignored.
    let foreign = build_vpk(&[("models/x.vmdl_c", b"AAA"), ("sounds/s.vsnd_c", b"BBB")]);
    std::fs::write(game.addons_dir.join("659083_pak67_dir.vpk"), &foreign).unwrap();
    std::fs::write(game.addons_dir.join(".dmm.json"), "{}").unwrap();

    let scanned = deadlock_mm_lib::library::scan::scan_addons(&game.addons_dir).unwrap();
    assert_eq!(scanned.len(), 1);
    assert!(!scanned[0].ours && scanned[0].entries.is_ok());

    let c = ctx(&e, &game);
    // Exact same content, different name -> refused as a duplicate.
    let dup = e._tmp.path().join("same_dir.vpk");
    std::fs::write(&dup, &foreign).unwrap();
    let err = install_mod(&c, &dup, 9, 1).unwrap_err();
    assert!(matches!(&err, InstallError::AlreadyInstalled(n) if n == "659083_pak67_dir.vpk"));
    assert_eq!(std::fs::read_to_string(&game.gameinfo).unwrap(), GAMEINFO, "refusal happens before any edit");

    // Same path, different content -> installs, but the overlap is reported.
    let other = e._tmp.path().join("other_dir.vpk");
    std::fs::write(&other, build_vpk(&[("models/x.vmdl_c", b"ZZZ"), ("new.txt", b"n")])).unwrap();
    let out = install_mod(&c, &other, 9, 1).unwrap();
    assert_eq!(out.overlaps.len(), 1);
    assert_eq!(out.overlaps[0].path, "models/x.vmdl_c");
    assert_eq!(out.overlaps[0].with_addon, "659083_pak67_dir.vpk");
    assert!(!out.overlaps[0].identical);
    // Foreign file untouched.
    assert_eq!(std::fs::read(game.addons_dir.join("659083_pak67_dir.vpk")).unwrap(), foreign);
}

#[test]
fn uninstall_keeps_backup_and_can_be_rolled_back() {
    use deadlock_mm_lib::install::installer::uninstall_files;
    let e = env();
    let game = validate_game_dir(&e.root).unwrap();
    let vpk = e._tmp.path().join("m_dir.vpk");
    std::fs::write(&vpk, build_vpk(&[("a.txt", b"payload")])).unwrap();
    let c = ctx(&e, &game);
    let out = install_mod(&c, &vpk, 5, 1).unwrap();
    let name = out.files[0].addon_name.clone();
    let original = std::fs::read(game.addons_dir.join(&name)).unwrap();

    // Works for a disabled file too.
    set_enabled(&c, &name, false).unwrap();
    let batch = uninstall_files(&c, 5, &[name.clone()]).unwrap();
    assert!(batch.starts_with("uninstall-5-"));
    assert!(!game.addons_dir.join(&name).exists() && !game.addons_dir.join(format!("{name}.disabled")).exists());

    e.journal.rollback_batch(&batch).unwrap();
    assert_eq!(std::fs::read(game.addons_dir.join(format!("{name}.disabled"))).unwrap(), original);
}

fn sevenz_of(path: &Path, files: &[(&str, Vec<u8>)]) {
    let src = path.with_extension("src");
    std::fs::create_dir_all(&src).unwrap();
    for (n, d) in files {
        std::fs::write(src.join(n), d).unwrap();
    }
    sevenz_rust2::compress_to_path(&src, path).unwrap();
}

#[test]
fn installs_from_7z_regardless_of_extension() {
    let e = env();
    let game = validate_game_dir(&e.root).unwrap();
    let archive = e._tmp.path().join("mod.dat"); // wrong extension on purpose
    sevenz_of(&archive, &[("readme.txt", b"hi".to_vec()), ("pak01_dir.vpk", build_vpk(&[("models/a.vmdl_c", b"7z-data")]))]);
    let out = install_mod(&ctx(&e, &game), &archive, 11, 1).unwrap();
    assert_eq!(out.files[0].addon_name, "pak01_dir.vpk");
    assert_eq!(out.files[0].entries[0].path, "models/a.vmdl_c");
}

#[test]
fn rejects_executables_inside_7z_and_unknown_formats() {
    let e = env();
    let game = validate_game_dir(&e.root).unwrap();
    let a = e._tmp.path().join("bad.7z");
    sevenz_of(&a, &[("ok_dir.vpk", build_vpk(&[("a", b"1")])), ("setup.exe", vec![1, 2, 3])]);
    assert!(matches!(install_mod(&ctx(&e, &game), &a, 1, 1).unwrap_err(), InstallError::ForbiddenFile(_)));
    assert!(!game.addons_dir.exists());

    let junk = e._tmp.path().join("junk.rar"); // .rar name, not a rar inside
    std::fs::write(&junk, b"this is not an archive").unwrap();
    assert!(matches!(install_mod(&ctx(&e, &game), &junk, 2, 1).unwrap_err(), InstallError::BadArchive(_)));
}

#[test]
fn zip_named_rar_is_still_extracted() {
    let e = env();
    let game = validate_game_dir(&e.root).unwrap();
    let z = e._tmp.path().join("actually_zip.rar");
    zip_of(&z, &[("p_dir.vpk", build_vpk(&[("x.txt", b"z")]))]);
    install_mod(&ctx(&e, &game), &z, 3, 1).unwrap();
}

#[test]
fn profile_switch_is_one_undoable_batch_and_rolls_back_on_failure() {
    use deadlock_mm_lib::install::installer::toggle_many;
    let e = env();
    let game = validate_game_dir(&e.root).unwrap();
    let c = ctx(&e, &game);
    let mut names = Vec::new();
    for (id, data) in [(1i64, b"a" as &[u8]), (2, b"b"), (3, b"c")] {
        let v = e._tmp.path().join(format!("m{id}_dir.vpk"));
        std::fs::write(&v, build_vpk(&[("f.txt", data)])).unwrap();
        names.push(install_mod(&c, &v, id, id as u8).unwrap().files[0].addon_name.clone());
    }
    let on = |n: &str| game.addons_dir.join(n).exists();

    // Profile: 1 off, 2 stays on, 3 off. One batch.
    let batch = toggle_many(&c, "7", &[(vec![names[0].clone()], false), (vec![names[2].clone()], false)]).unwrap();
    assert!(batch.starts_with("profile-7-"));
    assert!(!on(&names[0]) && on(&names[1]) && !on(&names[2]));
    e.journal.rollback_batch(&batch).unwrap();
    assert!(on(&names[0]) && on(&names[1]) && on(&names[2]), "one undo restores all");

    // A conflict in the middle (target name already taken) must leave everything as it was.
    std::fs::write(game.addons_dir.join(format!("{}.disabled", names[2])), b"squatter").unwrap();
    let err = toggle_many(&c, "8", &[(vec![names[0].clone()], false), (vec![names[2].clone()], false)]).unwrap_err();
    assert!(matches!(err, InstallError::AlreadyExists(_)));
    assert!(on(&names[0]) && on(&names[1]) && on(&names[2]), "failed switch is rolled back");
}

fn put(game: &deadlock_mm_lib::steam::locate::GameInstall, name: &str, body: &[u8]) {
    std::fs::create_dir_all(&game.addons_dir).unwrap();
    std::fs::write(game.addons_dir.join(name), body).unwrap();
}

fn item(id: i64, files: &[&str]) -> ReorderItem {
    ReorderItem { mod_id: id, files: files.iter().map(|s| s.to_string()).collect() }
}

fn read(game: &deadlock_mm_lib::steam::locate::GameInstall, name: &str) -> Option<String> {
    std::fs::read_to_string(game.addons_dir.join(name)).ok()
}

#[test]
fn reorder_swaps_slots_keeps_disabled_state_foreign_files_and_undoes() {
    let e = env();
    let game = validate_game_dir(&e.root).unwrap();
    // Mod 1 owns slots 1 and 2 (two files), mod 2 slot 4 (disabled), foreign addon sits in slot 3.
    put(&game, "pak01_dir.vpk", b"a1");
    put(&game, "pak02_dir.vpk", b"a2");
    put(&game, "pak04_dir.vpk.disabled", b"b");
    put(&game, "555_pak03_dir.vpk", b"foreign");
    let c = ctx(&e, &game);

    let order = [item(2, &["pak04_dir.vpk"]), item(1, &["pak01_dir.vpk", "pak02_dir.vpk"])];
    let plan = plan_reorder(&order).unwrap();
    assert_eq!(plan.len(), 3);

    let batch = reorder_files(&c, &order).unwrap().expect("something moved");
    // Occupied slots {1,2,4} are reassigned in the new order: mod 2 -> 1, mod 1 -> 2 and 4.
    assert_eq!(read(&game, "pak01_dir.vpk.disabled").as_deref(), Some("b"), "disabled state kept");
    assert_eq!(read(&game, "pak02_dir.vpk").as_deref(), Some("a1"));
    assert_eq!(read(&game, "pak04_dir.vpk").as_deref(), Some("a2"), "file order inside a mod is kept");
    assert_eq!(read(&game, "555_pak03_dir.vpk").as_deref(), Some("foreign"));
    assert!(std::fs::read_dir(&game.addons_dir).unwrap().flatten().all(|f| !f.file_name().to_string_lossy().ends_with(".ddmm-reorder")));

    e.journal.rollback_batch(&batch).unwrap();
    assert_eq!(read(&game, "pak01_dir.vpk").as_deref(), Some("a1"));
    assert_eq!(read(&game, "pak02_dir.vpk").as_deref(), Some("a2"));
    assert_eq!(read(&game, "pak04_dir.vpk.disabled").as_deref(), Some("b"));
    assert!(read(&game, "pak01_dir.vpk.disabled").is_none());
}

#[test]
fn reorder_in_place_is_a_no_op_and_failure_restores_everything() {
    let e = env();
    let game = validate_game_dir(&e.root).unwrap();
    put(&game, "pak01_dir.vpk", b"a");
    put(&game, "pak02_dir.vpk", b"b");
    let c = ctx(&e, &game);

    let same = [item(1, &["pak01_dir.vpk"]), item(2, &["pak02_dir.vpk"])];
    assert!(reorder_files(&c, &same).unwrap().is_none());

    // Mod 3 is listed but its file is gone: the rename of mod 2 done before the failure must be rolled back.
    let broken = [item(2, &["pak02_dir.vpk"]), item(3, &["pak03_dir.vpk"]), item(1, &["pak01_dir.vpk"])];
    assert!(reorder_files(&c, &broken).is_err());
    assert_eq!(read(&game, "pak01_dir.vpk").as_deref(), Some("a"));
    assert_eq!(read(&game, "pak02_dir.vpk").as_deref(), Some("b"));
}

#[test]
fn reorder_rejects_foreign_names_and_shared_slots() {
    assert!(plan_reorder(&[item(1, &["659083_pak67_dir.vpk"])]).is_err());
    assert!(plan_reorder(&[item(1, &["pak01_dir.vpk"]), item(2, &["pak01_dir.vpk"])]).is_err());
}

#[test]
fn update_swaps_files_in_one_batch_keeping_slot_and_disabled_state() {
    let e = env();
    let game = validate_game_dir(&e.root).unwrap();
    let c = ctx(&e, &game);
    let v1 = build_vpk(&[("models/a.vmdl_c", b"version one")]);
    let v2 = build_vpk(&[("models/a.vmdl_c", b"version two, different")]);
    let z1 = e._tmp.path().join("v1.zip");
    let z2 = e._tmp.path().join("v2.zip");
    zip_of(&z1, &[("pak01_dir.vpk", v1.clone())]);
    zip_of(&z2, &[("pak01_dir.vpk", v2.clone())]);

    install_mod(&c, &z1, 7, 3).unwrap();
    set_enabled(&c, "pak03_dir.vpk", false).unwrap();

    let out = update_files(&c, &z2, 7, &["pak03_dir.vpk".to_string()], 3).unwrap();
    assert!(out.batch.starts_with("update-7-"));
    assert_eq!(std::fs::read(game.addons_dir.join("pak03_dir.vpk.disabled")).unwrap(), v2, "new content, still disabled");
    assert!(!game.addons_dir.join("pak03_dir.vpk").exists());

    // One rollback brings the previous version back, still disabled.
    e.journal.rollback_batch(&out.batch).unwrap();
    assert_eq!(std::fs::read(game.addons_dir.join("pak03_dir.vpk.disabled")).unwrap(), v1);
}

#[test]
fn failed_update_restores_the_old_version() {
    let e = env();
    let game = validate_game_dir(&e.root).unwrap();
    let c = ctx(&e, &game);
    let v1 = build_vpk(&[("models/a.vmdl_c", b"version one")]);
    let z1 = e._tmp.path().join("v1.zip");
    zip_of(&z1, &[("pak01_dir.vpk", v1.clone())]);
    install_mod(&c, &z1, 7, 3).unwrap();
    put(&game, "pak04_dir.vpk", b"someone else in slot 4");

    // The new version ships two VPKs, so it would need slot 4, which is taken.
    let z2 = e._tmp.path().join("v2.zip");
    zip_of(
        &z2,
        &[("a/pak01_dir.vpk", build_vpk(&[("x/1", b"1")])), ("b/pak01_dir.vpk", build_vpk(&[("x/2", b"2")]))],
    );
    let err = update_files(&c, &z2, 7, &["pak03_dir.vpk".to_string()], 3).unwrap_err();
    assert!(matches!(err, InstallError::AlreadyExists(_)), "{err:?}");
    assert_eq!(std::fs::read(game.addons_dir.join("pak03_dir.vpk")).unwrap(), v1, "old file restored and enabled");
    assert_eq!(read(&game, "pak04_dir.vpk").as_deref(), Some("someone else in slot 4"));

    // A corrupt archive fails before anything is lost.
    let bad = e._tmp.path().join("bad.zip");
    zip_of(&bad, &[("pak01_dir.vpk", b"not a vpk".to_vec())]);
    assert!(update_files(&c, &bad, 7, &["pak03_dir.vpk".to_string()], 3).is_err());
    assert_eq!(std::fs::read(game.addons_dir.join("pak03_dir.vpk")).unwrap(), v1);
}

#[test]
fn conflict_count_ignores_identical_copies_and_disabled_addons() {
    use deadlock_mm_lib::library::scan::{real_conflicts, scan_addons};
    let e = env();
    let game = validate_game_dir(&e.root).unwrap();
    let a = build_vpk(&[("models/shared.vmdl_c", b"from A"), ("only/a.txt", b"a")]);
    let b = build_vpk(&[("models/shared.vmdl_c", b"from B, different"), ("only/b.txt", b"b")]);
    let same = build_vpk(&[("models/shared.vmdl_c", b"from A")]);
    std::fs::create_dir_all(&game.addons_dir).unwrap();
    std::fs::write(game.addons_dir.join("ddmm1_pak01_dir.vpk"), &a).unwrap();
    std::fs::write(game.addons_dir.join("ddmm2_pak02_dir.vpk"), &b).unwrap();
    assert_eq!(real_conflicts(&scan_addons(&game.addons_dir).unwrap()), 1, "different content in the same path");

    // Disabling one side resolves it.
    std::fs::rename(game.addons_dir.join("ddmm2_pak02_dir.vpk"), game.addons_dir.join("ddmm2_pak02_dir.vpk.disabled")).unwrap();
    assert_eq!(real_conflicts(&scan_addons(&game.addons_dir).unwrap()), 0);

    // An identical copy is harmless.
    std::fs::write(game.addons_dir.join("555_pak03_dir.vpk"), &same).unwrap();
    assert_eq!(real_conflicts(&scan_addons(&game.addons_dir).unwrap()), 0);

    // Another manager's switched-off mod (id prefix) is not loaded by the game, so it cannot conflict,
    // even with different content; the same content under a loaded name (pak##_dir.vpk) does.
    std::fs::write(game.addons_dir.join("666_pak04_dir.vpk"), &b).unwrap();
    assert_eq!(real_conflicts(&scan_addons(&game.addons_dir).unwrap()), 0, "switched-off foreign mod is ignored");
    std::fs::write(game.addons_dir.join("pak07_dir.vpk"), &b).unwrap();
    assert_eq!(real_conflicts(&scan_addons(&game.addons_dir).unwrap()), 1, "a loaded foreign file counts");
}

#[test]
fn foreign_mods_can_be_switched_off_and_on_and_undone() {
    use deadlock_mm_lib::install::foreign::toggle_foreign;
    let e = env();
    let game = validate_game_dir(&e.root).unwrap();
    let c = ctx(&e, &game);
    let dir = &game.addons_dir;
    std::fs::create_dir_all(dir).unwrap();
    let v = build_vpk(&[("models/a.vmdl_c", b"A")]);
    std::fs::write(dir.join("pak01_dir.vpk"), &v).unwrap();
    std::fs::write(dir.join("pak02_dir.vpk"), &v).unwrap();
    let names = |d: &Path| {
        let mut n: Vec<String> = std::fs::read_dir(d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        n.sort();
        n
    };

    // Off: the catalog id goes in front of the name, which the game does not load.
    let batch = toggle_foreign(&c, Some("650634"), &["pak01_dir.vpk".into(), "pak02_dir.vpk".into()], false).unwrap();
    assert_eq!(names(dir), ["650634_pak01_dir.vpk", "650634_pak02_dir.vpk"]);

    // Undo restores the exact names.
    e.journal.rollback_batch(&batch).unwrap();
    assert_eq!(names(dir), ["pak01_dir.vpk", "pak02_dir.vpk"]);

    // Off, then another mod takes pak01: switching back on picks the lowest free slot.
    toggle_foreign(&c, Some("650634"), &["pak01_dir.vpk".into()], false).unwrap();
    std::fs::write(dir.join("pak01_dir.vpk"), &v).unwrap();
    toggle_foreign(&c, Some("650634"), &["650634_pak01_dir.vpk".into()], true).unwrap();
    assert_eq!(names(dir), ["pak01_dir.vpk", "pak02_dir.vpk", "pak03_dir.vpk"]);

    // Without a catalog id the .disabled suffix is used and removed again.
    toggle_foreign(&c, None, &["pak03_dir.vpk".into()], false).unwrap();
    assert!(dir.join("pak03_dir.vpk.disabled").exists());
    toggle_foreign(&c, None, &["pak03_dir.vpk.disabled".into()], true).unwrap();
    assert!(dir.join("pak03_dir.vpk").exists());

    // Refusals: this manager's own files, paths, missing files, and mods split over chunk files.
    std::fs::write(dir.join("ddmm1_pak05_dir.vpk"), &v).unwrap();
    assert!(toggle_foreign(&c, None, &["ddmm1_pak05_dir.vpk".into()], false).is_err());
    assert!(toggle_foreign(&c, None, &["../gameinfo.gi".into()], false).is_err());
    assert!(toggle_foreign(&c, None, &["pak99_dir.vpk".into()], false).is_err());
    std::fs::write(dir.join("pak09_dir.vpk"), &v).unwrap();
    std::fs::write(dir.join("pak09_000.vpk"), b"chunk").unwrap();
    assert!(matches!(toggle_foreign(&c, None, &["pak09_dir.vpk".into()], false), Err(InstallError::ExternalChunks(_))));
    assert!(dir.join("pak09_dir.vpk").exists(), "nothing was renamed");
}
