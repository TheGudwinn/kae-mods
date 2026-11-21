//! Community content that lives in the project's repository as plain JSON files: news and mod collections.
use super::{err, AppState};
use tauri::State;

/// Where the files live. Only these two names can be asked for, nothing the UI sends becomes part of the URL.
const BASE: &str = "https://raw.githubusercontent.com/TheGudwinn/kae-mods/main/content";
const MAX_BYTES: usize = 256 * 1024;

fn file_for(kind: &str) -> Option<&'static str> {
    match kind {
        "news" => Some("news.json"),
        "collections" => Some("collections.json"),
        _ => None,
    }
}

/// Returns the raw JSON text of `news` or `collections`. The last good copy is kept on disk and used when
/// there is no connection, so the blocks do not go empty offline. The text is untrusted: the interface validates it.
#[tauri::command]
pub async fn fetch_content(state: State<'_, AppState>, kind: String) -> Result<String, String> {
    let name = file_for(&kind).ok_or("unknown_content")?;
    let cache = state.app_dir.join(format!("content-{name}"));
    let client = state.net.client();
    let fetched = async {
        let r = client.get(format!("{BASE}/{name}")).timeout(std::time::Duration::from_secs(8)).send().await.map_err(err)?;
        if r.status().as_u16() != 200 {
            return Err(format!("status {}", r.status().as_u16()));
        }
        let bytes = r.bytes().await.map_err(err)?;
        if bytes.len() > MAX_BYTES {
            return Err("content_too_big".to_string());
        }
        String::from_utf8(bytes.to_vec()).map_err(err)
    }
    .await;
    match fetched {
        Ok(text) => {
            let _ = std::fs::write(&cache, &text);
            Ok(text)
        }
        Err(e) => std::fs::read_to_string(&cache).map_err(|_| e),
    }
}
