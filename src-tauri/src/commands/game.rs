//! Game launch, performance presets and repair commands.
use super::library::{dirs, game};
use super::{err, AppState};
use crate::game::checks::{self, Finding};
use crate::game::perf;
use crate::game::prefs::{self, GamePrefs};
use crate::install::extract::Limits;
use crate::install::installer::Ctx;
use crate::install::journal::Journal;
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

fn prefs_path(state: &AppState) -> std::path::PathBuf {
    state.app_dir.join("game_prefs.json")
}

#[tauri::command]
pub fn get_game_prefs(state: State<'_, AppState>) -> GamePrefs {
    prefs::load(&prefs_path(&state))
}

/// Saves the options. The performance variables are written to `autoexec.cfg` when the game is known.
#[tauri::command]
pub async fn save_game_prefs(state: State<'_, AppState>, prefs: GamePrefs) -> Result<GamePrefs, String> {
    prefs.validate().map_err(err)?;
    let _guard = state.busy.lock().await;
    if let Ok(g) = game(&state) {
        let d = dirs(&state);
        let p = prefs.clone();
        tokio::task::spawn_blocking(move || {
            let journal = Journal::open(&d.journal);
            let ctx = Ctx { game: &g, staging: &d.staging, backups: &d.backups, journal: &journal, limits: Limits::default() };
            checks::write_performance(&ctx, &p).map(|_| ())
        })
        .await
        .map_err(err)?
        .map_err(err)?;
    }
    prefs::save(&prefs_path(&state), &prefs).map_err(err)?;
    Ok(prefs)
}

/// Starts Deadlock through Steam with the saved launch options.
#[tauri::command]
pub fn launch_game(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let g = game(&state)?;
    if !g.root.exists() {
        return Err("game_not_found".into());
    }
    let args = prefs::load(&prefs_path(&state)).launch_args().map_err(err)?;
    let url = prefs::steam_url(&args);
    app.opener().open_url(&url, None::<&str>).map_err(err)?;
    Ok(args.join(" "))
}

/// True while the Deadlock process is running. Used to warn that changes apply only after a restart.
/// Looks at the process list by image name (Deadlock's executable is `project8.exe`); never touches the process.
#[tauri::command]
pub async fn game_running() -> bool {
    tokio::task::spawn_blocking(|| {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            if let Ok(out) = std::process::Command::new("tasklist").args(["/FO", "CSV", "/NH"]).creation_flags(CREATE_NO_WINDOW).output() {
                let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
                return text.contains("\"project8.exe\"") || text.contains("\"deadlock.exe\"");
            }
        }
        false
    })
    .await
    .unwrap_or(false)
}

#[tauri::command]
pub fn run_checks(state: State<'_, AppState>) -> Result<Vec<Finding>, String> {
    Ok(checks::run_checks(&game(&state)?))
}

/// Looks into the enabled mods for models that may break on the current game version. Changes nothing.
#[tauri::command]
pub async fn scan_compat(state: State<'_, AppState>) -> Result<crate::game::compat::CompatReport, String> {
    let g = game(&state)?;
    tokio::task::spawn_blocking(move || -> Result<crate::game::compat::CompatReport, String> {
        let addons = crate::library::scan::scan_addons(&g.addons_dir).map_err(err)?;
        let pkg = g.citadel_dir.join(crate::game::compat::COMPAT_DIR).join(crate::game::compat::PACKAGE_FILE);
        let mut rep = crate::game::compat::scan(&addons, Some(&pkg), Some(&g.citadel_dir.join("pak01_dir.vpk")));
        let active = std::fs::read_to_string(&g.gameinfo)
            .map(|t| matches!(crate::install::gameinfo::ensure_path_before_addons(&t, crate::game::compat::SEARCH_PATH), crate::install::gameinfo::PatchResult::AlreadyPresent))
            .unwrap_or(true);
        if pkg.exists() && !active {
            rep.findings.push(crate::game::compat::CompatFinding { kind: "package_inactive".into(), severity: "warn", model: pkg.display().to_string(), mods: vec![] });
        }
        // the game was updated after the package was made: its models may have new camera data, so the copies
        // in the package are out of date
        let newer = |a: &std::path::Path, b: &std::path::Path| match (std::fs::metadata(a).and_then(|m| m.modified()), std::fs::metadata(b).and_then(|m| m.modified())) {
            (Ok(x), Ok(y)) => x > y,
            _ => false,
        };
        if pkg.exists() && newer(&g.citadel_dir.join("pak01_dir.vpk"), &pkg) {
            rep.findings.push(crate::game::compat::CompatFinding { kind: "compat_stale".into(), severity: "warn", model: pkg.display().to_string(), mods: vec![] });
        }
        Ok(rep)
    })
    .await
    .map_err(err)?
}

/// Gives models without camera data the camera data of the game's own model at the same path. The fixed
/// models go into one separate package in `citadel/guddion_compat`, which `gameinfo.gi` lists before
/// `citadel/addons` so it wins over every mod. The mods themselves are not touched. Journaled.
#[tauri::command]
pub async fn repair_cameras(state: State<'_, AppState>) -> Result<crate::game::compat::RepairReport, String> {
    use crate::game::compat::{self, COMPAT_DIR, LEGACY_PACKAGE, PACKAGE_FILE, SEARCH_PATH};
    use crate::install::gameinfo::{ensure_compat_path, write_atomic, PatchResult};
    use crate::install::journal::Op;
    let _guard = state.busy.lock().await;
    let g = game(&state)?;
    let d = dirs(&state);
    tokio::task::spawn_blocking(move || -> Result<compat::RepairReport, String> {
        let addons = crate::library::scan::scan_addons(&g.addons_dir).map_err(err)?;
        let dir = g.citadel_dir.join(COMPAT_DIR);
        let package = dir.join(PACKAGE_FILE);
        let legacy = g.addons_dir.join(LEGACY_PACKAGE);
        let (files, rep) = compat::repair_cameras(&g.citadel_dir.join("pak01_dir.vpk"), &addons)?;
        let journal = Journal::open(&d.journal);
        let ctx = Ctx { game: &g, staging: &d.staging, backups: &d.backups, journal: &journal, limits: Limits::default() };
        let batch = format!("repair-camera-{}", checks::stamp());

        // nothing to repair: an old package (made from mods that have changed since) must not stay, it would
        // override the real models; the first version's file in addons goes too
        if files.is_empty() {
            for old_file in [&package, &legacy] {
                if old_file.exists() {
                    let old = std::fs::read(old_file).map_err(err)?;
                    let name = old_file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let backup = d.backups.join(format!("{batch}_{name}"));
                    std::fs::create_dir_all(&d.backups).map_err(err)?;
                    write_atomic(&backup, &old).map_err(err)?;
                    journal.record(&batch, Op::DeleteFile { path: old_file.clone(), backup }).map_err(err)?;
                    std::fs::remove_file(old_file).map_err(err)?;
                }
            }
            return Ok(rep);
        }
        let refs: Vec<(&str, &[u8])> = files.iter().map(|(p, b)| (p.as_str(), b.as_slice())).collect();
        let bytes = crate::install::vpk::build_vpk(&refs);

        // gameinfo.gi first: if it cannot be patched, nothing else is written
        let gi_old = std::fs::read(&g.gameinfo).map_err(err)?;
        let gi_text = String::from_utf8(gi_old.clone()).map_err(|_| "gameinfo.gi is not valid UTF-8".to_string())?;
        let gi_new = match ensure_compat_path(&gi_text, SEARCH_PATH) {
            PatchResult::AlreadyPresent => None,
            PatchResult::Patched(t) => Some(t),
            PatchResult::Unrecognized => return Err("gameinfo.gi has a layout this app cannot edit safely".into()),
        };
        if !dir.exists() {
            journal.record(&batch, Op::CreateDir { path: dir.clone() }).map_err(err)?;
            std::fs::create_dir_all(&dir).map_err(err)?;
        }
        if package.exists() {
            let old = std::fs::read(&package).map_err(err)?;
            if old != bytes {
                checks::replace_journaled(&ctx, &batch, &package, &old, &bytes).map_err(err)?;
            } else if let Ok(f) = std::fs::OpenOptions::new().write(true).open(&package) {
                // still right for the current game: mark it as checked now, so it is not taken for out of date
                let _ = f.set_modified(std::time::SystemTime::now());
            }
        } else {
            journal.record(&batch, Op::CreateFile { path: package.clone() }).map_err(err)?;
            write_atomic(&package, &bytes).map_err(err)?;
        }
        if let Some(t) = gi_new {
            checks::replace_journaled(&ctx, &batch, &g.gameinfo, &gi_old, t.as_bytes()).map_err(err)?;
        }
        if legacy.exists() {
            // the first version kept the package in addons; its models are in the new one now
            let old = std::fs::read(&legacy).map_err(err)?;
            let backup = d.backups.join(format!("{batch}_{LEGACY_PACKAGE}"));
            std::fs::create_dir_all(&d.backups).map_err(err)?;
            write_atomic(&backup, &old).map_err(err)?;
            journal.record(&batch, Op::DeleteFile { path: legacy.clone(), backup }).map_err(err)?;
            std::fs::remove_file(&legacy).map_err(err)?;
        }
        Ok(rep)
    })
    .await
    .map_err(err)?
}

#[derive(Serialize)]
pub struct FixResult {
    pub changed: usize,
}

/// Applies one fix. `items` are the files the fix should touch (for `disable`).
#[tauri::command]
pub async fn apply_fix(state: State<'_, AppState>, fix: String, items: Vec<String>) -> Result<FixResult, String> {
    let _guard = state.busy.lock().await;
    let g = game(&state)?;
    let d = dirs(&state);
    tokio::task::spawn_blocking(move || -> Result<FixResult, String> {
        let journal = Journal::open(&d.journal);
        let ctx = Ctx { game: &g, staging: &d.staging, backups: &d.backups, journal: &journal, limits: Limits::default() };
        let changed = match fix.as_str() {
            "patch_gameinfo" => usize::from(!checks::fix_gameinfo(&ctx).map_err(err)?.is_empty()),
            "comment_autoexec" => usize::from(!checks::fix_autoexec_camera(&ctx).map_err(err)?.is_empty()),
            "clean_temp" => checks::clean_temp(&g).map_err(err)?,
            "disable" => checks::disable_addons(&ctx, &items).map_err(err)?,
            _ => return Err("unknown fix".into()),
        };
        Ok(FixResult { changed })
    })
    .await
    .map_err(err)?
}

#[derive(Serialize)]
pub struct PerfPreset {
    pub id: String,
    pub name: String,
    pub author: String,
    pub tier: String,
    pub blurb: String,
    pub recommended: bool,
    pub url: Option<String>,
    pub license: Option<String>,
    pub settings: usize,
    pub skipped: usize,
}

#[derive(Serialize)]
pub struct PerfState {
    pub presets: Vec<PerfPreset>,
    pub status: perf::PerfStatus,
}

fn read_gameinfo(g: &crate::steam::locate::GameInstall) -> Result<String, String> {
    String::from_utf8(std::fs::read(&g.gameinfo).map_err(err)?).map_err(|_| "gameinfo.gi is not valid UTF-8".to_string())
}

#[tauri::command]
pub fn perf_state(state: State<'_, AppState>) -> Result<PerfState, String> {
    let g = game(&state)?;
    let text = read_gameinfo(&g)?;
    let presets = perf::presets()
        .iter()
        .map(|p| PerfPreset {
            id: p.id.clone(),
            name: p.name.clone(),
            author: p.author.clone(),
            tier: p.tier.clone(),
            blurb: p.blurb.clone(),
            recommended: p.recommended,
            url: p.url.clone(),
            license: p.license.clone(),
            settings: p.entries.len(),
            skipped: p.skipped,
        })
        .collect();
    Ok(PerfState { presets, status: perf::status(&text) })
}

#[tauri::command]
pub fn perf_details(id: String) -> Result<Vec<(String, String)>, String> {
    perf::preset(&id).map(|p| p.entries.clone()).ok_or_else(|| format!("unknown config: {id}"))
}

/// Apply a config (`id`) or take the applied one out (`None`). Undo restores gameinfo.gi from the backup.
#[tauri::command]
pub async fn perf_apply(state: State<'_, AppState>, id: Option<String>) -> Result<perf::PerfStatus, String> {
    let _guard = state.busy.lock().await;
    let g = game(&state)?;
    let d = dirs(&state);
    tokio::task::spawn_blocking(move || -> Result<perf::PerfStatus, String> {
        let old = std::fs::read(&g.gameinfo).map_err(err)?;
        let text = String::from_utf8(old.clone()).map_err(|_| "gameinfo.gi is not valid UTF-8".to_string())?;
        let new = match &id {
            Some(id) => perf::apply(&text, perf::preset(id).ok_or_else(|| format!("unknown config: {id}"))?).map_err(err)?,
            None => perf::remove(&text),
        };
        if new != text {
            let journal = Journal::open(&d.journal);
            let ctx = Ctx { game: &g, staging: &d.staging, backups: &d.backups, journal: &journal, limits: Limits::default() };
            let batch = format!("performance-gi-{}", checks::stamp());
            checks::replace_journaled(&ctx, &batch, &g.gameinfo, &old, new.as_bytes()).map_err(err)?;
        }
        Ok(perf::status(&new))
    })
    .await
    .map_err(err)?
}

/// Takes another manager's performance overlay out of gameinfo.gi. The file is backed up and the change is journaled like every other edit.
#[tauri::command]
pub async fn perf_fix_foreign(state: State<'_, AppState>) -> Result<perf::PerfStatus, String> {
    let _guard = state.busy.lock().await;
    let g = game(&state)?;
    let d = dirs(&state);
    tokio::task::spawn_blocking(move || -> Result<perf::PerfStatus, String> {
        let old = std::fs::read(&g.gameinfo).map_err(err)?;
        let text = String::from_utf8(old.clone()).map_err(|_| "gameinfo.gi is not valid UTF-8".to_string())?;
        let new = perf::remove_foreign(&text).map_err(err)?;
        if new != text {
            let journal = Journal::open(&d.journal);
            let ctx = Ctx { game: &g, staging: &d.staging, backups: &d.backups, journal: &journal, limits: Limits::default() };
            let batch = format!("performance-gi-{}", checks::stamp());
            checks::replace_journaled(&ctx, &batch, &g.gameinfo, &old, new.as_bytes()).map_err(err)?;
        }
        Ok(perf::status(&new))
    })
    .await
    .map_err(err)?
}
