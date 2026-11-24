//! Minecraft commands: instances, game versions, catalogs, launching.
use super::AppState;
use crate::minecraft::{self as mc, catalog, java, launch, loader, modpack, share, versions, Config, Instance, Paths, Progress};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

type R<T> = Result<T, String>;

static CANCELS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<tokio::sync::Notify>>>> = std::sync::LazyLock::new(Default::default);

/// Runs `fut` until it ends or `mc_cancel(key)` is called; a cancelled run gives the error `cancelled`.
async fn cancellable<T>(key: &str, fut: impl std::future::Future<Output = R<T>>) -> R<T> {
    let n = std::sync::Arc::new(tokio::sync::Notify::new());
    CANCELS.lock().unwrap_or_else(|x| x.into_inner()).insert(key.to_string(), n.clone());
    let out = tokio::select! {
        r = fut => r,
        _ = n.notified() => Err("cancelled".to_string()),
    };
    CANCELS.lock().unwrap_or_else(|x| x.into_inner()).remove(key);
    out
}

#[tauri::command]
pub fn mc_cancel(key: String) {
    if let Some(n) = CANCELS.lock().unwrap_or_else(|x| x.into_inner()).get(&key) {
        n.notify_one();
    }
}

fn paths(s: &AppState) -> Paths {
    Paths::new(&s.app_dir)
}

fn reporter(app: &AppHandle, id: String) -> impl Fn(&str, u64, u64) + Send + Sync + 'static {
    let app = app.clone();
    move |stage: &str, done: u64, total: u64| {
        let _ = app.emit("mc-progress", Progress { id: id.clone(), stage: stage.into(), done, total });
    }
}

#[derive(serde::Serialize)]
pub struct ConfigView {
    pub has_curseforge_key: bool,
    pub account_name: String,
    /// The Microsoft account that is signed in.
    pub ms_name: Option<String>,
    /// An application id for the Microsoft sign-in is set (by the person or at build time).
    pub ms_ready: bool,
}

fn view(c: &Config) -> ConfigView {
    ConfigView { has_curseforge_key: !c.curseforge_key.trim().is_empty(), account_name: c.account.name.clone(), ms_name: c.account.ms.as_ref().map(|m| m.name.clone()), ms_ready: crate::minecraft::auth::client_id(c).is_some() }
}

#[tauri::command]
pub fn mc_config(state: State<'_, AppState>) -> ConfigView {
    view(&mc::load_config(&paths(&state)))
}

/// `curseforge_key: None` keeps the stored key; an empty string removes it.
#[tauri::command]
pub fn mc_save_config(state: State<'_, AppState>, curseforge_key: Option<String>, account_name: Option<String>, ms_client_id: Option<String>) -> R<ConfigView> {
    let p = paths(&state);
    let mut c: Config = mc::load_config(&p);
    if let Some(id) = ms_client_id {
        c.ms_client_id = id.trim().to_string();
    }
    if let Some(k) = curseforge_key {
        c.curseforge_key = k.trim().to_string();
    }
    if let Some(n) = account_name {
        let n = n.trim();
        let ok = (3..=16).contains(&n.chars().count()) && n.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        if !ok {
            return Err("bad_player_name".into());
        }
        c.account.name = n.to_string();
    }
    mc::save_config(&p, &c)?;
    Ok(view(&c))
}

/// Starts a Microsoft sign-in: the code to type on the page the person opens.
#[tauri::command]
pub async fn mc_ms_start(state: State<'_, AppState>) -> R<crate::minecraft::auth::DeviceCode> {
    let cfg = mc::load_config(&paths(&state));
    let cid = crate::minecraft::auth::client_id(&cfg).ok_or("ms_no_client_id")?;
    crate::minecraft::auth::start(&state.net.client(), &cid).await
}

/// Asks whether the person has finished. The name of the account when they have, `None` while not.
#[tauri::command]
pub async fn mc_ms_poll(state: State<'_, AppState>, device_code: String) -> R<Option<String>> {
    let p = paths(&state);
    let mut cfg = mc::load_config(&p);
    let cid = crate::minecraft::auth::client_id(&cfg).ok_or("ms_no_client_id")?;
    let Some(ms) = crate::minecraft::auth::poll(&state.net.client(), &cid, &device_code).await? else { return Ok(None) };
    let name = ms.name.clone();
    cfg.account.ms = Some(ms);
    cfg.account.offline = false;
    mc::save_config(&p, &cfg)?;
    Ok(Some(name))
}

#[tauri::command]
pub fn mc_ms_logout(state: State<'_, AppState>) -> R<ConfigView> {
    let p = paths(&state);
    let mut cfg = mc::load_config(&p);
    cfg.account.ms = None;
    cfg.account.offline = true;
    mc::save_config(&p, &cfg)?;
    Ok(view(&cfg))
}

#[derive(serde::Serialize)]
pub struct RootView {
    pub path: String,
    pub is_default: bool,
}

fn root_view(p: &Paths) -> RootView {
    RootView { path: p.root.to_string_lossy().into_owned(), is_default: p.root == p.default_root() }
}

#[tauri::command]
pub fn mc_root(state: State<'_, AppState>) -> RootView {
    root_view(&paths(&state))
}

/// Where games, Java and instances are stored. Data already downloaded stays in the old folder.
#[tauri::command]
pub fn mc_set_root(state: State<'_, AppState>, path: Option<String>) -> R<RootView> {
    let mut p = paths(&state);
    let dir = path.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(std::path::PathBuf::from);
    p.set_root(dir.as_deref())?;
    Ok(root_view(&p))
}

#[tauri::command]
pub async fn mc_game_versions(state: State<'_, AppState>) -> R<Vec<versions::GameVersion>> {
    versions::list_versions(&state.net.client()).await
}

#[tauri::command]
pub async fn mc_loader_versions(state: State<'_, AppState>, loader: String, mc_version: String) -> R<Vec<String>> {
    if loader::supports(&loader) {
        return loader::versions_for(&state.net.client(), &loader, &mc_version).await;
    }
    versions::loader_versions(&state.net.client(), &loader, &mc_version).await
}

#[tauri::command]
pub fn mc_instances(state: State<'_, AppState>) -> Vec<Instance> {
    mc::list_instances(&paths(&state))
}

#[tauri::command]
pub async fn mc_create_instance(app: AppHandle, state: State<'_, AppState>, name: String, mc_version: String, loader: String, loader_version: Option<String>) -> R<Instance> {
    let p = paths(&state);
    let name = name.trim();
    if name.is_empty() {
        return Err("name_required".into());
    }
    if !matches!(loader.as_str(), "vanilla" | "fabric" | "quilt" | "forge" | "neoforge") {
        return Err(format!("loader_unsupported:{loader}"));
    }
    let client = state.net.client();
    let mut inst = Instance { id: mc::new_instance_id(&p, name), name: name.into(), mc_version, loader, loader_version, created: mc::now(), ..Default::default() };
    // Forge and NeoForge are installed right away so that a failure shows up here and not at the first start
    inst.version_id = cancellable("create", loader::ensure_version(&client, &p, &inst, &reporter(&app, "create".into()))).await?;
    inst.loader_version = share::loader_version_of(&inst);
    mc::save_instance(&p, &inst)?;
    Ok(inst)
}

#[derive(serde::Deserialize)]
pub struct InstancePatch {
    pub name: Option<String>,
    pub memory_mb: Option<u32>,
    pub java_path: Option<String>,
    pub extra_args: Option<String>,
    /// `false` returns to the default memory.
    pub memory_custom: Option<bool>,
    pub loader_version: Option<String>,
    pub debug_console: Option<bool>,
}

#[tauri::command]
pub fn mc_update_instance(state: State<'_, AppState>, id: String, patch: InstancePatch) -> R<Instance> {
    let p = paths(&state);
    let mut i = mc::get_instance(&p, &id)?;
    if let Some(n) = patch.name.filter(|n| !n.trim().is_empty()) {
        i.name = n.trim().into();
    }
    if let Some(m) = patch.memory_mb {
        i.memory_mb = m.clamp(1024, 65536);
    }
    if let Some(c) = patch.memory_custom {
        i.memory_custom = c;
    }
    if let Some(a) = patch.extra_args {
        i.extra_args = a.trim().to_string();
    }
    if let Some(d) = patch.debug_console {
        i.debug_console = d;
    }
    if let Some(v) = patch.loader_version.filter(|v| !v.is_empty() && Some(v) != i.loader_version.as_ref()) {
        // another loader build: the version is installed again at the next start
        i.loader_version = Some(v);
        i.version_id = String::new();
    }
    if let Some(j) = patch.java_path {
        i.java_path = (!j.trim().is_empty()).then(|| j.trim().to_string());
    }
    mc::save_instance(&p, &i)?;
    Ok(i)
}

#[tauri::command]
pub fn mc_delete_instance(state: State<'_, AppState>, id: String) -> R<()> {
    let p = paths(&state);
    mc::get_instance(&p, &id)?; // also checks the id
    launch::stop(&id);
    std::fs::remove_dir_all(p.instance(&id)).map_err(mc::e)
}

/// Downloads the game files and Java an instance needs, then returns what to start.
async fn prepare(app: &AppHandle, state: &AppState, inst: &Instance) -> R<(launch::Prepared, Instance)> {
    let p = paths(state);
    let client = state.net.client();
    let rep = reporter(app, inst.id.clone());
    let mut inst = inst.clone();
    let have = !inst.version_id.is_empty() && p.versions().join(&inst.version_id).join(format!("{}.json", inst.version_id)).exists();
    if !have {
        inst.version_id = loader::ensure_version(&client, &p, &inst, &rep).await?;
        mc::save_instance(&p, &inst)?;
    }
    let doc = versions::install_version(&client, &p, &inst.version_id, &rep).await?;
    let major = doc["javaVersion"]["majorVersion"].as_u64().unwrap_or(8) as u32;
    let jv = java::ensure_java(&client, &p, major, inst.java_path.as_deref(), &rep).await?;
    let cfg = mc::load_config(&p);
    let (account, renewed) = crate::minecraft::auth::fresh(&client, &cfg).await?;
    if renewed.is_some() {
        // the new tokens are kept for the next start
        let mut c = cfg.clone();
        c.account = account.clone();
        mc::save_config(&p, &c)?;
    }
    let prep = launch::build(&p, &inst, &doc, jv, &account)?;
    Ok((prep, inst))
}

#[tauri::command]
pub async fn mc_prepare(app: AppHandle, state: State<'_, AppState>, id: String) -> R<()> {
    let inst = mc::get_instance(&paths(&state), &id)?;
    cancellable(&id, prepare(&app, &state, &inst)).await.map(|_| ())
}

#[tauri::command]
pub async fn mc_launch(app: AppHandle, state: State<'_, AppState>, id: String) -> R<u32> {
    let inst = mc::get_instance(&paths(&state), &id)?;
    let (prep, mut inst) = cancellable(&id, prepare(&app, &state, &inst)).await?;
    let pid = launch::spawn(&id, &prep)?;
    inst.last_played = mc::now();
    inst.session_start = inst.last_played;
    let p = paths(&state);
    mc::save_instance(&p, &inst)?;
    watch_session(app, p, id);
    Ok(pid)
}

static WATCHED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
static SESSION: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Adds the running session to the play time. `end` is when the game ended.
fn finish_session(p: &Paths, id: &str, end: u64) -> Option<Instance> {
    let _g = SESSION.lock().unwrap_or_else(|x| x.into_inner());
    let mut i = mc::get_instance(p, id).ok()?;
    if i.session_start == 0 {
        return None;
    }
    i.play_secs += end.max(i.session_start) - i.session_start;
    i.session_start = 0;
    let _ = mc::save_instance(p, &i);
    Some(i)
}

#[derive(serde::Serialize, Clone)]
pub struct ExitEvent {
    pub id: String,
    pub name: String,
    pub crashed: bool,
    pub code: Option<i32>,
    /// `memory` | `java` | `mods` | `unknown`
    pub reason: String,
    pub detail: String,
}

/// What the end of the log and the newest crash report say about why the game stopped.
fn crash_reason(dir: &std::path::Path) -> (String, String) {
    let tail = |path: &std::path::Path| -> String {
        let b = std::fs::read(path).unwrap_or_default();
        String::from_utf8_lossy(&b[b.len().saturating_sub(200_000)..]).into_owned()
    };
    let mut text = tail(&dir.join("launcher.log"));
    let newest = std::fs::read_dir(dir.join("crash-reports")).into_iter().flatten().flatten().filter_map(|d| Some((d.metadata().ok()?.modified().ok()?, d.path()))).max();
    if let Some((_, f)) = newest {
        text.push('\n');
        text.push_str(&tail(&f));
    }
    let line = |needle: &str| text.lines().rev().find(|l| l.contains(needle)).map(|l| l.trim().chars().take(180).collect::<String>());
    if text.contains("OutOfMemoryError") {
        return ("memory".into(), String::new());
    }
    if text.contains("UnsupportedClassVersionError") || text.contains("compiled by a more recent version of the Java Runtime") {
        return ("java".into(), line("UnsupportedClassVersionError").unwrap_or_default());
    }
    if text.contains("UnsatisfiedLinkError") || text.contains("Failed to locate library") {
        return ("natives".into(), line("Failed to locate library").unwrap_or_default());
    }
    for needle in ["requires", "Incompatible mod set", "Mixin apply failed", "Failed to load mod", "Caused by:"] {
        if let Some(d) = line(needle) {
            return ("mods".into(), d);
        }
    }
    ("unknown".into(), String::new())
}

/// Waits for the game to end, adds the play time and tells the page how it ended.
fn watch_session(app: AppHandle, p: Paths, id: String) {
    WATCHED.lock().unwrap_or_else(|x| x.into_inner()).push(id.clone());
    tauri::async_runtime::spawn(async move {
        while launch::is_running(&id) {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        let exit = launch::take_exit(&id);
        let name = finish_session(&p, &id, mc::now()).map(|i| i.name).unwrap_or_default();
        let crashed = exit.as_ref().is_some_and(|e| !e.user && e.code != Some(0));
        let (reason, detail) = if crashed { crash_reason(&mc::game_dir(&p, &id)) } else { (String::new(), String::new()) };
        WATCHED.lock().unwrap_or_else(|x| x.into_inner()).retain(|x| x != &id);
        let _ = app.emit("mc-exit", ExitEvent { id, name, crashed, code: exit.and_then(|e| e.code), reason, detail });
    });
}

#[tauri::command]
pub fn mc_running(state: State<'_, AppState>, id: String) -> bool {
    let running = launch::is_running(&id);
    if !running && !WATCHED.lock().unwrap_or_else(|x| x.into_inner()).contains(&id) {
        // the launcher was closed while the game ran: the session ended when the game last wrote to its log
        let p = paths(&state);
        let end = std::fs::metadata(mc::game_dir(&p, &id).join("launcher.log"))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or_else(mc::now);
        finish_session(&p, &id, end);
    }
    running
}

#[tauri::command]
pub fn mc_stop(id: String) {
    launch::stop(&id)
}

#[derive(serde::Serialize)]
pub struct LogChunk {
    pub text: String,
    /// Where the next read starts.
    pub offset: u64,
    /// The log was replaced by a new one (a new start): the text starts from the beginning.
    pub reset: bool,
}

/// The game console: what the game wrote since `offset` (the file is `launcher.log`, written at every start).
#[tauri::command]
pub fn mc_log(state: State<'_, AppState>, id: String, offset: u64) -> R<LogChunk> {
    use std::io::{Read, Seek, SeekFrom};
    let p = paths(&state);
    mc::get_instance(&p, &id)?;
    let path = mc::game_dir(&p, &id).join("launcher.log");
    let Ok(mut f) = std::fs::File::open(&path) else { return Ok(LogChunk { text: String::new(), offset: 0, reset: offset > 0 }) };
    let len = f.metadata().map_err(mc::e)?.len();
    let (start, reset) = if len < offset { (0, true) } else { (offset, false) };
    // a long log is read from its tail
    let start = if len - start > 400_000 { len - 400_000 } else { start };
    f.seek(SeekFrom::Start(start)).map_err(mc::e)?;
    let mut buf = Vec::new();
    f.take(len - start).read_to_end(&mut buf).map_err(mc::e)?;
    Ok(LogChunk { text: String::from_utf8_lossy(&buf).into_owned(), offset: len, reset })
}

#[derive(serde::Serialize)]
pub struct Screenshot {
    pub name: String,
    pub size: u64,
    pub modified: u64,
}

#[tauri::command]
pub fn mc_screenshots(state: State<'_, AppState>, id: String) -> R<Vec<Screenshot>> {
    let p = paths(&state);
    mc::get_instance(&p, &id)?;
    let mut out: Vec<Screenshot> = std::fs::read_dir(mc::game_dir(&p, &id).join("screenshots"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|d| {
            let m = d.metadata().ok()?;
            let modified = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
            Some(Screenshot { name: d.file_name().to_string_lossy().into_owned(), size: m.len(), modified })
        })
        .collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(out)
}

#[tauri::command]
pub fn mc_java(state: State<'_, AppState>, major: u32) -> java::JavaState {
    java::java_state(&paths(&state), major)
}

#[tauri::command]
pub async fn mc_search(state: State<'_, AppState>, req: catalog::SearchReq) -> R<catalog::Page> {
    let key = mc::load_config(&paths(&state)).curseforge_key;
    catalog::search(&state.net.client(), &key, &req).await
}

#[tauri::command]
pub async fn mc_install_project(state: State<'_, AppState>, id: String, provider: String, project_id: String, kind: String) -> R<catalog::Installed> {
    let p = paths(&state);
    let inst = mc::get_instance(&p, &id)?;
    let key = mc::load_config(&p).curseforge_key;
    catalog::install_project(&state.net.client(), &p, &key, &inst, &provider, &project_id, &kind).await
}

#[tauri::command]
pub async fn mc_install_modpack(app: AppHandle, state: State<'_, AppState>, provider: String, project_id: String, name: String, mc_version: Option<String>, loader: Option<String>) -> R<Instance> {
    let p = paths(&state);
    let client = state.net.client();
    let key = mc::load_config(&p).curseforge_key;
    let rep = reporter(&app, format!("pack:{project_id}"));
    let pack_key = format!("pack:{project_id}");
    let mut inst = cancellable(&pack_key, async {
        if provider == "curseforge" {
            modpack::install_curseforge(&client, &p, &key, &project_id, &name, &rep).await
        } else {
            modpack::install_modrinth(&client, &p, &project_id, &name, mc_version.as_deref(), loader.as_deref(), &rep).await
        }
    })
    .await?;
    inst.version_id = cancellable(&pack_key, loader::ensure_version(&client, &p, &inst, &rep)).await?;
    inst.loader_version = share::loader_version_of(&inst);
    inst.source = format!("{provider}:{project_id}");
    mc::save_instance(&p, &inst)?;
    Ok(inst)
}

/// Packs an instance into a `.kaepack` file at `path`.
#[tauri::command]
pub async fn mc_export_instance(app: AppHandle, state: State<'_, AppState>, id: String, path: String) -> R<()> {
    let p = paths(&state);
    let inst = mc::get_instance(&p, &id)?;
    let mut dest = std::path::PathBuf::from(path);
    if dest.extension().is_none_or(|x| !x.eq_ignore_ascii_case(share::EXT)) {
        dest.set_extension(share::EXT);
    }
    share::export(&state.net.client(), &p, &inst, &dest, &reporter(&app, format!("share:{id}"))).await
}

/// Installs a shared pack file (`.kaepack` or Modrinth `.mrpack`) as a new instance.
#[tauri::command]
pub async fn mc_import_pack(app: AppHandle, state: State<'_, AppState>, path: String) -> R<Instance> {
    let p = paths(&state);
    let client = state.net.client();
    let bytes = std::fs::read(&path).map_err(mc::e)?;
    let rep = reporter(&app, "import".into());
    let mut inst = cancellable("import", async {
        match share::kind_of(&bytes)? {
            "kae" => share::import(&client, &p, &bytes, None, &rep).await,
            "mrpack" => modpack::install_mrpack(&client, &p, &bytes, None, &rep).await,
            _ => Err("curseforge_zip_import".to_string()),
        }
    })
    .await?;
    inst.version_id = cancellable("import", loader::ensure_version(&client, &p, &inst, &rep)).await?;
    inst.loader_version = share::loader_version_of(&inst);
    mc::save_instance(&p, &inst)?;
    Ok(inst)
}

#[tauri::command]
pub fn mc_files(state: State<'_, AppState>, id: String) -> R<Vec<catalog::InstalledFile>> {
    let p = paths(&state);
    mc::get_instance(&p, &id)?;
    Ok(catalog::list_files(&p, &id))
}

#[tauri::command]
pub fn mc_file_enabled(state: State<'_, AppState>, id: String, kind: String, name: String, enabled: bool) -> R<()> {
    let p = paths(&state);
    mc::get_instance(&p, &id)?;
    catalog::set_file_enabled(&p, &id, &kind, &name, enabled)
}

#[tauri::command]
pub fn mc_file_remove(state: State<'_, AppState>, id: String, kind: String, name: String) -> R<()> {
    let p = paths(&state);
    mc::get_instance(&p, &id)?;
    catalog::remove_file(&p, &id, &kind, &name)
}

#[tauri::command]
pub fn mc_open_folder(app: AppHandle, state: State<'_, AppState>, id: String, sub: Option<String>) -> R<()> {
    let p = paths(&state);
    mc::get_instance(&p, &id)?;
    let mut dir = mc::game_dir(&p, &id);
    if let Some(s) = sub.filter(|s| matches!(s.as_str(), "screenshots" | "logs" | "saves" | "mods" | "resourcepacks" | "shaderpacks")) {
        dir = dir.join(s);
    }
    std::fs::create_dir_all(&dir).map_err(mc::e)?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(mc::e)
}

/// Discord Rich Presence. `details: None` removes the status.
#[tauri::command]
pub fn presence_set(state: State<'_, AppState>, details: Option<String>, detail_state: Option<String>, timer: Option<bool>) {
    match details {
        Some(d) => crate::presence::set(&state.app_dir, &d, detail_state.as_deref(), timer.unwrap_or(false)),
        None => crate::presence::clear(),
    }
}

#[tauri::command]
pub fn presence_available(state: State<'_, AppState>) -> bool {
    crate::presence::available(&state.app_dir)
}

#[tauri::command]
pub async fn mc_check_updates(state: State<'_, AppState>, id: String) -> R<catalog::UpdateReport> {
    let p = paths(&state);
    let inst = mc::get_instance(&p, &id)?;
    catalog::check_updates(&state.net.client(), &p, &inst).await
}

#[tauri::command]
pub async fn mc_update_file(state: State<'_, AppState>, id: String, kind: String, name: String) -> R<String> {
    let p = paths(&state);
    let inst = mc::get_instance(&p, &id)?;
    catalog::update_file(&state.net.client(), &p, &inst, &kind, &name).await
}

const IMAGE_EXT: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

/// A small JPEG of the picture as a data address: a screenshot of a few MB becomes some tens of KB.
fn data_uri(path: &std::path::Path, max_w: u32, max_h: u32) -> Option<String> {
    use base64::Engine;
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    if !IMAGE_EXT.contains(&ext.as_str()) {
        return None;
    }
    let img = image::load_from_memory(&std::fs::read(path).ok()?).ok()?;
    let small = img.thumbnail(max_w, max_h).to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 82).encode_image(&small).ok()?;
    Some(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(out)))
}

fn cover_file(p: &Paths, id: &str) -> Option<std::path::PathBuf> {
    IMAGE_EXT.iter().map(|e| p.instance(id).join(format!("cover.{e}"))).find(|f| f.exists())
}

/// The pictures of the instances that have one, by id.
#[tauri::command]
pub fn mc_covers(state: State<'_, AppState>) -> std::collections::HashMap<String, String> {
    let p = paths(&state);
    mc::list_instances(&p).into_iter().filter_map(|i| cover_file(&p, &i.id).and_then(|f| data_uri(&f, 360, 360)).map(|d| (i.id, d))).collect()
}

/// `path: None` removes the picture.
#[tauri::command]
pub fn mc_set_cover(state: State<'_, AppState>, id: String, path: Option<String>) -> R<()> {
    let p = paths(&state);
    mc::get_instance(&p, &id)?;
    while let Some(old) = cover_file(&p, &id) {
        std::fs::remove_file(old).map_err(mc::e)?;
    }
    let Some(src) = path else { return Ok(()) };
    let src = std::path::PathBuf::from(src);
    let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !IMAGE_EXT.contains(&ext.as_str()) {
        return Err("not an image".into());
    }
    if std::fs::metadata(&src).map_err(mc::e)?.len() > 4_000_000 {
        return Err("image_too_big".into());
    }
    std::fs::copy(&src, p.instance(&id).join(format!("cover.{ext}"))).map_err(mc::e)?;
    Ok(())
}

fn screenshot_path(p: &Paths, id: &str, name: &str) -> R<std::path::PathBuf> {
    if name.contains(['/', '\\']) || name.contains("..") {
        return Err("bad file name".into());
    }
    Ok(mc::game_dir(p, id).join("screenshots").join(name))
}

/// A screenshot as a picture the page can show.
#[tauri::command]
pub fn mc_screenshot(state: State<'_, AppState>, id: String, name: String) -> R<String> {
    let p = paths(&state);
    mc::get_instance(&p, &id)?;
    data_uri(&screenshot_path(&p, &id, &name)?, 520, 300).ok_or_else(|| "not an image".to_string())
}

#[tauri::command]
pub fn mc_open_screenshot(app: AppHandle, state: State<'_, AppState>, id: String, name: String) -> R<()> {
    let p = paths(&state);
    mc::get_instance(&p, &id)?;
    app.opener().open_path(screenshot_path(&p, &id, &name)?.to_string_lossy(), None::<&str>).map_err(mc::e)
}

/// Takes the picture of a project (the icon of an installed modpack) as the picture of the instance.
#[tauri::command]
pub async fn mc_set_cover_url(state: State<'_, AppState>, id: String, url: String) -> R<()> {
    let p = paths(&state);
    mc::get_instance(&p, &id)?;
    if !url.starts_with("https://") {
        return Err("bad address".into());
    }
    let r = state.net.client().get(&url).send().await.map_err(mc::e)?;
    if !r.status().is_success() {
        return Err(format!("http {}", r.status().as_u16()));
    }
    let ext = match r.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("") {
        t if t.contains("jpeg") => "jpg",
        t if t.contains("webp") => "webp",
        _ => "png",
    };
    let bytes = r.bytes().await.map_err(mc::e)?;
    if bytes.len() > 4_000_000 {
        return Err("image_too_big".into());
    }
    while let Some(old) = cover_file(&p, &id) {
        std::fs::remove_file(old).map_err(mc::e)?;
    }
    std::fs::write(p.instance(&id).join(format!("cover.{ext}")), bytes).map_err(mc::e)
}

#[tauri::command]
pub async fn mc_installed(state: State<'_, AppState>, id: String) -> R<Vec<String>> {
    let p = paths(&state);
    let inst = mc::get_instance(&p, &id)?;
    catalog::installed_projects(&state.net.client(), &p, &inst).await
}

/// Installed memory in MB, for the memory advice.
#[tauri::command]
pub fn mc_system_memory() -> u64 {
    let mut s = sysinfo::System::new();
    s.refresh_memory();
    s.total_memory() / 1_048_576
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &dest)?;
        } else {
            std::fs::copy(e.path(), &dest)?;
        }
    }
    Ok(())
}

/// A copy of an instance with its own files, worlds and settings.
#[tauri::command]
pub async fn mc_clone_instance(state: State<'_, AppState>, id: String, name: Option<String>) -> R<Instance> {
    let p = paths(&state);
    let src = mc::get_instance(&p, &id)?;
    let new_name = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("{} (copy)", src.name));
    let new_id = mc::new_instance_id(&p, &new_name);
    let (from, to) = (p.instance(&id), p.instance(&new_id));
    let to2 = to.clone();
    tokio::task::spawn_blocking(move || copy_dir(&from, &to2)).await.map_err(mc::e)?.map_err(mc::e)?;
    let _ = std::fs::remove_dir_all(to.join("natives"));
    let mut inst = src;
    inst.id = new_id;
    inst.name = new_name;
    inst.created = mc::now();
    inst.last_played = 0;
    inst.play_secs = 0;
    inst.session_start = 0;
    mc::save_instance(&p, &inst)?;
    Ok(inst)
}

/// Downloads the game files of an instance again: the version folder is removed and checked files are fetched anew.
#[tauri::command]
pub async fn mc_repair_instance(app: AppHandle, state: State<'_, AppState>, id: String) -> R<()> {
    let p = paths(&state);
    let mut inst = mc::get_instance(&p, &id)?;
    if launch::is_running(&id) {
        return Err("instance_running".into());
    }
    if !inst.version_id.is_empty() && !inst.version_id.contains(['/', '\\', '.']) {
        let _ = std::fs::remove_dir_all(p.versions().join(&inst.version_id));
    }
    let _ = std::fs::remove_dir_all(p.instance(&id).join("natives"));
    inst.version_id = String::new();
    mc::save_instance(&p, &inst)?;
    cancellable(&id, prepare(&app, &state, &inst)).await.map(|_| ())
}

fn zip_dir(dir: &std::path::Path, base: &std::path::Path, w: &mut zip::ZipWriter<std::fs::File>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let path = e.path();
        let rel = path.strip_prefix(base).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        if e.file_type()?.is_dir() {
            w.add_directory(format!("{rel}/"), zip::write::SimpleFileOptions::default())?;
            zip_dir(&path, base, w)?;
        } else {
            w.start_file(rel, zip::write::SimpleFileOptions::default())?;
            std::io::copy(&mut std::fs::File::open(&path)?, w)?;
        }
    }
    Ok(())
}

/// Packs the worlds of an instance into a zip next to it. Returns the file.
#[tauri::command]
pub async fn mc_backup_saves(state: State<'_, AppState>, id: String) -> R<String> {
    let p = paths(&state);
    mc::get_instance(&p, &id)?;
    let saves = mc::game_dir(&p, &id).join("saves");
    if !saves.is_dir() {
        return Err("no_worlds".into());
    }
    let dir = p.instance(&id).join("backups");
    std::fs::create_dir_all(&dir).map_err(mc::e)?;
    let file = dir.join(format!("worlds-{}.zip", mc::now()));
    let out = file.clone();
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let mut w = zip::ZipWriter::new(std::fs::File::create(&out)?);
        zip_dir(&saves, &saves, &mut w)?;
        w.finish()?;
        Ok(())
    })
    .await
    .map_err(mc::e)?
    .map_err(mc::e)?;
    Ok(file.to_string_lossy().into_owned())
}

#[derive(serde::Serialize)]
pub struct PackUpdate {
    pub available: bool,
    pub version: String,
}

/// Whether the Modrinth pack this instance came from has a newer version.
#[tauri::command]
pub async fn mc_check_pack_update(state: State<'_, AppState>, id: String) -> R<PackUpdate> {
    let p = paths(&state);
    let inst = mc::get_instance(&p, &id)?;
    let (Some(pid), false) = (inst.source.strip_prefix("modrinth:"), inst.source_version.is_empty()) else {
        return Ok(PackUpdate { available: false, version: String::new() });
    };
    let (latest, number) = modpack::latest_pack_version(&state.net.client(), pid).await?;
    Ok(PackUpdate { available: !latest.is_empty() && latest != inst.source_version, version: number })
}

/// Installs the newest version of the pack as a new instance, takes the worlds and settings over and keeps the old one.
#[tauri::command]
pub async fn mc_update_pack(app: AppHandle, state: State<'_, AppState>, id: String) -> R<Instance> {
    let p = paths(&state);
    let old = mc::get_instance(&p, &id)?;
    let pid = old.source.strip_prefix("modrinth:").ok_or("not a catalog pack")?.to_string();
    let client = state.net.client();
    let rep = reporter(&app, format!("update:{id}"));
    let key = format!("update:{id}");
    let (latest, _) = modpack::latest_pack_version(&client, &pid).await?;
    // the worlds are saved before anything is touched
    let _ = mc_backup_saves(state.clone(), id.clone()).await;
    let mut inst = cancellable(&key, modpack::install_modrinth_version(&client, &p, &latest, &format!("{} (updated)", old.name), &rep)).await?;
    inst.version_id = cancellable(&key, loader::ensure_version(&client, &p, &inst, &rep)).await?;
    inst.loader_version = share::loader_version_of(&inst);
    inst.source = old.source.clone();
    inst.memory_mb = old.memory_mb;
    inst.memory_custom = old.memory_custom;
    inst.extra_args = old.extra_args.clone();
    inst.java_path = old.java_path.clone();
    let (from, to) = (mc::game_dir(&p, &id), mc::game_dir(&p, &inst.id));
    for item in ["saves", "screenshots"] {
        if from.join(item).is_dir() {
            copy_dir(&from.join(item), &to.join(item)).map_err(mc::e)?;
        }
    }
    for item in ["options.txt", "servers.dat"] {
        if from.join(item).is_file() {
            let _ = std::fs::copy(from.join(item), to.join(item));
        }
    }
    mc::save_instance(&p, &inst)?;
    Ok(inst)
}

#[derive(serde::Serialize)]
pub struct CodeView {
    pub code: String,
    pub skipped: usize,
}

/// The share code of an instance: its mods by Modrinth version.
#[tauri::command]
pub async fn mc_export_code(state: State<'_, AppState>, id: String) -> R<CodeView> {
    let p = paths(&state);
    let inst = mc::get_instance(&p, &id)?;
    let (code, skipped) = catalog::export_code(&state.net.client(), &p, &inst).await?;
    Ok(CodeView { code, skipped })
}

#[derive(serde::Serialize)]
pub struct CodeImport {
    pub instance: Instance,
    /// Files that could not be downloaded.
    pub failed: usize,
}

/// Builds an instance from a share code.
#[tauri::command]
pub async fn mc_import_code(app: AppHandle, state: State<'_, AppState>, code: String) -> R<CodeImport> {
    let p = paths(&state);
    let client = state.net.client();
    let c = catalog::decode_code(&code)?;
    if !matches!(c.loader.as_str(), "vanilla" | "fabric" | "quilt" | "forge" | "neoforge") {
        return Err(format!("loader_unsupported:{}", c.loader));
    }
    let name = if c.name.trim().is_empty() { "Shared pack".to_string() } else { c.name.clone() };
    let mut inst = Instance { id: mc::new_instance_id(&p, &name), name, mc_version: c.mc.clone(), loader: c.loader.clone(), loader_version: c.lv.clone(), created: mc::now(), ..Default::default() };
    let rep = reporter(&app, "code".into());
    inst.version_id = cancellable("code", loader::ensure_version(&client, &p, &inst, &rep)).await?;
    inst.loader_version = share::loader_version_of(&inst);
    mc::save_instance(&p, &inst)?;
    let app2 = app.clone();
    let report = move |done: u64, total: u64| {
        let _ = app2.emit("mc-progress", Progress { id: "code".into(), stage: "mods".into(), done, total });
    };
    let failed = cancellable("code", async { catalog::install_code_files(&client, &p, &inst, &c, &report).await }).await?;
    Ok(CodeImport { instance: inst, failed })
}

#[tauri::command]
pub async fn mc_project(state: State<'_, AppState>, id: String) -> R<serde_json::Value> {
    catalog::project_detail(&state.net.client(), &id).await
}

#[tauri::command]
pub async fn mc_file_meta(state: State<'_, AppState>, id: String) -> R<Vec<catalog::FileMeta>> {
    let p = paths(&state);
    let inst = mc::get_instance(&p, &id)?;
    catalog::file_meta(&state.net.client(), &p, &inst).await
}

#[tauri::command]
pub fn mc_file_meta_cached(state: State<'_, AppState>, id: String) -> Vec<catalog::FileMeta> {
    catalog::cached_file_meta(&paths(&state), &id)
}
