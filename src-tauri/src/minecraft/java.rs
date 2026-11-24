//! Finding a suitable Java, or downloading one (Eclipse Temurin) when the system has none.
use super::{e, Paths, Report, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

fn exe() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}

/// Major version printed by `java -version`, e.g. 8 for "1.8.0_392" and 21 for "21.0.2".
fn major_of(path: &Path) -> Option<u32> {
    let mut cmd = Command::new(path);
    cmd.arg("-version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // no console window
    }
    let out = cmd.output().ok()?;
    let text = String::from_utf8_lossy(&out.stderr).to_string() + &String::from_utf8_lossy(&out.stdout);
    let q = text.split('"').nth(1)?;
    let mut parts = q.split(['.', '_', '-', '+']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 { parts.next()?.parse().ok() } else { Some(first) }
}

fn managed_dir(p: &Paths, major: u32) -> PathBuf {
    p.runtime().join(format!("java-{major}"))
}

fn find_in(dir: &Path) -> Option<PathBuf> {
    let direct = dir.join("bin").join(exe());
    if direct.exists() {
        return Some(direct);
    }
    // the archive holds one top folder
    std::fs::read_dir(dir).ok()?.flatten().map(|d| d.path().join("bin").join(exe())).find(|p| p.exists())
}

/// A Java that can run `major`, if one is already on this computer.
pub fn find_java(p: &Paths, major: u32, custom: Option<&str>) -> Option<PathBuf> {
    if let Some(c) = custom.filter(|c| !c.is_empty()) {
        let c = PathBuf::from(c);
        return c.exists().then_some(c);
    }
    if let Some(m) = find_in(&managed_dir(p, major)) {
        return Some(m);
    }
    let mut candidates: Vec<PathBuf> = vec![];
    if let Some(h) = std::env::var_os("JAVA_HOME") {
        candidates.push(PathBuf::from(h).join("bin").join(exe()));
    }
    candidates.push(PathBuf::from(exe()));
    for c in candidates {
        // old games want exactly 8; new ones accept anything at least as new as required
        if let Some(m) = major_of(&c) {
            if m == major || (major > 8 && m >= major) {
                return Some(c);
            }
        }
    }
    None
}

/// Downloads a Java runtime of the given major version into the app folder.
pub async fn download_java(client: &reqwest::Client, p: &Paths, major: u32, report: Report<'_>) -> Result<PathBuf> {
    if !cfg!(windows) {
        return Err(format!("Java {major} is not installed; install it from your package manager"));
    }
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x64" };
    let url = format!("https://api.adoptium.net/v3/binary/latest/{major}/ga/windows/{arch}/jre/hotspot/normal/eclipse");
    report("java", 0, 1);
    let r = client.get(&url).send().await.map_err(e)?;
    if !r.status().is_success() {
        return Err(format!("Java {major} download failed: http {}", r.status().as_u16()));
    }
    let total = r.content_length().unwrap_or(0);
    use futures_util::StreamExt;
    let mut stream = r.bytes_stream();
    let mut data: Vec<u8> = Vec::with_capacity(total as usize);
    while let Some(chunk) = stream.next().await {
        data.extend_from_slice(&chunk.map_err(e)?);
        report("java", data.len() as u64, total.max(data.len() as u64));
    }
    let dir = managed_dir(p, major);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(e)?;
    let dir2 = dir.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(e)?;
        for i in 0..z.len() {
            let mut f = z.by_index(i).map_err(e)?;
            // `enclosed_name` drops entries that try to leave the folder
            let Some(rel) = f.enclosed_name() else { continue };
            let out = dir2.join(rel);
            if f.is_dir() {
                std::fs::create_dir_all(&out).map_err(e)?;
            } else {
                if let Some(parent) = out.parent() {
                    std::fs::create_dir_all(parent).map_err(e)?;
                }
                let mut o = std::fs::File::create(&out).map_err(e)?;
                std::io::copy(&mut f, &mut o).map_err(e)?;
            }
        }
        Ok(())
    })
    .await
    .map_err(e)??;
    find_in(&dir).ok_or_else(|| "the downloaded Java has no java.exe".to_string())
}

/// Finds Java for `major` or downloads it.
pub async fn ensure_java(client: &reqwest::Client, p: &Paths, major: u32, custom: Option<&str>, report: Report<'_>) -> Result<PathBuf> {
    if let Some(j) = find_java(p, major, custom) {
        return Ok(j);
    }
    download_java(client, p, major, report).await
}

#[derive(Debug, serde::Serialize)]
pub struct JavaState {
    pub major: u32,
    pub found: bool,
    pub path: Option<String>,
}

pub fn java_state(p: &Paths, major: u32) -> JavaState {
    let f = find_java(p, major, None);
    JavaState { major, found: f.is_some(), path: f.map(|x| x.to_string_lossy().into_owned()) }
}
