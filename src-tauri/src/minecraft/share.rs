//! The `.kaepack` format: one file that carries a whole instance so it can be shared.
//!
//! It is a zip with `kae-pack.json` (game version, loader and the list of files with download links)
//! and an `overrides/` folder holding everything that has no public link: configs and files that are not on Modrinth.
use super::{e, fetch_file, game_dir, new_instance_id, now, Instance, Paths, Report, Result};
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const EXT: &str = "kaepack";
const MANIFEST: &str = "kae-pack.json";
const FORMAT: u32 = 1;
/// Folders whose content is checked against Modrinth; the rest of them goes into `overrides`.
const CATALOG_DIRS: [&str; 3] = ["mods", "resourcepacks", "shaderpacks"];
/// Settings worth sharing next to the mods. Worlds and logs stay at home.
const SETTINGS: [&str; 4] = ["config", "defaultconfigs", "kubejs", "scripts"];
const MAX_UNPACKED: u64 = 4 << 30;

#[derive(Debug, Serialize, Deserialize)]
struct Entry {
    path: String,
    sha1: String,
    size: u64,
    urls: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    format: u32,
    app: String,
    name: String,
    mc_version: String,
    loader: String,
    loader_version: Option<String>,
    memory_mb: u32,
    files: Vec<Entry>,
}

/// The loader build behind an instance, read back from the version id the loader installer made.
pub fn loader_version_of(inst: &Instance) -> Option<String> {
    if let Some(v) = &inst.loader_version {
        return Some(v.clone());
    }
    let id = &inst.version_id;
    let mc = &inst.mc_version;
    match inst.loader.as_str() {
        "fabric" => id.strip_prefix("fabric-loader-")?.strip_suffix(&format!("-{mc}")).map(String::from),
        "quilt" => id.strip_prefix("quilt-loader-")?.strip_suffix(&format!("-{mc}")).map(String::from),
        "neoforge" => id.strip_prefix("neoforge-").map(String::from),
        "forge" => id.strip_prefix(&format!("{mc}-forge-")).or_else(|| id.strip_prefix(&format!("{mc}-forge{mc}-"))).map(String::from),
        _ => None,
    }
}

fn walk(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) {
    for d in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = d.path();
        if d.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            walk(&p, base, out);
        } else {
            out.push(p.strip_prefix(base).unwrap_or(&p).to_path_buf());
        }
    }
}

fn rel_string(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Packs an instance into `dest`. Files known to Modrinth are stored as links, everything else is copied in.
pub async fn export(client: &reqwest::Client, p: &Paths, inst: &Instance, dest: &Path, report: Report<'_>) -> Result<()> {
    let gdir = game_dir(p, &inst.id);
    let mut catalog: Vec<PathBuf> = vec![];
    let mut loose: Vec<PathBuf> = vec![];
    for dir in CATALOG_DIRS {
        let mut found = vec![];
        walk(&gdir.join(dir), &gdir, &mut found);
        catalog.extend(found);
    }
    for dir in SETTINGS {
        walk(&gdir.join(dir), &gdir, &mut loose);
    }
    if gdir.join("options.txt").exists() {
        loose.push(PathBuf::from("options.txt"));
    }
    if catalog.len() + loose.len() > 20_000 {
        return Err("too many files to share".into());
    }

    // hashes, then one question to Modrinth for all of them
    let mut hashes: HashMap<String, String> = HashMap::new(); // rel path -> sha1
    for (i, rel) in catalog.iter().enumerate() {
        let bytes = std::fs::read(gdir.join(rel)).map_err(e)?;
        hashes.insert(rel_string(rel), hex::encode(Sha1::digest(&bytes)));
        report("hash", i as u64 + 1, catalog.len() as u64);
    }
    let mut known: HashMap<String, (String, u64)> = HashMap::new(); // sha1 -> (url, size)
    let all: Vec<&String> = hashes.values().collect();
    for chunk in all.chunks(200) {
        let sent = client.post("https://api.modrinth.com/v2/version_files").json(&serde_json::json!({ "hashes": chunk, "algorithm": "sha1" })).send().await;
        // offline or refused: everything in this chunk is then packed by value
        let Ok(r) = sent.and_then(|r| r.error_for_status()) else { continue };
        let v: serde_json::Value = r.json().await.map_err(e)?;
        for (hash, ver) in v.as_object().into_iter().flatten() {
            let f = ver["files"].as_array().into_iter().flatten().find(|f| f["hashes"]["sha1"].as_str() == Some(hash.as_str()));
            if let Some(f) = f {
                if let Some(url) = f["url"].as_str() {
                    known.insert(hash.clone(), (url.to_string(), f["size"].as_u64().unwrap_or(0)));
                }
            }
        }
    }

    let mut entries: Vec<Entry> = vec![];
    for rel in &catalog {
        let key = rel_string(rel);
        let sha = &hashes[&key];
        match known.get(sha) {
            Some((url, size)) => entries.push(Entry { path: key, sha1: sha.clone(), size: *size, urls: vec![url.clone()] }),
            None => loose.push(rel.clone()),
        }
    }
    let manifest = Manifest {
        format: FORMAT,
        app: "kae-mods".into(),
        name: inst.name.clone(),
        mc_version: inst.mc_version.clone(),
        loader: inst.loader.clone(),
        loader_version: loader_version_of(inst),
        memory_mb: inst.memory_mb,
        files: entries,
    };

    let tmp = dest.with_extension("part");
    {
        let file = std::fs::File::create(&tmp).map_err(e)?;
        let mut z = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        z.start_file(MANIFEST, opts).map_err(e)?;
        z.write_all(&serde_json::to_vec_pretty(&manifest).map_err(e)?).map_err(e)?;
        for (i, rel) in loose.iter().enumerate() {
            z.start_file(format!("overrides/{}", rel_string(rel)), opts).map_err(e)?;
            z.write_all(&std::fs::read(gdir.join(rel)).map_err(e)?).map_err(e)?;
            report("pack", i as u64 + 1, loose.len() as u64);
        }
        z.finish().map_err(e)?;
    }
    std::fs::rename(&tmp, dest).map_err(e)
}

/// What a pack file turned out to be, before anything is installed.
pub fn kind_of(bytes: &[u8]) -> Result<&'static str> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| "this is not a pack file".to_string())?;
    if z.by_name(MANIFEST).is_ok() {
        Ok("kae")
    } else if z.by_name("modrinth.index.json").is_ok() {
        Ok("mrpack")
    } else if z.by_name("manifest.json").is_ok() {
        Ok("curseforge")
    } else {
        Err("this is not a pack file".into())
    }
}

/// Unpacks a `.kaepack` into a new instance (without its game version; the caller prepares that).
pub async fn import(client: &reqwest::Client, p: &Paths, bytes: &[u8], name: Option<&str>, report: Report<'_>) -> Result<Instance> {
    let manifest: Manifest = {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(e)?;
        let mut f = z.by_name(MANIFEST).map_err(|_| "the pack has no manifest".to_string())?;
        let mut v = vec![];
        f.read_to_end(&mut v).map_err(e)?;
        serde_json::from_slice(&v).map_err(|x| format!("the pack manifest is damaged: {x}"))?
    };
    if manifest.format > FORMAT {
        return Err("this pack was made by a newer version of the app".into());
    }
    if !matches!(manifest.loader.as_str(), "vanilla" | "fabric" | "quilt" | "forge" | "neoforge") {
        return Err(format!("loader_unsupported:{}", manifest.loader));
    }
    let title = name.filter(|n| !n.trim().is_empty()).unwrap_or(&manifest.name).to_string();
    let inst = Instance {
        id: new_instance_id(p, &title),
        name: title,
        mc_version: manifest.mc_version.clone(),
        loader: manifest.loader.clone(),
        loader_version: manifest.loader_version.clone(),
        memory_mb: manifest.memory_mb.clamp(1024, 65536),
        created: now(),
        ..Default::default()
    };
    let gdir = game_dir(p, &inst.id);
    std::fs::create_dir_all(&gdir).map_err(e)?;

    // files that came by value
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(e)?;
    let mut unpacked: u64 = 0;
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(e)?;
        let Some(rel) = f.enclosed_name() else { continue };
        let rel = rel_string(&rel);
        let Some(inner) = rel.strip_prefix("overrides/") else { continue };
        if f.is_dir() || inner.is_empty() {
            continue;
        }
        unpacked += f.size();
        if unpacked > MAX_UNPACKED {
            return Err("the pack is unreasonably large".into());
        }
        let out = gdir.join(inner);
        std::fs::create_dir_all(out.parent().ok_or("bad path")?).map_err(e)?;
        let mut o = std::fs::File::create(&out).map_err(e)?;
        std::io::copy(&mut f, &mut o).map_err(e)?;
    }

    // files that come from the web
    for en in &manifest.files {
        if en.path.contains("..") || Path::new(&en.path).is_absolute() || !CATALOG_DIRS.iter().any(|d| en.path.starts_with(&format!("{d}/"))) {
            return Err("the pack names a file outside the game folder".into());
        }
    }
    let total = manifest.files.len() as u64;
    let done = AtomicU64::new(0);
    let results: Vec<Result<()>> = stream::iter(manifest.files)
        .map(|en| {
            let (gdir, done) = (&gdir, &done);
            async move {
                let mut last = "the file has no download link".to_string();
                for url in &en.urls {
                    match fetch_file(client, url, &gdir.join(&en.path), Some(&en.sha1)).await {
                        Ok(()) => {
                            last.clear();
                            break;
                        }
                        Err(x) => last = x,
                    }
                }
                report("mods", done.fetch_add(1, Ordering::Relaxed) + 1, total);
                if last.is_empty() { Ok(()) } else { Err(last) }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_version_is_read_back_from_the_version_id() {
        let mk = |loader: &str, id: &str| Instance { loader: loader.into(), version_id: id.into(), mc_version: "1.20.1".into(), ..Default::default() };
        assert_eq!(loader_version_of(&mk("fabric", "fabric-loader-0.15.7-1.20.1")).as_deref(), Some("0.15.7"));
        assert_eq!(loader_version_of(&mk("forge", "1.20.1-forge-47.2.0")).as_deref(), Some("47.2.0"));
        assert_eq!(loader_version_of(&mk("neoforge", "neoforge-21.1.77")).as_deref(), Some("21.1.77"));
        assert_eq!(loader_version_of(&mk("vanilla", "1.20.1")), None);
    }

    #[tokio::test]
    async fn a_pack_survives_export_and_import() {
        let dir = tempfile::tempdir().unwrap();
        let p = Paths::new(dir.path());
        let inst = Instance { id: "a".into(), name: "Mine".into(), mc_version: "1.20.1".into(), loader: "fabric".into(), loader_version: Some("0.15.7".into()), memory_mb: 3000, ..Default::default() };
        let g = game_dir(&p, "a");
        std::fs::create_dir_all(g.join("mods")).unwrap();
        std::fs::create_dir_all(g.join("config")).unwrap();
        std::fs::write(g.join("mods").join("own.jar"), b"not on modrinth").unwrap();
        std::fs::write(g.join("config").join("x.toml"), b"a=1").unwrap();
        // an offline client: nothing is known to Modrinth, so everything is packed by value
        let client = reqwest::Client::builder().proxy(reqwest::Proxy::all("http://127.0.0.1:9").unwrap()).build().unwrap();
        let out = dir.path().join("mine.kaepack");
        export(&client, &p, &inst, &out, &|_, _, _| {}).await.unwrap();
        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(kind_of(&bytes).unwrap(), "kae");
        let back = import(&client, &p, &bytes, Some("Copy"), &|_, _, _| {}).await.unwrap();
        assert_eq!((back.mc_version.as_str(), back.loader.as_str(), back.memory_mb), ("1.20.1", "fabric", 3000));
        assert_eq!(std::fs::read(game_dir(&p, &back.id).join("mods").join("own.jar")).unwrap(), b"not on modrinth");
        assert_eq!(std::fs::read(game_dir(&p, &back.id).join("config").join("x.toml")).unwrap(), b"a=1");
    }
}
