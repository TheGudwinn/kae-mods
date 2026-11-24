//! Installing modpacks: Modrinth `.mrpack` and CurseForge zip packs become a new instance.
use super::{e, fetch_file, game_dir, get_json, new_instance_id, now, Instance, Paths, Report, Result};
use futures_util::{stream, StreamExt};
use serde_json::Value;
use std::io::Read;
use std::path::Path;

/// Where the pack wants its mod loader, from the pack's own metadata.
struct Spec {
    mc: String,
    loader: String,
    loader_version: Option<String>,
}

fn loader_from_cf(id: &str) -> (String, Option<String>) {
    let (name, ver) = id.split_once('-').unwrap_or((id, ""));
    (name.to_string(), (!ver.is_empty()).then(|| ver.to_string()))
}

fn extract_overrides(zip_bytes: &[u8], prefixes: &[&str], dest: &Path) -> Result<()> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).map_err(e)?;
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(e)?;
        let Some(rel) = f.enclosed_name() else { continue };
        let rel_s = rel.to_string_lossy().replace('\\', "/");
        let Some(inner) = prefixes.iter().find_map(|p| rel_s.strip_prefix(&format!("{p}/"))) else { continue };
        if f.is_dir() || inner.is_empty() {
            continue;
        }
        let out = dest.join(inner);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(e)?;
        }
        let mut o = std::fs::File::create(&out).map_err(e)?;
        std::io::copy(&mut f, &mut o).map_err(e)?;
    }
    Ok(())
}

fn read_entry(zip_bytes: &[u8], name: &str) -> Result<Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).map_err(e)?;
    let mut f = z.by_name(name).map_err(|_| format!("the pack has no {name}"))?;
    let mut v = vec![];
    f.read_to_end(&mut v).map_err(e)?;
    Ok(v)
}

async fn download_bytes(client: &reqwest::Client, url: &str, report: Report<'_>) -> Result<Vec<u8>> {
    let r = client.get(url).send().await.map_err(e)?;
    if !r.status().is_success() {
        return Err(format!("pack download failed: http {}", r.status().as_u16()));
    }
    let total = r.content_length().unwrap_or(0);
    let mut s = r.bytes_stream();
    let mut data = vec![];
    while let Some(c) = s.next().await {
        data.extend_from_slice(&c.map_err(e)?);
        report("pack", data.len() as u64, total.max(data.len() as u64));
    }
    Ok(data)
}

fn check_loader(spec: &Spec) -> Result<()> {
    match spec.loader.as_str() {
        "vanilla" | "fabric" | "quilt" | "forge" | "neoforge" => Ok(()),
        other => Err(format!("loader_unsupported:{other}")),
    }
}

fn finish(p: &Paths, name: &str, spec: Spec) -> Result<Instance> {
    let id = new_instance_id(p, name);
    let inst = Instance { id, name: name.to_string(), mc_version: spec.mc, loader: spec.loader, loader_version: spec.loader_version, created: now(), ..Default::default() };
    Ok(inst)
}

/// `mc` and `loader` pick the pack build for that game version when the player filtered the catalog.
pub async fn install_modrinth(client: &reqwest::Client, p: &Paths, project_id: &str, name: &str, mc: Option<&str>, loader: Option<&str>, report: Report<'_>) -> Result<Instance> {
    let mut url = url::Url::parse(&format!("https://api.modrinth.com/v2/project/{project_id}/version")).map_err(e)?;
    {
        let mut q = url.query_pairs_mut();
        if let Some(v) = mc.filter(|v| !v.is_empty()) {
            q.append_pair("game_versions", &serde_json::to_string(&[v]).map_err(e)?);
        }
        if let Some(l) = loader.filter(|l| !l.is_empty()) {
            q.append_pair("loaders", &serde_json::to_string(&[l]).map_err(e)?);
        }
    }
    let versions = get_json(client, url.as_str()).await?;
    let ver = versions.as_array().and_then(|a| a.first()).ok_or("this pack has no build for the chosen version")?;
    pack_from_version(client, p, ver, name, report).await
}

/// Installs the pack file of one Modrinth version as a new instance and remembers which version it was.
async fn pack_from_version(client: &reqwest::Client, p: &Paths, ver: &Value, name: &str, report: Report<'_>) -> Result<Instance> {
    let file = ver["files"].as_array().and_then(|fs| fs.iter().find(|f| f["primary"] == true).or(fs.first())).ok_or("this pack has no files")?;
    let url = file["url"].as_str().ok_or("pack file without url")?;
    let bytes = download_bytes(client, url, report).await?;
    let mut inst = install_mrpack(client, p, &bytes, Some(name), report).await?;
    inst.source_version = ver["id"].as_str().unwrap_or("").to_string();
    Ok(inst)
}

/// The newest version of a Modrinth project (any game version) and its id.
pub async fn latest_pack_version(client: &reqwest::Client, project_id: &str) -> Result<(String, String)> {
    let v = get_json(client, &format!("https://api.modrinth.com/v2/project/{project_id}/version")).await?;
    let ver = v.as_array().and_then(|a| a.first()).ok_or("this pack has no versions")?;
    Ok((ver["id"].as_str().unwrap_or("").to_string(), ver["version_number"].as_str().unwrap_or("").to_string()))
}

/// Installs the given version of a Modrinth pack as a new instance.
pub async fn install_modrinth_version(client: &reqwest::Client, p: &Paths, version_id: &str, name: &str, report: Report<'_>) -> Result<Instance> {
    let ver = get_json(client, &format!("https://api.modrinth.com/v2/version/{version_id}")).await?;
    pack_from_version(client, p, &ver, name, report).await
}

/// Installs a Modrinth pack file (`.mrpack`) as a new instance.
pub async fn install_mrpack(client: &reqwest::Client, p: &Paths, bytes: &[u8], name: Option<&str>, report: Report<'_>) -> Result<Instance> {
    let bytes = bytes.to_vec();
    let index: Value = serde_json::from_slice(&read_entry(&bytes, "modrinth.index.json")?).map_err(e)?;
    let name = name.filter(|n| !n.trim().is_empty()).or(index["name"].as_str()).unwrap_or("Modpack");
    let deps = &index["dependencies"];
    let (loader, lv) = if let Some(v) = deps["fabric-loader"].as_str() {
        ("fabric", Some(v))
    } else if let Some(v) = deps["quilt-loader"].as_str() {
        ("quilt", Some(v))
    } else if let Some(v) = deps["neoforge"].as_str() {
        ("neoforge", Some(v))
    } else if let Some(v) = deps["forge"].as_str() {
        ("forge", Some(v))
    } else {
        ("vanilla", None)
    };
    let spec = Spec { mc: deps["minecraft"].as_str().ok_or("the pack does not name a Minecraft version")?.into(), loader: loader.into(), loader_version: lv.map(String::from) };
    check_loader(&spec)?;
    let inst = finish(p, name, spec)?;
    let gdir = game_dir(p, &inst.id);
    std::fs::create_dir_all(&gdir).map_err(e)?;
    extract_overrides(&bytes, &["overrides", "client-overrides"], &gdir)?;

    let mut files: Vec<(String, String, Option<String>)> = vec![]; // path, url, sha1
    for f in index["files"].as_array().into_iter().flatten().filter(|f| f["env"]["client"] != "unsupported") {
        let rel = f["path"].as_str().ok_or("file without path")?;
        if rel.contains("..") || Path::new(rel).is_absolute() {
            return Err("pack file path escapes the instance".to_string());
        }
        let url = f["downloads"][0].as_str().ok_or("file without download link")?;
        files.push((rel.to_string(), url.to_string(), f["hashes"]["sha1"].as_str().map(String::from)));
    }
    let total = files.len() as u64;
    let done = std::sync::atomic::AtomicU64::new(0);
    let results: Vec<Result<()>> = stream::iter(files)
        .map(|(rel, url, sha1)| {
            let (gdir, done) = (&gdir, &done);
            async move {
                let r = fetch_file(client, &url, &gdir.join(&rel), sha1.as_deref()).await;
                report("mods", done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1, total);
                r
            }
        })
        .buffer_unordered(8)
        .collect()
        .await;
    for r in results {
        r?;
    }
    Ok(inst)
}

pub async fn install_curseforge(client: &reqwest::Client, p: &Paths, key: &str, project_id: &str, name: &str, report: Report<'_>) -> Result<Instance> {
    if key.trim().is_empty() {
        return Err("curseforge_key_missing".into());
    }
    let get = |url: String| client.get(url).header("x-api-key", key).header("Accept", "application/json");
    let v: Value = get(format!("https://api.curseforge.com/v1/mods/{project_id}/files?pageSize=10")).send().await.map_err(e)?.error_for_status().map_err(e)?.json().await.map_err(e)?;
    let f = v["data"].as_array().into_iter().flatten().max_by_key(|f| f["fileDate"].as_str().unwrap_or("").to_string()).ok_or("this pack has no files")?;
    let fid = f["id"].as_u64().ok_or("pack file without id")?;
    let fname = f["fileName"].as_str().unwrap_or("pack.zip");
    let url = f["downloadUrl"].as_str().map(String::from).unwrap_or_else(|| format!("https://edge.forgecdn.net/files/{}/{}/{}", fid / 1000, fid % 1000, fname));
    let bytes = download_bytes(client, &url, report).await?;
    let manifest: Value = serde_json::from_slice(&read_entry(&bytes, "manifest.json")?).map_err(e)?;
    let loader_id = manifest["minecraft"]["modLoaders"].as_array().and_then(|l| l.iter().find(|x| x["primary"] == true).or(l.first())).and_then(|x| x["id"].as_str()).unwrap_or("");
    let (loader, lv) = if loader_id.is_empty() { ("vanilla".to_string(), None) } else { loader_from_cf(loader_id) };
    let spec = Spec { mc: manifest["minecraft"]["version"].as_str().ok_or("the pack does not name a Minecraft version")?.into(), loader, loader_version: lv };
    check_loader(&spec)?;
    let inst = finish(p, name, spec)?;
    let gdir = game_dir(p, &inst.id);
    std::fs::create_dir_all(&gdir).map_err(e)?;
    let overrides = manifest["overrides"].as_str().unwrap_or("overrides").to_string();
    extract_overrides(&bytes, &[&overrides], &gdir)?;

    let ids: Vec<u64> = manifest["files"].as_array().into_iter().flatten().filter_map(|f| f["fileID"].as_u64()).collect();
    let mut files: Vec<(String, String, Option<String>)> = vec![]; // name, url, sha1
    for chunk in ids.chunks(200) {
        let body = serde_json::json!({ "fileIds": chunk });
        let r: Value = client.post("https://api.curseforge.com/v1/mods/files").header("x-api-key", key).json(&body).send().await.map_err(e)?.error_for_status().map_err(e)?.json().await.map_err(e)?;
        for f in r["data"].as_array().into_iter().flatten() {
            let (Some(id), Some(n)) = (f["id"].as_u64(), f["fileName"].as_str()) else { continue };
            let url = f["downloadUrl"].as_str().map(String::from).unwrap_or_else(|| format!("https://edge.forgecdn.net/files/{}/{}/{}", id / 1000, id % 1000, n));
            let sha1 = f["hashes"].as_array().into_iter().flatten().find(|h| h["algo"] == 1).and_then(|h| h["value"].as_str()).map(String::from);
            files.push((n.to_string(), url, sha1));
        }
    }
    let total = files.len() as u64;
    let done = std::sync::atomic::AtomicU64::new(0);
    let results: Vec<Result<()>> = stream::iter(files)
        .map(|(n, url, sha1)| {
            let (gdir, done) = (&gdir, &done);
            async move {
                let folder = if n.ends_with(".zip") { "resourcepacks" } else { "mods" };
                let safe: String = n.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c }).collect();
                let r = fetch_file(client, &url, &gdir.join(folder).join(safe), sha1.as_deref()).await;
                report("mods", done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1, total);
                r
            }
        })
        .buffer_unordered(8)
        .collect()
        .await;
    for r in results {
        r?;
    }
    Ok(inst)
}
