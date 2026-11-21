//! Profile commands.
use super::library::{dirs, game, installed_mods, InstalledMod};
use super::{err, AppState};
use crate::install::extract::Limits;
use crate::install::installer::{toggle_many, Ctx};
use crate::install::journal::Journal;
use crate::profiles::store;
use crate::profiles::{self, InstalledRef, ProfileEntry};
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

#[derive(Serialize)]
pub struct ProfileEntryView {
    #[serde(flatten)]
    pub entry: ProfileEntry,
    pub installed: bool,
}

#[derive(Serialize)]
pub struct ProfileView {
    pub id: i64,
    pub name: String,
    pub active: bool,
    pub mods: Vec<ProfileEntryView>,
}

#[derive(Serialize)]
pub struct ApplyResult {
    pub enabled: usize,
    pub disabled: usize,
    pub missing: Vec<ProfileEntry>,
}

fn sql_err(e: sqlx::Error) -> String {
    if store::is_unique_violation(&e) { "name_taken".into() } else { e.to_string() }
}

/// Current library as profile entries (slot order, enabled state from disk).
fn snapshot(mods: &[InstalledMod]) -> Vec<ProfileEntry> {
    mods.iter()
        .filter_map(|m| {
            Some(ProfileEntry {
                source: m.source.clone()?,
                remote_id: m.remote_id.clone()?,
                name: m.name.clone(),
                version: m.version.clone(),
                enabled: m.enabled,
                position: 0,
            })
        })
        .enumerate()
        .map(|(i, mut e)| {
            e.position = i as i32;
            e
        })
        .collect()
}

async fn view(state: &AppState, id: i64, name: String, active: bool, installed: &[InstalledMod]) -> Result<ProfileView, String> {
    let entries = store::entries_of(&state.db().await?.pool, id).await.map_err(err)?;
    let mods = entries
        .into_iter()
        .map(|e| {
            let is_installed = installed.iter().any(|m| m.source.as_deref() == Some(&e.source) && m.remote_id.as_deref() == Some(&e.remote_id));
            ProfileEntryView { entry: e, installed: is_installed }
        })
        .collect();
    Ok(ProfileView { id, name, active, mods })
}

#[tauri::command]
pub async fn list_profiles(state: State<'_, AppState>) -> Result<Vec<ProfileView>, String> {
    let installed = installed_mods(&state).await?;
    let rows = store::list(&state.db().await?.pool).await.map_err(err)?;
    let mut out = Vec::new();
    for (id, name, active) in rows {
        out.push(view(&state, id, name, active, &installed).await?);
    }
    Ok(out)
}

/// New profile from the current library state.
#[tauri::command]
pub async fn create_profile(state: State<'_, AppState>, name: String) -> Result<i64, String> {
    let name = profiles::clean_name(&name)?;
    let entries = snapshot(&installed_mods(&state).await?);
    store::create(&state.db().await?.pool, &name, &entries).await.map_err(sql_err)
}

#[tauri::command]
pub async fn update_profile_from_current(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let entries = snapshot(&installed_mods(&state).await?);
    let pool = &state.db().await?.pool;
    store::replace_entries(pool, id, &entries).await.map_err(err)?;
    store::set_active(pool, Some(id)).await.map_err(err)
}

#[tauri::command]
pub async fn rename_profile(state: State<'_, AppState>, id: i64, name: String) -> Result<(), String> {
    store::rename(&state.db().await?.pool, id, &profiles::clean_name(&name)?).await.map_err(sql_err)
}

#[tauri::command]
pub async fn delete_profile(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    store::delete(&state.db().await?.pool, id).await.map_err(err)
}

/// Switch to a profile: enable/disable installed mods to match it. One undoable action.
#[tauri::command]
pub async fn apply_profile(state: State<'_, AppState>, id: i64) -> Result<ApplyResult, String> {
    let _guard = state.busy.lock().await;
    let game = game(&state)?;
    let d = dirs(&state);
    let pool = &state.db().await?.pool;
    let entries = store::entries_of(pool, id).await.map_err(err)?;
    let installed = installed_mods(&state).await?;

    let refs: Vec<InstalledRef> = installed
        .iter()
        .filter_map(|m| Some(InstalledRef { id: m.id, source: m.source.clone()?, remote_id: m.remote_id.clone()?, enabled: m.enabled }))
        .collect();
    let plan = profiles::plan_apply(&entries, &refs);

    let files_of = |mod_id: i64| installed.iter().find(|m| m.id == mod_id).map(|m| m.files.iter().map(|f| f.name.clone()).collect::<Vec<_>>()).unwrap_or_default();
    let mut items: Vec<(Vec<String>, bool)> = plan.disable.iter().map(|i| (files_of(*i), false)).collect();
    items.extend(plan.enable.iter().map(|i| (files_of(*i), true)));

    if !items.is_empty() {
        let label = id.to_string();
        tokio::task::spawn_blocking(move || {
            let journal = Journal::open(&d.journal);
            let ctx = Ctx { game: &game, staging: &d.staging, backups: &d.backups, journal: &journal, limits: Limits::default() };
            toggle_many(&ctx, &label, &items).map(|_| ())
        })
        .await
        .map_err(err)?
        .map_err(err)?;
    }
    store::set_active(pool, Some(id)).await.map_err(err)?;
    Ok(ApplyResult { enabled: plan.enable.len(), disabled: plan.disable.len(), missing: plan.missing })
}

#[tauri::command]
pub async fn export_profile(state: State<'_, AppState>, id: i64, path: String) -> Result<(), String> {
    let pool = &state.db().await?.pool;
    let name = store::name_of(pool, id).await.map_err(err)?.ok_or("profile_not_found")?;
    let entries = store::entries_of(pool, id).await.map_err(err)?;
    let json = serde_json::to_vec_pretty(&profiles::to_file(&name, entries)).map_err(err)?;
    tokio::fs::write(PathBuf::from(path), json).await.map_err(err)
}

/// Prefix of a profile share code, so a pasted code is recognised and a future format can be told apart.
const CODE_PREFIX: &str = "kae1:";
/// Codes made before the rename.
const LEGACY_CODE_PREFIX: &str = "kea1:";

/// The profile as one line of text (`kae1:` + base64url of the same JSON a profile file holds), to paste into a chat.
#[tauri::command]
pub async fn profile_code(state: State<'_, AppState>, id: i64) -> Result<String, String> {
    use base64::Engine;
    let pool = &state.db().await?.pool;
    let name = store::name_of(pool, id).await.map_err(err)?.ok_or("profile_not_found")?;
    let entries = store::entries_of(pool, id).await.map_err(err)?;
    let json = serde_json::to_vec(&profiles::to_file(&name, entries)).map_err(err)?;
    Ok(format!("{CODE_PREFIX}{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)))
}

/// Import a profile from a share code. Same checks as a file: untrusted text, nothing is installed.
#[tauri::command]
pub async fn import_profile_code(state: State<'_, AppState>, code: String) -> Result<i64, String> {
    use base64::Engine;
    let body = code.trim().strip_prefix(CODE_PREFIX).or_else(|| code.trim().strip_prefix(LEGACY_CODE_PREFIX)).ok_or("not_a_profile:wrong code")?;
    if body.len() > profiles::MAX_FILE_BYTES * 2 {
        return Err("file_too_large".into());
    }
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(body.trim()).map_err(|_| "not_a_profile:bad code".to_string())?;
    let file = profiles::parse_profile_file(&bytes)?;
    store_imported(&state, file).await
}

async fn store_imported(state: &AppState, file: profiles::ProfileFile) -> Result<i64, String> {
    let pool = &state.db().await?.pool;
    // Keep the imported name unique: "Name", "Name (2)", ...
    for n in 1..=50 {
        let candidate = if n == 1 { file.name.clone() } else { format!("{} ({n})", file.name) };
        match store::create(pool, &candidate, &file.mods).await {
            Ok(id) => return Ok(id),
            Err(e) if store::is_unique_violation(&e) => continue,
            Err(e) => return Err(err(e)),
        }
    }
    Err("name_taken".into())
}

/// Import a profile file. Never installs anything: entries for mods you do not have are listed as missing.
#[tauri::command]
pub async fn import_profile(state: State<'_, AppState>, path: String) -> Result<i64, String> {
    let meta = tokio::fs::metadata(&path).await.map_err(err)?;
    if meta.len() as usize > profiles::MAX_FILE_BYTES {
        return Err("file_too_large".into());
    }
    let bytes = tokio::fs::read(&path).await.map_err(err)?;
    let file = profiles::parse_profile_file(&bytes)?;
    store_imported(&state, file).await
}
