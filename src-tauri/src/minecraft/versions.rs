//! Game versions: the Mojang manifest, mod loader profiles, and downloading a version with its libraries and assets.
use super::{e, fetch_file, get_json, Paths, Report, Result};
use futures_util::{stream, StreamExt};
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const FABRIC_META: &str = "https://meta.fabricmc.net/v2";
const QUILT_META: &str = "https://meta.quiltmc.org/v3";

#[derive(Debug, Clone, Serialize)]
pub struct GameVersion {
    pub id: String,
    /// `release`, `snapshot`, `old_beta`, `old_alpha`
    pub kind: String,
    pub released: String,
}

pub async fn list_versions(client: &reqwest::Client) -> Result<Vec<GameVersion>> {
    let v = get_json(client, MANIFEST).await?;
    Ok(v["versions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| {
            Some(GameVersion { id: x["id"].as_str()?.into(), kind: x["type"].as_str()?.into(), released: x["releaseTime"].as_str().unwrap_or("").into() })
        })
        .collect())
}

/// Loader versions that exist for one game version, newest first.
pub async fn loader_versions(client: &reqwest::Client, loader: &str, mc: &str) -> Result<Vec<String>> {
    let url = match loader {
        "fabric" => format!("{FABRIC_META}/versions/loader/{mc}"),
        "quilt" => format!("{QUILT_META}/versions/loader/{mc}"),
        _ => return Ok(vec![]),
    };
    let v = get_json(client, &url).await?;
    Ok(v.as_array().into_iter().flatten().filter_map(|x| x["loader"]["version"].as_str().map(String::from)).collect())
}

/// Writes `versions/<id>/<id>.json` for a vanilla or loader version and returns its id.
/// A loader profile names its parent in `inheritsFrom`, so the parent is written too.
pub async fn ensure_version_json(client: &reqwest::Client, p: &Paths, mc: &str, loader: &str, loader_version: Option<&str>) -> Result<String> {
    let vanilla_path = p.versions().join(mc).join(format!("{mc}.json"));
    if !vanilla_path.exists() {
        let m = get_json(client, MANIFEST).await?;
        let url = m["versions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|x| x["id"] == mc)
            .and_then(|x| x["url"].as_str())
            .ok_or_else(|| format!("Minecraft {mc} is not in the Mojang list"))?;
        let doc = client.get(url).send().await.map_err(e)?.error_for_status().map_err(e)?.bytes().await.map_err(e)?;
        write(&vanilla_path, &doc)?;
    }
    let (meta, tag) = match loader {
        "fabric" => (FABRIC_META, "fabric-loader"),
        "quilt" => (QUILT_META, "quilt-loader"),
        _ => return Ok(mc.to_string()),
    };
    let lv = match loader_version {
        Some(v) => v.to_string(),
        None => loader_versions(client, loader, mc).await?.into_iter().next().ok_or_else(|| format!("no {loader} build for Minecraft {mc}"))?,
    };
    let id = format!("{tag}-{lv}-{mc}");
    let path = p.versions().join(&id).join(format!("{id}.json"));
    if !path.exists() {
        let doc = client.get(format!("{meta}/versions/loader/{mc}/{lv}/profile/json")).send().await.map_err(e)?.error_for_status().map_err(e)?.bytes().await.map_err(e)?;
        write(&path, &doc)?;
    }
    Ok(id)
}

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::create_dir_all(path.parent().ok_or("bad path")?).map_err(e)?;
    std::fs::write(path, bytes).map_err(e)
}

/// The version document with `inheritsFrom` folded in, as the launcher uses it.
pub fn load_merged(p: &Paths, id: &str) -> Result<Value> {
    let raw = std::fs::read(p.versions().join(id).join(format!("{id}.json"))).map_err(|x| format!("version {id} is not downloaded: {x}"))?;
    let mut child: Value = serde_json::from_slice(&raw).map_err(e)?;
    let Some(parent_id) = child["inheritsFrom"].as_str().map(String::from) else {
        return Ok(child);
    };
    let mut parent = load_merged(p, &parent_id)?;
    // libraries: the child's go first so that they win on the class path
    let mut libs: Vec<Value> = child["libraries"].as_array().cloned().unwrap_or_default();
    libs.extend(parent["libraries"].as_array().cloned().unwrap_or_default());
    for kind in ["game", "jvm"] {
        let mut merged: Vec<Value> = parent["arguments"][kind].as_array().cloned().unwrap_or_default();
        merged.extend(child["arguments"][kind].as_array().cloned().unwrap_or_default());
        if !merged.is_empty() {
            parent["arguments"][kind] = Value::Array(merged);
        }
    }
    if let Some(o) = child.as_object() {
        for (k, v) in o {
            if !matches!(k.as_str(), "libraries" | "arguments" | "inheritsFrom") {
                parent[k] = v.clone();
            }
        }
    }
    parent["libraries"] = Value::Array(libs);
    parent["id"] = Value::String(id.into());
    Ok(parent)
}

fn os_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

/// Evaluates the `rules` array Mojang attaches to libraries and arguments (no optional features are enabled).
pub fn rules_allow(rules: &Value) -> bool {
    let Some(rules) = rules.as_array() else { return true };
    let mut allowed = false;
    for r in rules {
        let matches_os = match r["os"].as_object() {
            None => true,
            Some(o) => {
                let name_ok = o.get("name").and_then(|n| n.as_str()).is_none_or(|n| n == os_name());
                let arch_ok = o.get("arch").and_then(|n| n.as_str()).is_none_or(|a| match a {
                    "x86" => cfg!(target_arch = "x86"),
                    "arm64" | "aarch64" => cfg!(target_arch = "aarch64"),
                    _ => true,
                });
                name_ok && arch_ok
            }
        };
        let features_ok = r["features"].as_object().is_none_or(|f| f.is_empty());
        if matches_os && features_ok {
            allowed = r["action"] == "allow";
        }
    }
    allowed
}

/// `natives-windows-arm64` and `natives-windows-x86` libraries must not unpack over the right ones.
fn native_matches_arch(name: &str) -> bool {
    let tail = name.rsplit(':').next().unwrap_or("");
    let arm = tail.ends_with("arm64") || tail.ends_with("aarch64");
    let x86 = tail.ends_with("-x86") || tail.ends_with("x86_32");
    if cfg!(target_arch = "aarch64") { arm } else if cfg!(target_arch = "x86") { x86 } else { !arm && !x86 }
}

/// Maven coordinates `group:artifact:version[:classifier]` to a relative path.
fn maven_path(name: &str) -> Option<String> {
    let mut it = name.split(':');
    let (g, a, v) = (it.next()?, it.next()?, it.next()?);
    let classifier = it.next();
    let file = match classifier {
        Some(c) => format!("{a}-{v}-{c}.jar"),
        None => format!("{a}-{v}.jar"),
    };
    Some(format!("{}/{a}/{v}/{file}", g.replace('.', "/")))
}

#[derive(Debug, Clone)]
pub struct Artifact {
    pub url: String,
    pub path: PathBuf,
    pub sha1: Option<String>,
    /// Native libraries are unpacked into the natives folder before launch.
    pub native: bool,
}

/// Every jar the version needs: libraries and native libraries that apply to this system.
pub fn library_artifacts(p: &Paths, doc: &Value) -> Vec<Artifact> {
    let mut out = vec![];
    for lib in doc["libraries"].as_array().into_iter().flatten() {
        if !rules_allow(&lib["rules"]) {
            continue;
        }
        let name = lib["name"].as_str().unwrap_or("");
        // old style natives: a classifier chosen per system
        let classifier = lib["natives"][os_name()].as_str().map(|c| c.replace("${arch}", if cfg!(target_pointer_width = "32") { "32" } else { "64" }));
        let dl = &lib["downloads"];
        let mut push = |art: &Value, rel: Option<String>, native: bool| {
            let rel = art["path"].as_str().map(String::from).or(rel);
            let Some(rel) = rel else { return };
            let url = art["url"].as_str().map(String::from).or_else(|| lib["url"].as_str().map(|b| format!("{}/{rel}", b.trim_end_matches('/'))));
            let Some(url) = url.filter(|u| !u.is_empty()) else { return };
            out.push(Artifact { url, path: p.libraries().join(&rel), sha1: art["sha1"].as_str().map(String::from), native });
        };
        if let Some(c) = &classifier {
            let art = &dl["classifiers"][c.as_str()];
            if art.is_object() {
                push(art, None, true);
            }
        }
        if dl["artifact"].is_object() {
            // `natives-windows` style libraries carry the classifier in their name
            let native = name.contains(":natives-");
            if native && !native_matches_arch(name) {
                continue;
            }
            push(&dl["artifact"], None, native);
        } else if !dl.is_object() {
            // loader libraries list only a name and a repository
            if let Some(rel) = maven_path(name) {
                let base = lib["url"].as_str().unwrap_or("https://libraries.minecraft.net/");
                out.push(Artifact { url: format!("{}/{rel}", base.trim_end_matches('/')), path: p.libraries().join(&rel), sha1: lib["sha1"].as_str().map(String::from), native: false });
            }
        }
    }
    out
}

pub fn client_jar(p: &Paths, doc: &Value) -> PathBuf {
    let base = doc["inheritsFrom"].as_str().or(doc["id"].as_str()).unwrap_or("");
    p.versions().join(base).join(format!("{base}.jar"))
}

/// Downloads everything `version_id` needs to start: the game jar, libraries, native libraries and assets.
pub async fn install_version(client: &reqwest::Client, p: &Paths, version_id: &str, report: Report<'_>) -> Result<Value> {
    let doc = load_merged(p, version_id)?;
    report("jar", 0, 1);
    let jar = &doc["downloads"]["client"];
    let url = jar["url"].as_str().ok_or("this version has no client download")?;
    let dest = client_jar(p, &doc);
    fetch_file(client, url, &dest, jar["sha1"].as_str()).await?;

    let libs = library_artifacts(p, &doc);
    let total = libs.len() as u64;
    let done = AtomicU64::new(0);
    let results: Vec<Result<()>> = stream::iter(libs.clone())
        .map(|a| {
            let done = &done;
            async move {
                let r = fetch_file(client, &a.url, &a.path, a.sha1.as_deref()).await;
                report("libraries", done.fetch_add(1, Ordering::Relaxed) + 1, total);
                r
            }
        })
        .buffer_unordered(8)
        .collect()
        .await;
    for r in results {
        r?;
    }

    install_assets(client, p, &doc, report).await?;
    Ok(doc)
}

async fn install_assets(client: &reqwest::Client, p: &Paths, doc: &Value, report: Report<'_>) -> Result<()> {
    let idx = &doc["assetIndex"];
    let (Some(id), Some(url)) = (idx["id"].as_str(), idx["url"].as_str()) else { return Ok(()) };
    let index_path = p.assets().join("indexes").join(format!("{id}.json"));
    fetch_file(client, url, &index_path, idx["sha1"].as_str()).await?;
    let index: Value = serde_json::from_slice(&std::fs::read(&index_path).map_err(e)?).map_err(e)?;
    let objects: Vec<(String, u64)> = index["objects"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(_, o)| Some((o["hash"].as_str()?.to_string(), o["size"].as_u64().unwrap_or(0))))
        .collect();
    let total = objects.len() as u64;
    let done = AtomicU64::new(0);
    let objects_dir = p.assets().join("objects");
    let failed = AtomicU64::new(0);
    stream::iter(objects)
        .map(|(hash, _)| {
            let (done, failed, dir) = (&done, &failed, &objects_dir);
            async move {
                let dest = dir.join(&hash[..2]).join(&hash);
                if !dest.exists() {
                    let url = format!("https://resources.download.minecraft.net/{}/{hash}", &hash[..2]);
                    if fetch_file(client, &url, &dest, Some(&hash)).await.is_err() {
                        failed.fetch_add(1, Ordering::Relaxed);
                    }
                }
                report("assets", done.fetch_add(1, Ordering::Relaxed) + 1, total);
            }
        })
        .buffer_unordered(24)
        .collect::<Vec<_>>()
        .await;
    // a few missing sounds do not stop the game, but a broken download should be visible
    match failed.load(Ordering::Relaxed) {
        0 => Ok(()),
        n if n * 20 > total => Err(format!("{n} game files could not be downloaded; check the connection and try again")),
        _ => Ok(()),
    }
}

/// Unpacks native libraries (`.dll`/`.so`/`.dylib`) into `dir`.
pub fn extract_natives(libs: &[Artifact], dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(e)?;
    for a in libs.iter().filter(|a| a.native) {
        let Ok(f) = std::fs::File::open(&a.path) else { continue };
        let Ok(mut z) = zip::ZipArchive::new(f) else { continue };
        for i in 0..z.len() {
            let Ok(mut entry) = z.by_index(i) else { continue };
            let Some(rel) = entry.enclosed_name() else { continue };
            let name = rel.to_string_lossy().into_owned();
            if entry.is_dir() || name.contains("META-INF") {
                continue;
            }
            if !(name.ends_with(".dll") || name.ends_with(".so") || name.ends_with(".dylib") || name.ends_with(".jnilib")) {
                continue;
            }
            let file = Path::new(&name).file_name().map(|n| dir.join(n));
            if let Some(out) = file {
                if let Ok(mut o) = std::fs::File::create(&out) {
                    let _ = std::io::copy(&mut entry, &mut o);
                }
            }
        }
    }
    Ok(())
}
