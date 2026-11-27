//! User settings persisted as `settings.json` in the app data folder.
//!
//! The proxy password is stored in this file in plain text: there is no OS keyring
//! integration yet. The file lives in the per-user app data folder and is never exported or logged.
use crate::net::config::{NetConfig, ProxyKind, ProxyMode};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SOURCE_GAMEBANANA: &str = "gamebanana";

/// Releases of this repository are the update source. `DDMM_REPO` (set at compile time) lets a fork point
/// its builds at its own releases.
const UPDATE_REPO: &str = "TheGudwinn/kae-mods";

/// `latest.json` locations tried in order.
pub fn default_update_endpoints() -> Vec<String> {
    let repo = option_env!("DDMM_REPO").filter(|r| !r.is_empty()).unwrap_or(UPDATE_REPO);
    vec![format!("https://github.com/{repo}/releases/latest/download/latest.json")]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub net: NetConfig,
    /// Manually chosen Deadlock folder; overrides auto-detection.
    pub game_path: Option<String>,
    /// Check for a new app version at startup. The only request made is the manifest download.
    pub check_updates: bool,
    /// Update manifest URLs (GitHub Releases first, then mirrors). Packages are verified against the
    /// public key built into the app, so a mirror cannot deliver unsigned code.
    pub update_endpoints: Vec<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self { net: NetConfig::default(), game_path: None, check_updates: true, update_endpoints: default_update_endpoints() }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SettingsError {
    #[error("mirror list is empty")]
    NoMirrors,
    #[error("too many mirrors (max 8)")]
    TooManyMirrors,
    #[error("invalid mirror address: {0}")]
    BadMirror(String),
    #[error("proxy host and port are required")]
    ProxyIncomplete,
    #[error("proxy host must be a bare host name or address: {0}")]
    BadProxyHost(String),
}

/// What the settings screen sends back. `password: None` keeps the stored one, `Some("")` clears it.
#[derive(Debug, Clone, Deserialize)]
pub struct ProxyInput {
    pub mode: ProxyMode,
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SettingsInput {
    pub proxy: ProxyInput,
    /// Base URLs for GameBanana, primary first.
    pub mirrors: Vec<String>,
    pub game_path: Option<String>,
    #[serde(default)]
    pub check_updates: bool,
    #[serde(default)]
    pub update_endpoints: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProxyView {
    pub mode: ProxyMode,
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub has_password: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingsView {
    pub proxy: ProxyView,
    pub mirrors: Vec<String>,
    pub game_path: Option<String>,
    pub check_updates: bool,
    pub update_endpoints: Vec<String>,
}

impl AppSettings {
    pub fn view(&self) -> SettingsView {
        let p = &self.net.proxy;
        SettingsView {
            proxy: ProxyView {
                mode: p.mode.clone(),
                kind: p.kind.clone(),
                host: p.host.clone(),
                port: p.port,
                username: p.username.clone(),
                has_password: p.password.as_deref().is_some_and(|s| !s.is_empty()),
            },
            mirrors: self.net.mirrors.get(SOURCE_GAMEBANANA).cloned().unwrap_or_default(),
            game_path: self.game_path.clone(),
            check_updates: self.check_updates,
            update_endpoints: self.update_endpoints.clone(),
        }
    }

    /// Returns the updated settings; `self` is untouched when validation fails.
    pub fn with_input(&self, input: SettingsInput) -> Result<AppSettings, SettingsError> {
        let mut next = self.clone();

        let mirrors = clean_mirrors(&input.mirrors)?;
        next.net.mirrors.insert(SOURCE_GAMEBANANA.to_string(), mirrors);

        let p = input.proxy;
        let host = p.host.trim().to_string();
        if p.mode == ProxyMode::Custom {
            if host.is_empty() || p.port == 0 {
                return Err(SettingsError::ProxyIncomplete);
            }
            if host.contains("://") || host.contains('/') || host.contains('@') || host.chars().any(char::is_whitespace) {
                return Err(SettingsError::BadProxyHost(host));
            }
        }
        let old = &self.net.proxy;
        let username = p.username.map(|u| u.trim().to_string()).filter(|u| !u.is_empty());
        let password = match (&username, p.password) {
            (None, _) => None,
            (Some(_), Some(pw)) if pw.is_empty() => None,
            (Some(_), Some(pw)) => Some(pw),
            (Some(_), None) => old.password.clone(),
        };
        next.net.proxy = crate::net::config::ProxyConfig { mode: p.mode, kind: p.kind, host, port: p.port, username, password };

        next.check_updates = input.check_updates;
        // Empty is allowed: it means "no update source configured".
        next.update_endpoints = if input.update_endpoints.iter().all(|s| s.trim().is_empty()) {
            Vec::new()
        } else {
            clean_mirrors(&input.update_endpoints)?
        };

        next.game_path = input.game_path.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        Ok(next)
    }
}

/// Trims, drops blanks and duplicates, and enforces https (plain http only for loopback).
pub fn clean_mirrors(raw: &[String]) -> Result<Vec<String>, SettingsError> {
    let mut out: Vec<String> = Vec::new();
    for item in raw.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let url = url::Url::parse(item).map_err(|_| SettingsError::BadMirror(item.to_string()))?;
        let host = url.host_str().ok_or_else(|| SettingsError::BadMirror(item.to_string()))?;
        let loopback = matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1");
        let scheme_ok = url.scheme() == "https" || (url.scheme() == "http" && loopback);
        if !scheme_ok || !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some() {
            return Err(SettingsError::BadMirror(item.to_string()));
        }
        let norm = item.trim_end_matches('/').to_string();
        if !out.contains(&norm) {
            out.push(norm);
        }
    }
    if out.is_empty() {
        return Err(SettingsError::NoMirrors);
    }
    if out.len() > 8 {
        return Err(SettingsError::TooManyMirrors);
    }
    Ok(out)
}

/// Missing file -> defaults. A corrupt file is moved aside (`settings.json.bad`) and defaults are used,
/// so a broken edit never blocks the app from starting.
pub fn load(path: &Path) -> AppSettings {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let mut settings: AppSettings = serde_json::from_str(&text).unwrap_or_else(|e| {
                eprintln!("settings.json is invalid ({e}); using defaults");
                let _ = std::fs::rename(path, path.with_extension("json.bad"));
                AppSettings::default()
            });
            // older builds had no built-in source and saved an empty list; switching updates off is `check_updates`
            if settings.update_endpoints.is_empty() {
                settings.update_endpoints = default_update_endpoints();
            }
            settings
        }
        Err(_) => AppSettings::default(),
    }
}

/// Atomic write: temp file in the same folder, then rename.
pub fn save(path: &Path, s: &AppSettings) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(s).map_err(std::io::Error::other)?)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(mode: ProxyMode) -> SettingsInput {
        SettingsInput {
            proxy: ProxyInput { mode, kind: ProxyKind::Socks5, host: "127.0.0.1".into(), port: 1080, username: None, password: None },
            mirrors: vec!["https://gamebanana.com/".into(), "https://mirror.example.org".into()],
            game_path: None,
            check_updates: true,
            update_endpoints: vec![],
        }
    }

    #[test]
    fn mirrors_are_normalised() {
        let m = clean_mirrors(&["  https://a.example/ ".into(), "https://a.example".into(), "".into()]).unwrap();
        assert_eq!(m, vec!["https://a.example"]);
    }

    #[test]
    fn mirrors_reject_bad_schemes_and_credentials() {
        for bad in ["http://gamebanana.com", "ftp://x.org", "https://u:p@x.org", "not a url", "https://x.org/?a=1", "file:///c:/x"] {
            assert!(matches!(clean_mirrors(&[bad.into()]), Err(SettingsError::BadMirror(_))), "{bad}");
        }
        assert!(clean_mirrors(&["http://127.0.0.1:8080".into()]).is_ok());
        assert_eq!(clean_mirrors(&[]), Err(SettingsError::NoMirrors));
    }

    #[test]
    fn custom_proxy_needs_host_and_port() {
        let base = AppSettings::default();
        let mut i = input(ProxyMode::Custom);
        i.proxy.port = 0;
        assert_eq!(base.with_input(i).unwrap_err(), SettingsError::ProxyIncomplete);
        let mut i = input(ProxyMode::Custom);
        i.proxy.host = "http://evil/".into();
        assert!(matches!(base.with_input(i), Err(SettingsError::BadProxyHost(_))));
        // Unused proxy fields are not validated when the mode is not custom.
        let mut i = input(ProxyMode::None);
        i.proxy.host = String::new();
        assert!(base.with_input(i).is_ok());
    }

    #[test]
    fn password_is_kept_cleared_or_replaced() {
        let mut s = AppSettings::default();
        let mut i = input(ProxyMode::Custom);
        i.proxy.username = Some("u".into());
        i.proxy.password = Some("secret".into());
        s = s.with_input(i).unwrap();
        assert!(s.view().proxy.has_password);

        let mut keep = input(ProxyMode::Custom);
        keep.proxy.username = Some("u".into());
        s = s.with_input(keep).unwrap();
        assert_eq!(s.net.proxy.password.as_deref(), Some("secret"));

        let mut clear = input(ProxyMode::Custom);
        clear.proxy.username = Some("u".into());
        clear.proxy.password = Some(String::new());
        s = s.with_input(clear).unwrap();
        assert!(!s.view().proxy.has_password);

        // Dropping the user name drops the password with it.
        let mut i = input(ProxyMode::Custom);
        i.proxy.username = Some("u".into());
        i.proxy.password = Some("again".into());
        s = s.with_input(i).unwrap();
        s = s.with_input(input(ProxyMode::Custom)).unwrap();
        assert!(s.net.proxy.password.is_none());
    }

    #[test]
    fn update_endpoints_may_be_empty_but_never_plain_http() {
        let base = AppSettings::default();
        assert!(base.with_input(input(ProxyMode::None)).unwrap().update_endpoints.is_empty());

        let mut i = input(ProxyMode::None);
        i.update_endpoints = vec![" https://github.com/o/r/releases/latest/download/latest.json ".into(), "https://mirror.example/latest.json".into()];
        let s = base.with_input(i).unwrap();
        assert_eq!(s.update_endpoints.len(), 2);
        assert!(s.update_endpoints[0].starts_with("https://github.com/"));

        let mut bad = input(ProxyMode::None);
        bad.update_endpoints = vec!["http://evil.example/latest.json".into()];
        assert!(matches!(base.with_input(bad), Err(SettingsError::BadMirror(_))));
    }

    #[test]
    fn view_never_contains_the_password() {
        let mut s = AppSettings::default();
        s.net.proxy.password = Some("secret".into());
        assert!(!serde_json::to_string(&s.view()).unwrap().contains("secret"));
    }

    #[test]
    fn save_load_roundtrip_and_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert!(load(&path).game_path.is_none());

        let s = AppSettings::default().with_input({
            let mut i = input(ProxyMode::None);
            i.game_path = Some(" D:/Games/Deadlock ".into());
            i
        });
        let s = s.unwrap();
        save(&path, &s).unwrap();
        let back = load(&path);
        assert_eq!(back.game_path.as_deref(), Some("D:/Games/Deadlock"));
        assert_eq!(back.net.mirrors["gamebanana"].len(), 2);

        std::fs::write(&path, "{ not json").unwrap();
        assert!(load(&path).game_path.is_none());
        assert!(dir.path().join("settings.json.bad").exists());
    }

    #[test]
    fn old_files_with_missing_fields_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"game_path":"X"}"#).unwrap();
        let s = load(&path);
        assert_eq!(s.game_path.as_deref(), Some("X"));
        assert_eq!(s.net.max_retries, 5);
    }
}
