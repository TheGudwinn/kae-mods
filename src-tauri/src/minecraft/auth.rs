//! Signing in with a Microsoft account: device code, Xbox Live, XSTS, Minecraft services.
//!
//! The person opens a page, enters a code and signs in there; this app never sees the password. The
//! tokens are kept in the Minecraft config file of the app folder and renewed before a start.
//! It needs the application id of an app registered by the developer (`Config::ms_client_id`, or the
//! build variable `KAE_MS_CLIENT_ID`). Mojang must also approve that application for the Minecraft services.
use super::{e, now, Account, Config, MsAccount, Result};
use serde::Serialize;
use serde_json::{json, Value};

/// Application (client) id of the "Kae Mods" registration in Microsoft Entra. It is public, not a secret.
/// A different one can be set in the settings or with the build variable `KAE_MS_CLIENT_ID`.
pub const CLIENT_ID: &str = "087668ac-2c39-49a9-b91f-830c41d607df";

const SCOPE: &str = "XboxLive.signin offline_access";
const DEVICE: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";

/// A POST with a form body (the client is built without the form feature).
fn post_form(client: &reqwest::Client, url: &str, pairs: &[(&str, &str)]) -> reqwest::RequestBuilder {
    let body = url::form_urlencoded::Serializer::new(String::new()).extend_pairs(pairs).finish();
    client.post(url).header("Content-Type", "application/x-www-form-urlencoded").body(body)
}

/// The application id to sign in with, if any is set.
pub fn client_id(cfg: &Config) -> Option<String> {
    let own = cfg.ms_client_id.trim();
    if !own.is_empty() {
        return Some(own.to_string());
    }
    Some(option_env!("KAE_MS_CLIENT_ID").filter(|s| !s.is_empty()).unwrap_or(CLIENT_ID).to_string())
}

#[derive(Debug, Serialize)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub device_code: String,
    pub interval: u64,
    pub expires_in: u64,
}

/// Step one: ask for a code the person types on the Microsoft page.
pub async fn start(client: &reqwest::Client, cid: &str) -> Result<DeviceCode> {
    let r = post_form(client, DEVICE, &[("client_id", cid), ("scope", SCOPE)]).send().await.map_err(e)?;
    let v: Value = r.json().await.map_err(e)?;
    if let Some(err) = v["error"].as_str() {
        return Err(format!("ms_error:{err}"));
    }
    Ok(DeviceCode {
        user_code: v["user_code"].as_str().ok_or("no code")?.into(),
        verification_uri: v["verification_uri"].as_str().unwrap_or("https://microsoft.com/link").into(),
        device_code: v["device_code"].as_str().ok_or("no code")?.into(),
        interval: v["interval"].as_u64().unwrap_or(5).max(2),
        expires_in: v["expires_in"].as_u64().unwrap_or(900),
    })
}

/// Step two, repeated: `None` while the person has not finished, the account when they have.
pub async fn poll(client: &reqwest::Client, cid: &str, device_code: &str) -> Result<Option<MsAccount>> {
    let r = post_form(client, TOKEN, &[("grant_type", "urn:ietf:params:oauth:grant-type:device_code"), ("client_id", cid), ("device_code", device_code)])
        .send()
        .await
        .map_err(e)?;
    let v: Value = r.json().await.map_err(e)?;
    match v["error"].as_str() {
        Some("authorization_pending" | "slow_down") => return Ok(None),
        Some(other) => return Err(format!("ms_error:{other}")),
        None => {}
    }
    let access = v["access_token"].as_str().ok_or("no token")?;
    let refresh = v["refresh_token"].as_str().unwrap_or("");
    login_chain(client, access, refresh).await.map(Some)
}

/// A new Minecraft token from the refresh token.
async fn refresh(client: &reqwest::Client, cid: &str, old: &MsAccount) -> Result<MsAccount> {
    let r = post_form(client, TOKEN, &[("grant_type", "refresh_token"), ("client_id", cid), ("refresh_token", old.refresh_token.as_str()), ("scope", SCOPE)]).send().await.map_err(e)?;
    let v: Value = r.json().await.map_err(e)?;
    let access = v["access_token"].as_str().ok_or("ms_session_expired")?;
    login_chain(client, access, v["refresh_token"].as_str().unwrap_or(&old.refresh_token)).await
}

/// Microsoft token -> Xbox Live -> XSTS -> Minecraft token and profile.
async fn login_chain(client: &reqwest::Client, access: &str, refresh_token: &str) -> Result<MsAccount> {
    let xbl: Value = client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .json(&json!({ "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={access}") }, "RelyingParty": "http://auth.xboxlive.com", "TokenType": "JWT" }))
        .send()
        .await
        .map_err(e)?
        .json()
        .await
        .map_err(e)?;
    let xbl_token = xbl["Token"].as_str().ok_or("ms_xbox_failed")?;
    let uhs = xbl["DisplayClaims"]["xui"][0]["uhs"].as_str().ok_or("ms_xbox_failed")?;

    let r = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .json(&json!({ "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] }, "RelyingParty": "rp://api.minecraftservices.com/", "TokenType": "JWT" }))
        .send()
        .await
        .map_err(e)?;
    let xsts: Value = r.json().await.map_err(e)?;
    if let Some(code) = xsts["XErr"].as_u64() {
        return Err(match code {
            2148916233 => "ms_no_xbox_account",
            2148916235 => "ms_region",
            2148916238 => "ms_child_account",
            _ => "ms_xbox_failed",
        }
        .into());
    }
    let xsts_token = xsts["Token"].as_str().ok_or("ms_xbox_failed")?;

    let r = client.post("https://api.minecraftservices.com/authentication/login_with_xbox").json(&json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") })).send().await.map_err(e)?;
    if r.status().as_u16() == 403 {
        return Err("ms_app_not_approved".into());
    }
    let mc: Value = r.json().await.map_err(e)?;
    let mc_token = mc["access_token"].as_str().ok_or("ms_minecraft_failed")?;
    let expires = mc["expires_in"].as_u64().unwrap_or(86400);

    let r = client.get("https://api.minecraftservices.com/minecraft/profile").bearer_auth(mc_token).send().await.map_err(e)?;
    if r.status().as_u16() == 404 {
        return Err("ms_no_game".into());
    }
    let profile: Value = r.json().await.map_err(e)?;
    Ok(MsAccount {
        name: profile["name"].as_str().ok_or("ms_no_game")?.into(),
        uuid: profile["id"].as_str().ok_or("ms_no_game")?.into(),
        refresh_token: refresh_token.into(),
        mc_token: mc_token.into(),
        expires_at: now() + expires.saturating_sub(60),
    })
}

/// The account to start the game with: the Microsoft one with a token that is still good, or as it is when there is none.
pub async fn fresh(client: &reqwest::Client, cfg: &Config) -> Result<(Account, Option<MsAccount>)> {
    let mut account = cfg.account.clone();
    let Some(ms) = account.ms.clone() else { return Ok((account, None)) };
    if ms.expires_at > now() + 120 {
        return Ok((account, None));
    }
    let cid = client_id(cfg).ok_or("ms_session_expired")?;
    let renewed = refresh(client, &cid, &ms).await.map_err(|_| "ms_session_expired".to_string())?;
    account.ms = Some(renewed.clone());
    Ok((account, Some(renewed)))
}
