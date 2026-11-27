use super::error::InstallError;
use super::extract::{detect_kind, extract_vpks, ArchiveKind, Limits};
use super::gameinfo::{ensure_addons_path, write_atomic, PatchResult};
use super::journal::{ensure_within, Journal, Op};
use super::pak_order::{addon_name, disabled_name, is_legacy, slot_of};
use super::vpk::{read_vpk_index, VpkFile};
use crate::library::scan::{find_duplicate, overlaps, scan_addons, Overlap};
use crate::steam::locate::GameInstall;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub struct Ctx<'a> {
    pub game: &'a GameInstall,
    /// Scratch space for extraction (deleted after install).
    pub staging: &'a Path,
    /// Where originals of modified game files are kept.
    pub backups: &'a Path,
    pub journal: &'a Journal,
    pub limits: Limits,
}

#[derive(Debug)]
pub struct InstalledFile {
    pub addon_name: String,
    pub path: PathBuf,
    pub sha256: String,
    pub size: u64,
    pub entries: Vec<VpkFile>,
}

#[derive(Debug)]
pub struct InstallOutcome {
    /// Pass to `Journal::rollback_batch` to undo this whole installation.
    pub batch: String,
    pub files: Vec<InstalledFile>,
    pub gameinfo_patched: bool,
    /// Files that also exist in addons installed earlier (by us or other managers).
    pub overlaps: Vec<Overlap>,
}

fn now() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
}

fn sha256_of(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = std::io::Read::read(&mut f, &mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// Install a mod from a `.zip`, `.7z`, `.rar` or bare `.vpk` into the game's addons directory.
/// The format is detected from the file contents, not the extension.
/// VPKs get slots `first_slot`, `first_slot + 1`, ...
pub fn install_mod(ctx: &Ctx, archive: &Path, mod_id: i64, first_slot: u8) -> Result<InstallOutcome, InstallError> {
    let batch = format!("install-{mod_id}-{}", now());
    install_in_batch(ctx, archive, mod_id, first_slot, &batch)
}

/// Same as `install_mod` but records into a caller-chosen journal batch. On failure the WHOLE batch
/// is rolled back, including anything the caller recorded earlier under the same id.
fn install_in_batch(ctx: &Ctx, archive: &Path, mod_id: i64, first_slot: u8, batch: &str) -> Result<InstallOutcome, InstallError> {
    let batch = batch.to_string();
    let stage = ctx.staging.join(format!("{mod_id}-{}", now()));

    let result = (|| {
        // 1. Get VPKs into staging (archives are extracted safely; a bare .vpk is just copied).
        let vpks: Vec<PathBuf> = if detect_kind(archive)? == ArchiveKind::Vpk {
            std::fs::create_dir_all(&stage)?;
            let dest = stage.join(archive.file_name().unwrap());
            std::fs::copy(archive, &dest)?;
            vec![dest]
        } else {
            extract_vpks(archive, &stage, &ctx.limits)?
        };

        // 2. Validate every VPK before touching the game folder.
        let mut parsed = Vec::new();
        for v in &vpks {
            let name = v.file_name().unwrap().to_string_lossy().into_owned();
            let info = read_vpk_index(v).map_err(|e| InstallError::BadVpk(name.clone(), e))?;
            if info.external_archives {
                return Err(InstallError::ExternalChunks(name));
            }
            parsed.push((v.clone(), info));
        }
        if (first_slot as usize) + parsed.len() - 1 > super::pak_order::MAX_SLOTS {
            return Err(InstallError::TooManySlots(super::pak_order::MAX_SLOTS));
        }

        // 2b. Check what is already installed: refuse exact duplicates, report overlaps.
        let existing = scan_addons(&ctx.game.addons_dir)?;
        let mut all_overlaps = Vec::new();
        for (_, info) in &parsed {
            if let Some(dup) = find_duplicate(&info.files, &existing) {
                return Err(InstallError::AlreadyInstalled(dup.file_name.clone()));
            }
            all_overlaps.extend(overlaps(&info.files, &existing));
        }

        // 2c. Two of our files must never share a slot (names differ by mod id, so the name check
        // below would not notice): a multi-VPK mod could otherwise land on a slot another mod uses.
        for i in 0..parsed.len() {
            let slot = first_slot + i as u8;
            if let Some(other) = existing.iter().find(|a| slot_of(&a.file_name) == Some(slot)) {
                return Err(InstallError::AlreadyExists(ctx.game.addons_dir.join(&other.file_name)));
            }
        }

        // 3. Make sure the game reads addons/ (backup + journal before editing gameinfo.gi).
        let gi_bytes = std::fs::read(&ctx.game.gameinfo)?;
        // A file that is not valid UTF-8 is not something we can edit safely.
        let gi_text = String::from_utf8(gi_bytes.clone()).map_err(|_| InstallError::GameinfoUnrecognized)?;
        let patched = match ensure_addons_path(&gi_text) {
            PatchResult::AlreadyPresent => false,
            PatchResult::Unrecognized => return Err(InstallError::GameinfoUnrecognized),
            PatchResult::Patched(new_text) => {
                // The patched text must itself be recognized as complete before it touches the disk.
                if ensure_addons_path(&new_text) != PatchResult::AlreadyPresent {
                    return Err(InstallError::GameinfoUnrecognized);
                }
                std::fs::create_dir_all(ctx.backups)?;
                let backup = ctx.backups.join(format!("{batch}_gameinfo.gi"));
                // Backup first, flushed to disk and verified byte for byte, then journaled.
                write_atomic(&backup, &gi_bytes)?;
                if std::fs::read(&backup)? != gi_bytes {
                    return Err(InstallError::Journal("gameinfo.gi backup does not match the original".into()));
                }
                ctx.journal.record(&batch, Op::ReplaceFile { path: ctx.game.gameinfo.clone(), backup })?;
                write_atomic(&ctx.game.gameinfo, new_text.as_bytes())?;
                true
            }
        };

        // 4. Copy VPKs into addons/ under our naming scheme.
        if !ctx.game.addons_dir.exists() {
            ctx.journal.record(&batch, Op::CreateDir { path: ctx.game.addons_dir.clone() })?;
            std::fs::create_dir_all(&ctx.game.addons_dir)?;
        }
        let mut files = Vec::new();
        for (i, (src, info)) in parsed.into_iter().enumerate() {
            let name = addon_name(mod_id, first_slot + i as u8);
            let dest = ctx.game.addons_dir.join(&name);
            ensure_within(&ctx.game.addons_dir, &dest)?;
            if dest.exists() || ctx.game.addons_dir.join(disabled_name(&name)).exists() {
                return Err(InstallError::AlreadyExists(dest));
            }
            ctx.journal.record(&batch, Op::CreateFile { path: dest.clone() })?;
            std::fs::copy(&src, &dest)?;
            files.push(InstalledFile {
                addon_name: name,
                sha256: sha256_of(&dest)?,
                size: std::fs::metadata(&dest)?.len(),
                path: dest,
                entries: info.files,
            });
        }
        Ok(InstallOutcome { batch: batch.clone(), files, gameinfo_patched: patched, overlaps: all_overlaps })
    })();

    let _ = std::fs::remove_dir_all(&stage);
    if result.is_err() {
        // Leave the game folder exactly as it was before this attempt.
        let _ = ctx.journal.rollback_batch(&batch);
    }
    result
}

/// Delete a mod's addon files, keeping a backup of each so the action can be rolled back.
/// Returns the journal batch id (`uninstall-<mod_id>-<ts>`).
pub fn uninstall_files(ctx: &Ctx, mod_id: i64, addon_names: &[String]) -> Result<String, InstallError> {
    let batch = format!("uninstall-{mod_id}-{}", now());
    uninstall_in_batch(ctx, &batch, addon_names)?;
    Ok(batch)
}

fn uninstall_in_batch(ctx: &Ctx, batch: &str, addon_names: &[String]) -> Result<(), InstallError> {
    std::fs::create_dir_all(ctx.backups)?;
    for name in addon_names {
        for candidate in [name.clone(), disabled_name(name)] {
            let path = ctx.game.addons_dir.join(&candidate);
            ensure_within(&ctx.game.addons_dir, &path)?;
            if !path.is_file() {
                continue;
            }
            let backup = ctx.backups.join(format!("{batch}_{candidate}"));
            std::fs::copy(&path, &backup)?;
            ctx.journal.record(batch, Op::DeleteFile { path: path.clone(), backup })?;
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// Enable or disable all files of one mod under a single journal batch (`toggle-<mod_id>-<ts>`).
pub fn toggle_mod(ctx: &Ctx, mod_id: i64, addons: &[String], enabled: bool) -> Result<String, InstallError> {
    let batch = format!("toggle-{mod_id}-{}", now());
    for addon in addons {
        let on = ctx.game.addons_dir.join(addon);
        let off = ctx.game.addons_dir.join(disabled_name(addon));
        ensure_within(&ctx.game.addons_dir, &on)?;
        let (from, to) = if enabled { (off, on) } else { (on, off) };
        if !from.exists() {
            continue;
        }
        if to.exists() {
            return Err(InstallError::AlreadyExists(to));
        }
        ctx.journal.record(&batch, Op::Rename { from: from.clone(), to: to.clone() })?;
        std::fs::rename(&from, &to)?;
    }
    Ok(batch)
}

/// Apply several enable/disable changes under ONE journal batch (`profile-<label>-<ts>`) so a
/// profile switch can be undone with a single action. On failure everything done so far is undone.
pub fn toggle_many(ctx: &Ctx, label: &str, items: &[(Vec<String>, bool)]) -> Result<String, InstallError> {
    let batch = format!("profile-{label}-{}", now());
    let result = (|| {
        for (addons, enabled) in items {
            for addon in addons {
                let on = ctx.game.addons_dir.join(addon);
                let off = ctx.game.addons_dir.join(disabled_name(addon));
                ensure_within(&ctx.game.addons_dir, &on)?;
                let (from, to) = if *enabled { (off, on) } else { (on, off) };
                if !from.exists() {
                    continue;
                }
                if to.exists() {
                    return Err(InstallError::AlreadyExists(to));
                }
                ctx.journal.record(&batch, Op::Rename { from: from.clone(), to: to.clone() })?;
                std::fs::rename(&from, &to)?;
            }
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = ctx.journal.rollback_batch(&batch);
    }
    result.map(|_| batch)
}

/// Enable or disable one addon file by renaming it (the game only loads `*_dir.vpk`).
/// Returns the journal batch id for undo.
pub fn set_enabled(ctx: &Ctx, addon: &str, enabled: bool) -> Result<String, InstallError> {
    let on = ctx.game.addons_dir.join(addon);
    let off = ctx.game.addons_dir.join(disabled_name(addon));
    ensure_within(&ctx.game.addons_dir, &on)?;
    let (from, to) = if enabled { (off, on) } else { (on, off) };
    if !from.exists() {
        return Ok(String::new()); // already in the requested state
    }
    if to.exists() {
        return Err(InstallError::AlreadyExists(to));
    }
    let batch = format!("toggle-{addon}-{}", now());
    ctx.journal.record(&batch, Op::Rename { from: from.clone(), to: to.clone() })?;
    std::fs::rename(&from, &to)?;
    Ok(batch)
}

/// One mod in the requested priority order: its db id and its addon file names (enabled form,
/// without the `.disabled` suffix), in the mod's own file order.
pub struct ReorderItem {
    pub mod_id: i64,
    pub files: Vec<String>,
}

/// New file name for every file that has to move, given mods in the new priority order.
///
/// The set of slots our files occupy stays exactly the same (so foreign addons and free slots are
/// never touched); only which mod sits in which slot changes. A mod's files keep their relative order.
pub fn plan_reorder(items: &[ReorderItem]) -> Result<Vec<(String, String)>, InstallError> {
    let mut flat: Vec<(i64, &String)> = Vec::new();
    for it in items {
        let mut fs: Vec<&String> = it.files.iter().collect();
        fs.sort_by_key(|f| slot_of(f).unwrap_or(u8::MAX));
        flat.extend(fs.into_iter().map(|f| (it.mod_id, f)));
    }
    let mut slots: Vec<u8> = Vec::with_capacity(flat.len());
    for (_, f) in &flat {
        // ours are plain `pakNN_dir.vpk` names (or the first versions' ones); any other prefix is another manager's file
        let slot = slot_of(f)
            .filter(|slot| is_legacy(f) || **f == addon_name(0, *slot))
            .ok_or_else(|| InstallError::Journal(format!("not one of our addon files: {f}")))?;
        slots.push(slot);
    }
    slots.sort_unstable();
    slots.dedup();
    if slots.len() != flat.len() {
        return Err(InstallError::Journal("two files share one slot".into()));
    }
    Ok(flat
        .into_iter()
        .zip(slots)
        .filter_map(|((mod_id, old), slot)| {
            let new = addon_name(mod_id, slot);
            (&new != old).then(|| (old.clone(), new))
        })
        .collect())
}

/// Renames addon files so the slots follow `items` (index 0 = highest priority = lowest slot).
/// Two phases through temporary names so swaps cannot collide; every rename is journaled in one
/// batch (`reorder-0-<ts>`) and everything is undone if any step fails. `None` = already in order.
pub fn reorder_files(ctx: &Ctx, items: &[ReorderItem]) -> Result<Option<String>, InstallError> {
    let moves = plan_reorder(items)?;
    if moves.is_empty() {
        return Ok(None);
    }
    let batch = format!("reorder-0-{}", now());
    let dir = &ctx.game.addons_dir;
    let result = (|| {
        // Each moving file keeps its enabled/disabled state.
        let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new(); // (temp, final)
        for (old, new) in &moves {
            let (from, suffix) = if dir.join(old).is_file() {
                (dir.join(old), "")
            } else if dir.join(disabled_name(old)).is_file() {
                (dir.join(disabled_name(old)), super::pak_order::DISABLED_SUFFIX)
            } else {
                return Err(InstallError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, format!("{old} is missing"))));
            };
            let target = dir.join(format!("{new}{suffix}"));
            ensure_within(dir, &from)?;
            ensure_within(dir, &target)?;
            let mut tmp = from.clone().into_os_string();
            tmp.push(".ddmm-reorder");
            let tmp = PathBuf::from(tmp);
            if tmp.exists() {
                return Err(InstallError::AlreadyExists(tmp));
            }
            ctx.journal.record(&batch, Op::Rename { from: from.clone(), to: tmp.clone() })?;
            std::fs::rename(&from, &tmp)?;
            staged.push((tmp, target));
        }
        for (tmp, target) in staged {
            if target.exists() {
                return Err(InstallError::AlreadyExists(target));
            }
            ctx.journal.record(&batch, Op::Rename { from: tmp.clone(), to: target.clone() })?;
            std::fs::rename(&tmp, &target)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = ctx.journal.rollback_batch(&batch);
    }
    result.map(|_| Some(batch))
}

/// Replace a mod's files with a newer archive as ONE journal batch (`update-<mod_id>-<ts>`).
/// Old files are backed up and removed, the new VPKs take slots `first_slot..`, and the enabled/disabled
/// state of the mod is kept. Any failure (bad archive, a slot already taken, ...) restores the old files.
pub fn update_files(ctx: &Ctx, archive: &Path, mod_id: i64, old_addons: &[String], first_slot: u8) -> Result<InstallOutcome, InstallError> {
    let batch = format!("update-{mod_id}-{}", now());
    let was_enabled = old_addons.iter().any(|a| ctx.game.addons_dir.join(a).is_file());

    // Delete first: the new files reuse the same names/slots. A failure anywhere rolls the batch back.
    if let Err(e) = uninstall_in_batch(ctx, &batch, old_addons) {
        let _ = ctx.journal.rollback_batch(&batch);
        return Err(e);
    }
    let mut outcome = install_in_batch(ctx, archive, mod_id, first_slot, &batch)?;

    if !was_enabled {
        let result = (|| {
            for f in &mut outcome.files {
                let off = ctx.game.addons_dir.join(disabled_name(&f.addon_name));
                ctx.journal.record(&batch, Op::Rename { from: f.path.clone(), to: off.clone() })?;
                std::fs::rename(&f.path, &off)?;
                f.path = off;
            }
            Ok::<(), InstallError>(())
        })();
        if let Err(e) = result {
            let _ = ctx.journal.rollback_batch(&batch);
            return Err(e);
        }
    }
    outcome.batch = batch;
    Ok(outcome)
}
