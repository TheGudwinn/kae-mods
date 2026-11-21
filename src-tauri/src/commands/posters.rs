//! Poster material for the heroes page: the hero's name, tags, short lore, colour and pictures from the public
//! asset API, in the language of the app. Names are matched to the English list by hero id.
use super::AppState;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tauri::State;

const URL: &str = "https://api.deadlock-api.com/v1/assets/heroes";

#[derive(Serialize, Clone)]
pub struct HeroPoster {
    pub name: String,
    pub tags: Vec<String>,
    pub role: String,
    /// The first paragraph of the lore, plain text.
    pub lore: String,
    /// The hero's own colour, "#rrggbb".
    pub color: Option<String>,
    /// Large background art.
    pub background: Option<String>,
    /// A cut-out portrait with a transparent background.
    pub cutout: Option<String>,
}

type Posters = HashMap<String, HeroPoster>;

fn cache() -> &'static tokio::sync::Mutex<HashMap<String, Arc<Posters>>> {
    static C: OnceLock<tokio::sync::Mutex<HashMap<String, Arc<Posters>>>> = OnceLock::new();
    C.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

/// Text from the API carries line breaks as <br> and may carry other markup; only the first paragraph is kept.
fn plain_first_paragraph(html: &str) -> String {
    let first = html.split("<br>").map(str::trim).find(|p| !p.is_empty()).unwrap_or("");
    let mut out = String::new();
    let mut in_tag = false;
    for c in first.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out.trim().to_string();
    if out.chars().count() > 420 {
        let cut: String = out.chars().take(420).collect();
        format!("{}…", cut.trim_end())
    } else {
        out
    }
}

fn s(v: &serde_json::Value, ptr: &str) -> Option<String> {
    v.pointer(ptr).and_then(|x| x.as_str()).map(str::to_string)
}

/// Poster data by simplified English hero name (the same key `hero_portraits` uses), in `lang`
/// (`russian` or `english`). Fetched once per language and run.
#[tauri::command]
pub async fn hero_posters(state: State<'_, AppState>, lang: String) -> Result<Posters, String> {
    let lang = if lang == "russian" { "russian" } else { "english" };
    let mut guard = cache().lock().await;
    if let Some(p) = guard.get(lang) {
        return Ok((**p).clone());
    }
    let client = state.net.client();
    let get = |url: String| {
        let client = client.clone();
        async move {
            client
                .get(url)
                .timeout(std::time::Duration::from_secs(40))
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?
                .json::<serde_json::Value>()
                .await
                .map_err(|e| e.to_string())
        }
    };
    let en = get(URL.to_string()).await?;
    let local = if lang == "english" { en.clone() } else { get(format!("{URL}?language={lang}")).await? };
    let by_id: HashMap<i64, &serde_json::Value> = local.as_array().into_iter().flatten().filter_map(|h| Some((h.get("id")?.as_i64()?, h))).collect();
    let mut out = Posters::new();
    for h in en.as_array().into_iter().flatten() {
        let (Some(id), Some(en_name)) = (h.get("id").and_then(|v| v.as_i64()), s(h, "/name")) else { continue };
        let l = by_id.get(&id).copied().unwrap_or(h);
        let tags = l.get("tags").and_then(|t| t.as_array()).map(|a| a.iter().filter_map(|x| x.as_str()).take(4).map(str::to_string).collect()).unwrap_or_default();
        out.insert(
            super::hero_key(&en_name),
            HeroPoster {
                name: s(l, "/name").unwrap_or(en_name),
                tags,
                role: s(l, "/description/role").unwrap_or_default(),
                lore: plain_first_paragraph(&s(l, "/description/lore").unwrap_or_default()),
                color: s(h, "/colors/style_hex"),
                background: s(h, "/images/background_image_webp"),
                cutout: s(h, "/images/hero_card_gloat_webp").or_else(|| s(h, "/images/icon_hero_card_webp")),
            },
        );
    }
    let out = Arc::new(out);
    guard.insert(lang.to_string(), out.clone());
    Ok((*out).clone())
}
