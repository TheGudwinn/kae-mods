//! Tauri commands. Thin wrappers: all logic lives in the library modules.
mod abilities;
mod capture;
mod hero_art;
mod posters;
mod content;
mod game;
mod hero;
mod library;
mod mod_updates;
mod profiles;
mod queue;
mod settings;
mod updates;

pub use abilities::*;
pub use capture::*;
pub use hero_art::*;
pub use posters::*;
pub use content::*;
pub use game::*;
pub use hero::*;
pub use library::*;
pub use mod_updates::*;
pub use profiles::*;
pub use queue::*;
pub use settings::*;
pub use updates::*;

use crate::db::Db;
use crate::net::NetService;
use crate::sources::cache::CachedSource;
use crate::sources::gamebanana::GameBanana;
use crate::sources::{Category, ModDetails, ModSource, SearchPage, SearchQuery};
use crate::settings::AppSettings;
use crate::steam::locate::{resolve_game, validate_game_dir, GameInstall};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use tokio::sync::{Mutex, OnceCell};

pub struct AppState {
    pub net: Arc<NetService>,
    pub gb: Arc<GameBanana>,
    pub app_dir: PathBuf,
    pub db: OnceCell<Db>,
    /// Serializes operations that change game files.
    pub busy: Mutex<()>,
    pub settings: std::sync::Mutex<AppSettings>,
    pub queue: Arc<crate::queue::Queue>,
}

impl AppState {
    pub fn new(net: Arc<NetService>, gb: Arc<GameBanana>, app_dir: PathBuf, settings: AppSettings) -> Self {
        Self { net, gb, app_dir, db: OnceCell::new(), busy: Mutex::new(()), settings: std::sync::Mutex::new(settings), queue: Arc::new(crate::queue::Queue::new()) }
    }

    pub fn settings_path(&self) -> PathBuf {
        self.app_dir.join("settings.json")
    }

    /// The game folder: the manual path from settings, else Steam auto-detection.
    /// A manual path that no longer points at Deadlock is an error, not a silent fallback.
    pub fn game(&self) -> Result<Option<GameInstall>, String> {
        let manual = self.settings.lock().unwrap().game_path.clone();
        resolve_game(manual.as_deref()).map_err(|e| e.to_string())
    }

    /// Starts the local Postgres on first use.
    pub async fn db(&self) -> Result<&Db, String> {
        self.db.get_or_try_init(|| Db::start(&self.app_dir)).await.map_err(|e| e.to_string())
    }

    /// The catalog source; wrapped with the offline cache once the database is up.
    pub fn source(&self) -> Arc<dyn ModSource> {
        match self.db.get() {
            Some(db) => Arc::new(CachedSource::new(self.gb.clone(), db.pool.clone())),
            None => self.gb.clone(),
        }
    }
}

pub(crate) fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
pub fn detect_game(state: State<'_, AppState>) -> Result<Option<GameInstall>, String> {
    state.game()
}

#[tauri::command]
pub fn check_game_path(path: String) -> Result<GameInstall, String> {
    validate_game_dir(&PathBuf::from(path)).map_err(err)
}

#[tauri::command]
pub async fn search_mods(state: State<'_, AppState>, query: SearchQuery) -> Result<SearchPage, String> {
    state.source().search(&query).await.map_err(err)
}

#[tauri::command]
pub async fn mod_details(state: State<'_, AppState>, id: String) -> Result<ModDetails, String> {
    state.source().get_mod_details(&id).await.map_err(err)
}

#[tauri::command]
pub async fn list_categories(state: State<'_, AppState>, parent: Option<i64>) -> Result<Vec<Category>, String> {
    state.source().categories(parent).await.map_err(err)
}

/// Hero card pictures (280x380) keyed by a simplified hero name. GameBanana's own category icons are only 40x40.
/// The list is large, so it is fetched once per run; on failure nothing is cached and the caller falls back.
#[tauri::command]
pub async fn hero_portraits(state: State<'_, AppState>) -> Result<std::collections::HashMap<String, String>, String> {
    static CACHE: tokio::sync::OnceCell<std::collections::HashMap<String, String>> = tokio::sync::OnceCell::const_new();
    let client = state.net.client();
    CACHE
        .get_or_try_init(|| async move {
            let v: serde_json::Value = client
                .get("https://api.deadlock-api.com/v1/assets/heroes")
                .timeout(std::time::Duration::from_secs(20))
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?
                .json()
                .await
                .map_err(|e| e.to_string())?;
            let mut out = std::collections::HashMap::new();
            for h in v.as_array().into_iter().flatten() {
                let name = h.get("name").and_then(|x| x.as_str());
                let img = h.pointer("/images/icon_hero_card_webp").and_then(|x| x.as_str());
                if let (Some(n), Some(i)) = (name, img) {
                    out.insert(hero_key(n), i.to_string());
                }
            }
            Ok::<_, String>(out)
        })
        .await
        .cloned()
}

/// "The Doorman" and "Doorman" are the same hero; punctuation and case do not matter.
pub(crate) fn hero_key(name: &str) -> String {
    let l = name.to_lowercase();
    l.strip_prefix("the ").unwrap_or(&l).chars().filter(|c| c.is_alphanumeric()).collect()
}
