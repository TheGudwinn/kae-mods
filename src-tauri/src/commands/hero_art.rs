//! Hero art for the heroes page, shrunk. The public asset API serves backgrounds at 4096 px (several MB each);
//! the poster shows them at a few hundred, so they are fetched once, scaled down and kept on disk.
use super::AppState;
use sha2::Digest;
use tauri::State;

const ALLOWED: &str = "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/";
const MAX_DOWNLOAD: usize = 24 * 1024 * 1024;

/// A hero picture from the asset API as a JPEG no wider than `width` (between 160 and 1600 px).
#[tauri::command]
pub async fn hero_art(state: State<'_, AppState>, url: String, width: u32) -> Result<tauri::ipc::Response, String> {
    if !url.starts_with(ALLOWED) || url.len() > 200 || url.contains("..") || !url.is_ascii() {
        return Err("bad_url".into());
    }
    let width = width.clamp(160, 1600);
    let dir = state.app_dir.join("hero-art");
    let key = hex::encode(&sha2::Sha256::digest(format!("{url}|{width}").as_bytes())[..10]);
    let file = dir.join(format!("{key}.jpg"));
    if let Ok(bytes) = std::fs::read(&file) {
        return Ok(tauri::ipc::Response::new(bytes));
    }
    let client = state.net.client();
    let raw = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    if raw.len() > MAX_DOWNLOAD {
        return Err("too_big".into());
    }
    let jpeg = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let img = image::load_from_memory(&raw).map_err(|e| e.to_string())?;
        let img = if img.width() > width { img.resize(width, u32::MAX, image::imageops::FilterType::Triangle) } else { img };
        // flatten transparency onto the dark of the poster, JPEG has no alpha
        let rgba = img.to_rgba8();
        let mut rgb = image::RgbImage::new(rgba.width(), rgba.height());
        for (x, y, p) in rgba.enumerate_pixels() {
            let a = p[3] as u32;
            let bg = [15u32, 26, 23];
            rgb.put_pixel(x, y, image::Rgb([0, 1, 2].map(|i| ((p[i] as u32 * a + bg[i] * (255 - a)) / 255) as u8)));
        }
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 82).encode_image(&rgb).map_err(|e| e.to_string())?;
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())??;
    if std::fs::create_dir_all(&dir).is_ok() {
        let _ = std::fs::write(&file, &jpeg);
    }
    Ok(tauri::ipc::Response::new(jpeg))
}
