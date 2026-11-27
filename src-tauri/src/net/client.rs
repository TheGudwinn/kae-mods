use super::config::{NetConfig, ProxyKind, ProxyMode};
use crate::error::NetError;
use std::time::Duration;

/// Build the single HTTP client used by every request in the app.
pub fn build_client(cfg: &NetConfig) -> Result<reqwest::Client, NetError> {
    let mut b = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(cfg.connect_timeout_secs))
        .read_timeout(Duration::from_secs(cfg.read_timeout_secs))
        .user_agent(concat!("kae-mods/", env!("CARGO_PKG_VERSION")))
        .redirect(redirect_policy());

    match cfg.proxy.mode {
        ProxyMode::System => {}
        ProxyMode::None => b = b.no_proxy(),
        ProxyMode::Custom => {
            let p = &cfg.proxy;
            if p.host.trim().is_empty() || p.port == 0 {
                return Err(NetError::BadProxy("host and port are required".into()));
            }
            let scheme = match p.kind {
                ProxyKind::Http => "http",
                ProxyKind::Https => "https",
                ProxyKind::Socks5 => "socks5h", // resolve DNS through the proxy
            };
            let mut proxy = reqwest::Proxy::all(format!("{scheme}://{}:{}", p.host.trim(), p.port))
                .map_err(|e| NetError::BadProxy(e.to_string()))?;
            if let Some(u) = p.username.as_deref().filter(|u| !u.is_empty()) {
                proxy = proxy.basic_auth(u, p.password.as_deref().unwrap_or(""));
            }
            b = b.proxy(proxy);
        }
    }
    Ok(b.build()?)
}

/// Follow a few redirects, but only to http(s) and never from https down to plain http.
fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 5 {
            return attempt.error("too many redirects");
        }
        let to = attempt.url().scheme().to_string();
        let downgrade = to == "http" && attempt.previous().last().is_some_and(|u| u.scheme() == "https");
        if (to != "http" && to != "https") || downgrade {
            attempt.error("redirect to an insecure or unsupported scheme")
        } else {
            attempt.follow()
        }
    })
}
