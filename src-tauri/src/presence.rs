//! Discord Rich Presence: shows what the launcher is doing ("Browsing the catalog", "Playing Minecraft").
//!
//! The Discord application id is the constant [`APP_ID`]. It can be replaced without editing code: a
//! `discord_app_id.txt` file in the app folder wins, then the `KAE_DISCORD_APP_ID` build variable.
//! With no id, or without Discord running, every call is a quiet no-op: presence never gets in the
//! way of the launcher.
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::path::Path;
use std::sync::Mutex;

/// Application id of "Kae Mods" from discord.com/developers/applications (a public number, not a secret).
/// Used unless a file or build variable overrides it.
pub const APP_ID: &str = "1574491238780903515";

/// Logo shown in the status. Discord loads a public https image (png/jpg) by its address, so nothing has
/// to be uploaded to the developer portal.
pub const LOGO_URL: &str = "https://keamods.ru/img/logo.png";
pub const LOGO_TEXT: &str = "Kae Mods";

static CLIENT: Mutex<Option<DiscordIpcClient>> = Mutex::new(None);
static STARTED: Mutex<Option<i64>> = Mutex::new(None);

fn app_id(app_dir: &Path) -> Option<String> {
    let from_file = std::fs::read_to_string(app_dir.join("discord_app_id.txt")).ok().map(|s| s.trim().to_string());
    from_file
        .filter(|s| !s.is_empty())
        .or_else(|| option_env!("KAE_DISCORD_APP_ID").map(str::to_string))
        .or_else(|| Some(APP_ID.to_string()))
        .filter(|s| !s.is_empty())
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Whether an application id is configured at all (the settings page hides the switch otherwise).
pub fn available(app_dir: &Path) -> bool {
    app_id(app_dir).is_some()
}

/// Shows `details` and `state`. `timer` starts a "elapsed" counter that keeps running until `clear`.
pub fn set(app_dir: &Path, details: &str, state: Option<&str>, timer: bool) {
    let Some(id) = app_id(app_dir) else { return };
    let Ok(mut guard) = CLIENT.lock() else { return };
    if guard.is_none() {
        let mut c = DiscordIpcClient::new(&id);
        if c.connect().is_err() {
            return; // Discord is not running; the next update tries again
        }
        *guard = Some(c);
    }
    let started = {
        let mut s = STARTED.lock().unwrap_or_else(|e| e.into_inner());
        if timer {
            *s.get_or_insert_with(now)
        } else {
            *s = None;
            0
        }
    };
    let mut act = activity::Activity::new()
        .details(details)
        .assets(activity::Assets::new().large_image(LOGO_URL).large_text(LOGO_TEXT));
    if let Some(s) = state {
        act = act.state(s);
    }
    if timer {
        act = act.timestamps(activity::Timestamps::new().start(started));
    }
    if guard.as_mut().map(|c| c.set_activity(act).is_err()).unwrap_or(false) {
        *guard = None; // the pipe broke (Discord closed); reconnect on the next update
    }
}

pub fn clear() {
    if let Ok(mut g) = CLIENT.lock() {
        if let Some(c) = g.as_mut() {
            let _ = c.close();
        }
        *g = None;
    }
}
