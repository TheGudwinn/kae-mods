//! App self-update (Tauri updater). Packages are signed; the public key is baked into the app,
//! so the manifest location (GitHub or a mirror) is not trusted for integrity.
use super::{err, AppState};
use crate::net::config::{ProxyConfig, ProxyKind, ProxyMode};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::UpdaterExt;

#[derive(Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    pub notes: Option<String>,
}

#[derive(Serialize, Clone)]
struct UpdateProgress {
    downloaded: u64,
    total: Option<u64>,
}

/// Proxy URL for the updater's own HTTP client, or `None` to keep its default (system) behaviour.
fn proxy_url(p: &ProxyConfig) -> Option<url::Url> {
    if p.mode != ProxyMode::Custom || p.host.trim().is_empty() || p.port == 0 {
        return None;
    }
    let scheme = match p.kind {
        ProxyKind::Http => "http",
        ProxyKind::Https => "https",
        ProxyKind::Socks5 => "socks5h",
    };
    let mut u = url::Url::parse(&format!("{scheme}://{}:{}", p.host.trim(), p.port)).ok()?;
    if let Some(name) = p.username.as_deref().filter(|n| !n.is_empty()) {
        u.set_username(name).ok()?;
        u.set_password(p.password.as_deref()).ok()?;
    }
    Some(u)
}

async fn find_update(app: &AppHandle, state: &AppState) -> Result<Option<tauri_plugin_updater::Update>, String> {
    let (endpoints, proxy) = {
        let s = state.settings.lock().unwrap();
        (s.update_endpoints.clone(), s.net.proxy.clone())
    };
    if endpoints.is_empty() {
        return Err("updates_not_configured".into());
    }
    let urls = endpoints.iter().map(|e| url::Url::parse(e)).collect::<Result<Vec<_>, _>>().map_err(err)?;
    let mut b = app.updater_builder().endpoints(urls).map_err(err)?.timeout(std::time::Duration::from_secs(30));
    b = match (proxy.mode.clone(), proxy_url(&proxy)) {
        (ProxyMode::None, _) => b.no_proxy(),
        (_, Some(u)) => b.proxy(u),
        _ => b,
    };
    b.build().map_err(err)?.check().await.map_err(err)
}

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

/// False for the portable exe: only the installer leaves an uninstaller next to the app. An update always
/// goes through the installer, so a portable copy would stay old and reinstall on every start.
#[tauri::command]
pub fn is_installed() -> bool {
    std::env::current_exe().ok().and_then(|exe| exe.parent().map(|dir| dir.join("uninstall.exe").is_file())).unwrap_or(false)
}

/// `Ok(None)` = already up to date.
#[tauri::command]
pub async fn check_update(app: AppHandle, state: State<'_, AppState>) -> Result<Option<UpdateInfo>, String> {
    Ok(find_update(&app, &state).await?.map(|u| UpdateInfo { version: u.version.clone(), current: u.current_version.clone(), notes: u.body.clone() }))
}

/// Downloads and verifies the signature, stops the local database, then installs and restarts.
#[tauri::command]
pub async fn install_update(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    // No game-file operation may run while the app is being replaced.
    let _guard = state.busy.lock().await;
    let Some(update) = find_update(&app, &state).await? else { return Ok(()) };

    let app2 = app.clone();
    let mut downloaded = 0u64;
    let bytes = update
        .download(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = app2.emit("update-progress", UpdateProgress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(err)?; // signature is checked here, before anything is installed

    // On Windows the installer replaces the app and this process exits, so stop Postgres first.
    if let Some(db) = state.db.get() {
        let _ = db.shutdown().await;
    }
    update.install(bytes).map_err(|e| format!("update_install_failed:{e}"))?;
    app.restart()
}
