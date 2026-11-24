//! Searching and installing from Modrinth and CurseForge.
use super::{e, fetch_file, get_json, game_dir, Paths, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

const MODRINTH: &str = "https://api.modrinth.com/v2";
const CURSEFORGE: &str = "https://api.curseforge.com/v1";
const CF_GAME: u32 = 432;

#[derive(Debug, Clone, Deserialize)]
pub struct SearchReq {
    /// `modrinth` or `curseforge`
    pub provider: String,
    pub query: String,
    /// `mod`, `modpack`, `resourcepack`, `shader`
    pub kind: String,
    pub mc_version: Option<String>,
    pub loader: Option<String>,
    #[serde(default)]
    pub page: u32,
    /// `relevance` | `downloads` | `newest` | `updated`; empty = by the query (relevance) or by popularity.
    #[serde(default)]
    pub sort: Option<String>,
    /// A Modrinth category (`optimization`, `magic`...). Ignored by CurseForge.
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub provider: String,
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon: Option<String>,
    pub downloads: u64,
    pub author: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Page {
    pub hits: Vec<Hit>,
    pub total: u64,
}

pub const PAGE_SIZE: u32 = 24;

fn cf_class(kind: &str) -> u32 {
    match kind {
        "modpack" => 4471,
        "resourcepack" => 12,
        "shader" => 6552,
        _ => 6,
    }
}

fn cf_loader(l: &str) -> Option<u32> {
    match l {
        "forge" => Some(1),
        "fabric" => Some(4),
        "quilt" => Some(5),
        "neoforge" => Some(6),
        _ => None,
    }
}

fn cf_get(client: &reqwest::Client, key: &str, url: &str) -> reqwest::RequestBuilder {
    client.get(url).header("x-api-key", key).header("Accept", "application/json")
}

async fn cf_json(rb: reqwest::RequestBuilder) -> Result<Value> {
    let r = rb.send().await.map_err(e)?;
    let s = r.status().as_u16();
    if s == 403 || s == 401 {
        return Err("CurseForge rejected the API key".into());
    }
    if !r.status().is_success() {
        return Err(format!("CurseForge: http {s}"));
    }
    r.json().await.map_err(e)
}

pub async fn search(client: &reqwest::Client, cf_key: &str, req: &SearchReq) -> Result<Page> {
    let offset = req.page * PAGE_SIZE;
    if req.provider == "curseforge" {
        if cf_key.trim().is_empty() {
            return Err("curseforge_key_missing".into());
        }
        let mut url = url::Url::parse(&format!("{CURSEFORGE}/mods/search")).map_err(e)?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("gameId", &CF_GAME.to_string());
            q.append_pair("classId", &cf_class(&req.kind).to_string());
            q.append_pair("pageSize", &PAGE_SIZE.to_string());
            q.append_pair("index", &offset.to_string());
            let by_query = !req.query.trim().is_empty();
            let field = match req.sort.as_deref() {
                Some("downloads") => "6",
                Some("newest") => "11",
                Some("updated") => "3",
                Some("relevance") if by_query => "1",
                _ => if by_query { "1" } else { "2" }, // relevance / popularity
            };
            q.append_pair("sortField", field);
            q.append_pair("sortOrder", "desc");
            if !req.query.trim().is_empty() {
                q.append_pair("searchFilter", req.query.trim());
            }
            if let Some(v) = req.mc_version.as_deref().filter(|v| !v.is_empty()) {
                q.append_pair("gameVersion", v);
            }
            if let Some(l) = req.loader.as_deref().and_then(cf_loader) {
                if req.kind == "mod" || req.kind == "modpack" {
                    q.append_pair("modLoaderType", &l.to_string());
                }
            }
        }
        let v = cf_json(cf_get(client, cf_key, url.as_str())).await?;
        let hits = v["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| {
                Some(Hit {
                    provider: "curseforge".into(),
                    id: m["id"].as_u64()?.to_string(),
                    slug: m["slug"].as_str().unwrap_or("").into(),
                    title: m["name"].as_str()?.into(),
                    description: m["summary"].as_str().unwrap_or("").into(),
                    icon: m["logo"]["thumbnailUrl"].as_str().map(String::from),
                    downloads: m["downloadCount"].as_f64().unwrap_or(0.0) as u64,
                    author: m["authors"][0]["name"].as_str().unwrap_or("").into(),
                    kind: req.kind.clone(),
                })
            })
            .collect();
        return Ok(Page { hits, total: v["pagination"]["totalCount"].as_u64().unwrap_or(0) });
    }

    // Modrinth: facets are an AND of ORs
    let mut facets: Vec<Vec<String>> = vec![vec![format!("project_type:{}", if req.kind == "shader" { "shader" } else { req.kind.as_str() })]];
    if let Some(v) = req.mc_version.as_deref().filter(|v| !v.is_empty()) {
        facets.push(vec![format!("versions:{v}")]);
    }
    if let Some(l) = req.loader.as_deref().filter(|l| !l.is_empty() && (req.kind == "mod" || req.kind == "modpack")) {
        facets.push(vec![format!("categories:{l}")]);
    }
    if let Some(cat) = req.category.as_deref().filter(|c| !c.is_empty()) {
        facets.push(vec![format!("categories:{cat}")]);
    }
    let mut url = url::Url::parse(&format!("{MODRINTH}/search")).map_err(e)?;
    url.query_pairs_mut()
        .append_pair("query", req.query.trim())
        .append_pair("facets", &serde_json::to_string(&facets).map_err(e)?)
        .append_pair("limit", &PAGE_SIZE.to_string())
        .append_pair("offset", &offset.to_string())
        .append_pair(
            "index",
            match req.sort.as_deref() {
                Some(s @ ("downloads" | "newest" | "updated")) => s,
                Some("relevance") => "relevance",
                _ => if req.query.trim().is_empty() { "downloads" } else { "relevance" },
            },
        );
    let v = get_json(client, url.as_str()).await?;
    let hits = v["hits"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            Some(Hit {
                provider: "modrinth".into(),
                id: m["project_id"].as_str()?.into(),
                slug: m["slug"].as_str().unwrap_or("").into(),
                title: m["title"].as_str()?.into(),
                description: m["description"].as_str().unwrap_or("").into(),
                icon: m["icon_url"].as_str().map(String::from),
                downloads: m["downloads"].as_u64().unwrap_or(0),
                author: m["author"].as_str().unwrap_or("").into(),
                kind: req.kind.clone(),
            })
        })
        .collect();
    Ok(Page { hits, total: v["total_hits"].as_u64().unwrap_or(0) })
}

/// A file picked for download.
#[derive(Debug, Clone)]
pub struct Pick {
    pub url: String,
    pub name: String,
    pub sha1: Option<String>,
    /// Required Modrinth dependencies (project ids)
    pub deps: Vec<String>,
}

pub fn folder_for(kind: &str) -> &'static str {
    match kind {
        "resourcepack" => "resourcepacks",
        "shader" => "shaderpacks",
        "world" => "saves",
        _ => "mods",
    }
}

fn safe_name(n: &str) -> String {
    n.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c }).collect()
}

/// The newest file of a project that fits the game version and loader.
pub async fn pick_file(client: &reqwest::Client, cf_key: &str, provider: &str, id: &str, kind: &str, mc: &str, loader: &str) -> Result<Pick> {
    let wants_loader = kind == "mod" && loader != "vanilla";
    if provider == "curseforge" {
        let mut url = url::Url::parse(&format!("{CURSEFORGE}/mods/{id}/files")).map_err(e)?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("gameVersion", mc);
            if wants_loader {
                if let Some(l) = cf_loader(loader) {
                    q.append_pair("modLoaderType", &l.to_string());
                }
            }
            q.append_pair("pageSize", "20");
        }
        let v = cf_json(cf_get(client, cf_key, url.as_str())).await?;
        let f = v["data"].as_array().into_iter().flatten().filter(|f| f["isAvailable"] != false).max_by_key(|f| f["fileDate"].as_str().unwrap_or("").to_string());
        let f = f.ok_or_else(|| format!("no file for Minecraft {mc}"))?;
        let name = f["fileName"].as_str().ok_or("file without a name")?.to_string();
        let fid = f["id"].as_u64().ok_or("file without an id")?;
        let url = f["downloadUrl"].as_str().map(String::from).unwrap_or_else(|| format!("https://edge.forgecdn.net/files/{}/{}/{}", fid / 1000, fid % 1000, name));
        let sha1 = f["hashes"].as_array().into_iter().flatten().find(|h| h["algo"] == 1).and_then(|h| h["value"].as_str()).map(String::from);
        return Ok(Pick { url, name, sha1, deps: vec![] });
    }
    let mut url = url::Url::parse(&format!("{MODRINTH}/project/{id}/version")).map_err(e)?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("game_versions", &serde_json::to_string(&[mc]).map_err(e)?);
        if wants_loader {
            q.append_pair("loaders", &serde_json::to_string(&[loader]).map_err(e)?);
        }
    }
    let v = get_json(client, url.as_str()).await?;
    let ver = v.as_array().and_then(|a| a.first()).ok_or_else(|| format!("no version for Minecraft {mc}"))?;
    let files = ver["files"].as_array().ok_or("version without files")?;
    let f = files.iter().find(|f| f["primary"] == true).or(files.first()).ok_or("version without files")?;
    let deps = ver["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["dependency_type"] == "required")
        .filter_map(|d| d["project_id"].as_str().map(String::from))
        .collect();
    Ok(Pick { url: f["url"].as_str().ok_or("file without url")?.into(), name: f["filename"].as_str().unwrap_or("file.jar").into(), sha1: f["hashes"]["sha1"].as_str().map(String::from), deps })
}

#[derive(Debug, Serialize)]
pub struct Installed {
    pub file: String,
    pub deps: Vec<String>,
}

/// Downloads a project into an instance, with the required Modrinth dependencies.
pub async fn install_project(client: &reqwest::Client, p: &Paths, cf_key: &str, inst: &super::Instance, provider: &str, id: &str, kind: &str) -> Result<Installed> {
    let mut queue = vec![id.to_string()];
    let mut seen: Vec<String> = vec![];
    let mut first: Option<String> = None;
    let mut deps_done = vec![];
    while let Some(pid) = queue.pop() {
        if seen.contains(&pid) || seen.len() >= 24 {
            continue;
        }
        seen.push(pid.clone());
        let pick = match pick_file(client, cf_key, provider, &pid, kind, &inst.mc_version, &inst.loader).await {
            Ok(x) => x,
            // a dependency with no matching file must not fail the whole install
            Err(x) if first.is_some() => {
                let _ = x;
                continue;
            }
            Err(x) => return Err(x),
        };
        let dest: PathBuf = game_dir(p, &inst.id).join(folder_for(kind)).join(safe_name(&pick.name));
        fetch_file(client, &pick.url, &dest, pick.sha1.as_deref()).await?;
        record(p, &inst.id, kind, &safe_name(&pick.name), &format!("{provider}:{pid}"));
        if first.is_none() {
            first = Some(pick.name.clone());
        } else {
            deps_done.push(pick.name.clone());
        }
        queue.extend(pick.deps);
    }
    Ok(Installed { file: first.unwrap_or_default(), deps: deps_done })
}

#[derive(Debug, Serialize)]
pub struct InstalledFile {
    pub kind: String,
    pub name: String,
    pub size: u64,
    pub enabled: bool,
}

/// Files in the instance's mods, resource packs and shader packs folders. A disabled mod ends in `.disabled`.
pub fn list_files(p: &Paths, id: &str) -> Vec<InstalledFile> {
    let mut out = vec![];
    for (kind, folder) in [("mod", "mods"), ("resourcepack", "resourcepacks"), ("shader", "shaderpacks"), ("world", "saves")] {
        for d in std::fs::read_dir(game_dir(p, id).join(folder)).into_iter().flatten().flatten() {
            let name = d.file_name().to_string_lossy().into_owned();
            let enabled = !name.ends_with(".disabled");
            out.push(InstalledFile { kind: kind.into(), name, size: d.metadata().map(|m| m.len()).unwrap_or(0), enabled });
        }
    }
    // data packs sit inside the worlds: shown as "world/pack"
    for w in std::fs::read_dir(game_dir(p, id).join("saves")).into_iter().flatten().flatten() {
        let world = w.file_name().to_string_lossy().into_owned();
        for d in std::fs::read_dir(w.path().join("datapacks")).into_iter().flatten().flatten() {
            let pack = d.file_name().to_string_lossy().into_owned();
            let enabled = !pack.ends_with(".disabled");
            out.push(InstalledFile { kind: "datapack".into(), name: format!("{world}/{pack}"), size: d.metadata().map(|m| m.len()).unwrap_or(0), enabled });
        }
    }
    out.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

fn file_path(p: &Paths, id: &str, kind: &str, name: &str) -> Result<PathBuf> {
    let bad = |s: &str| s.is_empty() || s.contains(['/', '\\']) || s.contains("..");
    if kind == "datapack" {
        // a data pack belongs to a world: "world/pack"
        let (world, pack) = name.split_once('/').ok_or("bad file name")?;
        if bad(world) || bad(pack) {
            return Err("bad file name".into());
        }
        return Ok(game_dir(p, id).join("saves").join(world).join("datapacks").join(pack));
    }
    if bad(name) {
        return Err("bad file name".into());
    }
    Ok(game_dir(p, id).join(folder_for(kind)).join(name))
}

pub fn set_file_enabled(p: &Paths, id: &str, kind: &str, name: &str, enabled: bool) -> Result<()> {
    let from = file_path(p, id, kind, name)?;
    let to_name = if enabled { name.strip_suffix(".disabled").unwrap_or(name).to_string() } else if name.ends_with(".disabled") { name.to_string() } else { format!("{name}.disabled") };
    std::fs::rename(from, file_path(p, id, kind, &to_name)?).map_err(e)
}

pub fn remove_file(p: &Paths, id: &str, kind: &str, name: &str) -> Result<()> {
    let path = file_path(p, id, kind, name)?;
    if path.is_dir() {
        std::fs::remove_dir_all(path).map_err(e)
    } else {
        std::fs::remove_file(path).map_err(e)
    }
}

#[derive(Debug, Serialize)]
pub struct UpdateInfo {
    pub kind: String,
    /// The file as it is in the folder.
    pub file: String,
    pub new_name: String,
    pub version: String,
}

struct Latest {
    name: String,
    url: String,
    sha1: Option<String>,
    version: String,
}

fn sha1_of(path: &std::path::Path) -> Option<String> {
    use sha1::{Digest, Sha1};
    std::fs::read(path).ok().map(|b| hex::encode(Sha1::digest(b)))
}

/// Hashes of files by path, size and time: a file that did not change is not read again.
#[derive(Default, Serialize, Deserialize)]
pub struct HashCache(std::collections::HashMap<String, (u64, u64, String)>);

impl HashCache {
    pub fn load(p: &Paths, id: &str) -> Self {
        std::fs::read_to_string(p.instance(id).join("hashes.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, p: &Paths, id: &str) {
        if let Ok(t) = serde_json::to_string(self) {
            let _ = std::fs::write(p.instance(id).join("hashes.json"), t);
        }
    }

    pub fn sha1(&mut self, path: &std::path::Path) -> Option<String> {
        let meta = std::fs::metadata(path).ok()?;
        let size = meta.len();
        let time = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64;
        let key = path.to_string_lossy().into_owned();
        if let Some((s, t, h)) = self.0.get(&key) {
            if *s == size && *t == time {
                return Some(h.clone());
            }
        }
        let h = sha1_of(path)?;
        self.0.insert(key, (size, time, h.clone()));
        Some(h)
    }
}

/// Which of the hashes Modrinth knows. `None` when it cannot be asked.
async fn known_hashes(client: &reqwest::Client, hashes: &[String]) -> Option<std::collections::HashSet<String>> {
    let body = serde_json::json!({ "hashes": hashes, "algorithm": "sha1" });
    let v: Value = client.post(format!("{MODRINTH}/version_files")).json(&body).send().await.ok()?.json().await.ok()?;
    Some(v.as_object()?.keys().cloned().collect())
}

/// Newest Modrinth version for each hash, for the game version and loader of the instance. Files Modrinth does not know are missing.
async fn latest_for(client: &reqwest::Client, inst: &super::Instance, kind: &str, hashes: &[String]) -> Result<std::collections::HashMap<String, Latest>> {
    let mut body = serde_json::json!({ "hashes": hashes, "algorithm": "sha1", "game_versions": [inst.mc_version] });
    if kind == "mod" {
        let loaders: Vec<&str> = match inst.loader.as_str() {
            "quilt" => vec!["quilt", "fabric"],
            "vanilla" => vec![],
            l => vec![l],
        };
        body["loaders"] = serde_json::json!(loaders);
    }
    let r = client.post(format!("{MODRINTH}/version_files/update")).json(&body).send().await.map_err(e)?;
    if !r.status().is_success() {
        return Err(format!("modrinth: http {}", r.status().as_u16()));
    }
    let v: Value = r.json().await.map_err(e)?;
    let mut out = std::collections::HashMap::new();
    for (hash, ver) in v.as_object().into_iter().flatten() {
        let files = ver["files"].as_array();
        let file = files.and_then(|f| f.iter().find(|x| x["primary"].as_bool() == Some(true)).or(f.first()));
        let Some(file) = file else { continue };
        // the same file again: nothing newer
        if files.is_some_and(|f| f.iter().any(|x| x["hashes"]["sha1"].as_str().is_some_and(|h| h.eq_ignore_ascii_case(hash)))) {
            continue;
        }
        let (Some(name), Some(url)) = (file["filename"].as_str(), file["url"].as_str()) else { continue };
        out.insert(hash.clone(), Latest { name: name.into(), url: url.into(), sha1: file["hashes"]["sha1"].as_str().map(str::to_string), version: ver["version_number"].as_str().unwrap_or("").into() });
    }
    Ok(out)
}

/// Files that have a newer version on Modrinth. Only files Modrinth knows by their hash are looked at.
pub async fn check_updates(client: &reqwest::Client, p: &Paths, inst: &super::Instance) -> Result<UpdateReport> {
    let mut out = vec![];
    let mut unknown: Vec<String> = vec![];
    let mut cache = HashCache::load(p, &inst.id);
    for kind in ["mod", "resourcepack", "shader"] {
        let dir = game_dir(p, &inst.id).join(folder_for(kind));
        let mut by_hash: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for d in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = d.file_name().to_string_lossy().into_owned();
            if name.ends_with(".disabled") || d.path().is_dir() {
                continue;
            }
            if let Some(h) = cache.sha1(&d.path()) {
                by_hash.insert(h, name);
            }
        }
        if by_hash.is_empty() {
            continue;
        }
        let hashes: Vec<String> = by_hash.keys().cloned().collect();
        if let Some(known) = known_hashes(client, &hashes).await {
            unknown.extend(by_hash.iter().filter(|(h, _)| !known.contains(*h)).map(|(_, n)| format!("{kind}/{n}")));
        }
        let latest = latest_for(client, inst, kind, &hashes).await?;
        for (h, l) in latest {
            if let Some(file) = by_hash.get(&h) {
                out.push(UpdateInfo { kind: kind.into(), file: file.clone(), new_name: l.name, version: l.version });
            }
        }
    }
    out.sort_by(|a, b| a.file.to_lowercase().cmp(&b.file.to_lowercase()));
    cache.save(p, &inst.id);
    Ok(UpdateReport { updates: out, unknown })
}

#[derive(Debug, Serialize)]
pub struct UpdateReport {
    pub updates: Vec<UpdateInfo>,
    /// Files Modrinth does not know (`kind/name`): they have no updates here.
    pub unknown: Vec<String>,
}

/// Replaces one file with its newest version. The old file goes only after the new one is in place.
pub async fn update_file(client: &reqwest::Client, p: &Paths, inst: &super::Instance, kind: &str, name: &str) -> Result<String> {
    let old = file_path(p, &inst.id, kind, name)?;
    let hash = sha1_of(&old).ok_or("cannot read the file")?;
    let latest = latest_for(client, inst, kind, std::slice::from_ref(&hash)).await?;
    let l = latest.get(&hash).ok_or("no newer version")?;
    let dest = game_dir(p, &inst.id).join(folder_for(kind)).join(safe_name(&l.name));
    fetch_file(client, &l.url, &dest, l.sha1.as_deref()).await?;
    if dest != old {
        let _ = std::fs::remove_file(&old);
    }
    Ok(l.name.clone())
}

/// What was installed from the catalog is written down, so the catalog can tell which projects a pack already has.
fn manifest_path(p: &Paths, id: &str) -> PathBuf {
    p.instance(id).join("installed.json")
}

fn read_manifest(p: &Paths, id: &str) -> std::collections::BTreeMap<String, String> {
    std::fs::read_to_string(manifest_path(p, id)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn record(p: &Paths, id: &str, kind: &str, name: &str, key: &str) {
    let mut m = read_manifest(p, id);
    m.insert(format!("{kind}/{name}"), key.to_string());
    if let Ok(t) = serde_json::to_string(&m) {
        let _ = std::fs::write(manifest_path(p, id), t);
    }
}

/// The catalog projects (`provider:id`) that have a file in the instance. Files put there by hand are recognised by their hash on Modrinth.
pub async fn installed_projects(client: &reqwest::Client, p: &Paths, inst: &super::Instance) -> Result<Vec<String>> {
    let manifest = read_manifest(p, &inst.id);
    let mut cache = HashCache::load(p, &inst.id);
    let mut out: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut unknown: Vec<String> = vec![];
    for kind in ["mod", "resourcepack", "shader"] {
        for d in std::fs::read_dir(game_dir(p, &inst.id).join(folder_for(kind))).into_iter().flatten().flatten() {
            if d.path().is_dir() {
                continue;
            }
            let name = d.file_name().to_string_lossy().into_owned();
            let plain = name.strip_suffix(".disabled").unwrap_or(&name);
            match manifest.get(&format!("{kind}/{plain}")) {
                Some(k) => {
                    out.insert(k.clone());
                }
                None => {
                    if let Some(h) = cache.sha1(&d.path()) {
                        unknown.push(h);
                    }
                }
            }
        }
    }
    cache.save(p, &inst.id);
    if !unknown.is_empty() {
        let body = serde_json::json!({ "hashes": unknown, "algorithm": "sha1" });
        // offline: the written-down ones are still right
        if let Ok(r) = client.post(format!("{MODRINTH}/version_files")).json(&body).send().await {
            if let Ok(v) = r.json::<Value>().await {
                for ver in v.as_object().into_iter().flat_map(|o| o.values()) {
                    if let Some(pid) = ver["project_id"].as_str() {
                        out.insert(format!("modrinth:{pid}"));
                    }
                }
            }
        }
    }
    Ok(out.into_iter().collect())
}

/// A short text that names the mods of a pack by their Modrinth versions: enough to rebuild the pack.
#[derive(Serialize, Deserialize)]
pub struct ShareCode {
    pub v: u8,
    pub name: String,
    pub mc: String,
    pub loader: String,
    pub lv: Option<String>,
    pub mods: Vec<String>,
    pub rp: Vec<String>,
    pub sh: Vec<String>,
}

pub const CODE_PREFIX: &str = "KAE1-";
/// The short form: binary, with a Modrinth version id (8 letters and digits) packed into 6 bytes.
const CODE2_PREFIX: &str = "KAE2-";
const B62: &[u8; 62] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOADERS: [&str; 5] = ["vanilla", "fabric", "quilt", "forge", "neoforge"];

fn id_to_bytes(id: &str) -> Option<[u8; 6]> {
    if id.len() != 8 {
        return None;
    }
    let mut n: u64 = 0;
    for c in id.bytes() {
        n = n * 62 + B62.iter().position(|b| *b == c)? as u64;
    }
    let b = n.to_be_bytes();
    Some([b[2], b[3], b[4], b[5], b[6], b[7]])
}

fn bytes_to_id(b: &[u8]) -> String {
    let mut n: u64 = 0;
    for x in b {
        n = (n << 8) | *x as u64;
    }
    let mut out = [b'0'; 8];
    for i in (0..8).rev() {
        out[i] = B62[(n % 62) as usize];
        n /= 62;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn put_varint(out: &mut Vec<u8>, mut n: usize) {
    loop {
        let b = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
}

fn put_str(out: &mut Vec<u8>, s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() > 255 {
        return false;
    }
    out.push(b.len() as u8);
    out.extend_from_slice(b);
    true
}

/// `None` when something does not fit the short form; the long one is used then.
fn encode_v2(c: &ShareCode) -> Option<String> {
    use base64::Engine;
    let loader = LOADERS.iter().position(|l| *l == c.loader)? as u8;
    let default_name = c.name == format!("Minecraft {}", c.mc);
    let mut out = vec![loader | if default_name { 8 } else { 0 } | if c.lv.is_some() { 16 } else { 0 }];
    if !put_str(&mut out, &c.mc) || (!default_name && !put_str(&mut out, &c.name)) {
        return None;
    }
    if let Some(lv) = &c.lv {
        if !put_str(&mut out, lv) {
            return None;
        }
    }
    for list in [&c.mods, &c.rp, &c.sh] {
        put_varint(&mut out, list.len());
    }
    for id in c.mods.iter().chain(&c.rp).chain(&c.sh) {
        out.extend_from_slice(&id_to_bytes(id)?);
    }
    Some(format!("{CODE2_PREFIX}{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(out)))
}

fn decode_v2(body: &str) -> Option<ShareCode> {
    use base64::Engine;
    let b = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(body.trim()).ok()?;
    let mut i = 0usize;
    let flags = *b.get(i)?;
    i += 1;
    let mut take_str = |i: &mut usize| -> Option<String> {
        let n = *b.get(*i)? as usize;
        let s = b.get(*i + 1..*i + 1 + n)?;
        *i += 1 + n;
        String::from_utf8(s.to_vec()).ok()
    };
    let mc = take_str(&mut i)?;
    let name = if flags & 8 != 0 { format!("Minecraft {mc}") } else { take_str(&mut i)? };
    let lv = if flags & 16 != 0 { Some(take_str(&mut i)?) } else { None };
    let mut counts = [0usize; 3];
    for c in counts.iter_mut() {
        let (mut n, mut shift) = (0usize, 0);
        loop {
            let x = *b.get(i)?;
            i += 1;
            n |= ((x & 0x7f) as usize) << shift;
            if x & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 28 {
                return None;
            }
        }
        *c = n;
    }
    let mut lists: Vec<Vec<String>> = vec![];
    for n in counts {
        let mut l = vec![];
        for _ in 0..n {
            l.push(bytes_to_id(b.get(i..i + 6)?));
            i += 6;
        }
        lists.push(l);
    }
    let loader = LOADERS.get((flags & 7) as usize)?.to_string();
    let sh = lists.pop()?;
    let rp = lists.pop()?;
    let mods = lists.pop()?;
    Some(ShareCode { v: 1, name, mc, loader, lv, mods, rp, sh })
}

/// The code of an instance and how many of its files could not be named (not on Modrinth).
pub async fn export_code(client: &reqwest::Client, p: &Paths, inst: &super::Instance) -> Result<(String, usize)> {
    use base64::Engine;
    let mut cache = HashCache::load(p, &inst.id);
    let mut code = ShareCode { v: 1, name: inst.name.clone(), mc: inst.mc_version.clone(), loader: inst.loader.clone(), lv: inst.loader_version.clone(), mods: vec![], rp: vec![], sh: vec![] };
    let mut skipped = 0;
    for kind in ["mod", "resourcepack", "shader"] {
        let mut hashes: Vec<String> = vec![];
        for d in std::fs::read_dir(game_dir(p, &inst.id).join(folder_for(kind))).into_iter().flatten().flatten() {
            let name = d.file_name().to_string_lossy().into_owned();
            if name.ends_with(".disabled") || d.path().is_dir() {
                continue;
            }
            if let Some(h) = cache.sha1(&d.path()) {
                hashes.push(h);
            }
        }
        if hashes.is_empty() {
            continue;
        }
        let body = serde_json::json!({ "hashes": hashes, "algorithm": "sha1" });
        let r = client.post(format!("{MODRINTH}/version_files")).json(&body).send().await.map_err(e)?;
        let v: Value = r.json().await.map_err(e)?;
        let ids: Vec<String> = hashes.iter().filter_map(|h| v[h]["id"].as_str().map(str::to_string)).collect();
        skipped += hashes.len() - ids.len();
        match kind {
            "mod" => code.mods = ids,
            "resourcepack" => code.rp = ids,
            _ => code.sh = ids,
        }
    }
    cache.save(p, &inst.id);
    if let Some(short) = encode_v2(&code) {
        return Ok((short, skipped));
    }
    let json = serde_json::to_vec(&code).map_err(e)?;
    Ok((format!("{CODE_PREFIX}{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)), skipped))
}

pub fn decode_code(text: &str) -> Result<ShareCode> {
    use base64::Engine;
    if let Some(body) = text.trim().strip_prefix(CODE2_PREFIX) {
        return decode_v2(body).ok_or_else(|| "the code is damaged".to_string());
    }
    let body = text.trim().strip_prefix(CODE_PREFIX).ok_or("this is not a Kae Mods code")?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(body.trim()).map_err(|_| "the code is damaged".to_string())?;
    let code: ShareCode = serde_json::from_slice(&bytes).map_err(|_| "the code is damaged".to_string())?;
    if code.v != 1 {
        return Err("the code is from a newer version of the app".into());
    }
    Ok(code)
}

/// Downloads the files a code names into the instance. `report` gets (done, total).
pub async fn install_code_files(client: &reqwest::Client, p: &Paths, inst: &super::Instance, code: &ShareCode, report: &(dyn Fn(u64, u64) + Sync)) -> Result<usize> {
    let all: Vec<(&str, &String)> = code.mods.iter().map(|v| ("mod", v)).chain(code.rp.iter().map(|v| ("resourcepack", v))).chain(code.sh.iter().map(|v| ("shader", v))).collect();
    let total = all.len() as u64;
    let mut failed = 0;
    for (n, chunk) in all.chunks(40).enumerate() {
        let ids = serde_json::to_string(&chunk.iter().map(|(_, v)| v.as_str()).collect::<Vec<_>>()).map_err(e)?;
        let mut url = url::Url::parse(&format!("{MODRINTH}/versions")).map_err(e)?;
        url.query_pairs_mut().append_pair("ids", &ids);
        let versions = get_json(client, url.as_str()).await?;
        for (done, (kind, vid)) in chunk.iter().enumerate() {
            report((n * 40 + done) as u64, total);
            let ver = versions.as_array().and_then(|a| a.iter().find(|x| x["id"].as_str() == Some(vid.as_str())));
            let file = ver.and_then(|v| v["files"].as_array()).and_then(|f| f.iter().find(|x| x["primary"].as_bool() == Some(true)).or(f.first()));
            let (Some(file), Some(ver)) = (file, ver) else {
                failed += 1;
                continue;
            };
            let (Some(name), Some(url)) = (file["filename"].as_str(), file["url"].as_str()) else {
                failed += 1;
                continue;
            };
            let dest = game_dir(p, &inst.id).join(folder_for(kind)).join(safe_name(name));
            if fetch_file(client, url, &dest, file["hashes"]["sha1"].as_str()).await.is_err() {
                failed += 1;
                continue;
            }
            if let Some(pid) = ver["project_id"].as_str() {
                record(p, &inst.id, kind, &safe_name(name), &format!("modrinth:{pid}"));
            }
        }
    }
    report(total, total);
    Ok(failed)
}

/// Everything the page of one Modrinth project shows: the project, its authors and its versions.
pub async fn project_detail(client: &reqwest::Client, id: &str) -> Result<Value> {
    let id = id.replace(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_', "");
    let (u1, u2, u3) = (format!("{MODRINTH}/project/{id}"), format!("{MODRINTH}/project/{id}/version"), format!("{MODRINTH}/project/{id}/members"));
    let (project, versions, team) = tokio::join!(get_json(client, &u1), get_json(client, &u2), get_json(client, &u3));
    let project = project?;
    let versions: Vec<Value> = versions
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .take(40)
        .map(|v| {
            let files = v["files"].as_array();
            let size = files.and_then(|f| f.iter().find(|f| f["primary"] == true).or(f.first())).and_then(|f| f["size"].as_u64()).unwrap_or(0);
            serde_json::json!({
                "id": v["id"], "name": v["name"], "version_number": v["version_number"], "version_type": v["version_type"],
                "game_versions": v["game_versions"], "loaders": v["loaders"], "date_published": v["date_published"],
                "downloads": v["downloads"], "changelog": v["changelog"], "size": size,
            })
        })
        .collect();
    let team: Vec<String> = team.ok().and_then(|t| t.as_array().cloned()).unwrap_or_default().iter().filter_map(|m| m["user"]["username"].as_str().map(String::from)).collect();
    Ok(serde_json::json!({
        "id": project["id"], "slug": project["slug"], "title": project["title"], "description": project["description"], "body": project["body"],
        "icon": project["icon_url"], "downloads": project["downloads"], "followers": project["followers"], "categories": project["categories"],
        "game_versions": project["game_versions"], "loaders": project["loaders"], "updated": project["updated"], "project_type": project["project_type"],
        "gallery": project["gallery"], "team": team, "versions": versions,
        "source_url": project["source_url"], "issues_url": project["issues_url"], "wiki_url": project["wiki_url"],
    }))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileMeta {
    pub kind: String,
    pub file: String,
    pub project: String,
    pub title: String,
    pub icon: Option<String>,
    pub author: String,
    pub more_authors: usize,
    pub client: String,
    pub server: String,
}

/// The last answer, read from disk with no network: the page shows it at once while a fresh one is fetched.
pub fn cached_file_meta(p: &Paths, id: &str) -> Vec<FileMeta> {
    std::fs::read_to_string(p.instance(id).join("filemeta.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// What Modrinth knows about the files of an instance (title, picture, author, where it runs), found by their hashes.
pub async fn file_meta(client: &reqwest::Client, p: &Paths, inst: &super::Instance) -> Result<Vec<FileMeta>> {
    let mut cache = HashCache::load(p, &inst.id);
    let mut by_hash: std::collections::HashMap<String, (String, String)> = std::collections::HashMap::new();
    for kind in ["mod", "resourcepack", "shader"] {
        for d in std::fs::read_dir(game_dir(p, &inst.id).join(folder_for(kind))).into_iter().flatten().flatten() {
            if d.path().is_dir() {
                continue;
            }
            if let Some(h) = cache.sha1(&d.path()) {
                by_hash.insert(h, (kind.to_string(), d.file_name().to_string_lossy().into_owned()));
            }
        }
    }
    cache.save(p, &inst.id);
    if by_hash.is_empty() {
        return Ok(vec![]);
    }
    let hashes: Vec<&String> = by_hash.keys().collect();
    let body = serde_json::json!({ "hashes": hashes, "algorithm": "sha1" });
    let r = client.post(format!("{MODRINTH}/version_files")).json(&body).send().await.map_err(e)?;
    if !r.status().is_success() {
        return Err(format!("modrinth: http {}", r.status().as_u16()));
    }
    let versions: Value = r.json().await.map_err(e)?;
    let mut of_hash: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (h, v) in versions.as_object().into_iter().flatten() {
        if let Some(pid) = v["project_id"].as_str() {
            of_hash.insert(h.clone(), pid.to_string());
        }
    }
    let mut ids: Vec<String> = of_hash.values().cloned().collect();
    ids.sort();
    ids.dedup();
    let mut projects: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for chunk in ids.chunks(100) {
        let url = format!("{MODRINTH}/projects?ids={}", url::form_urlencoded::byte_serialize(serde_json::to_string(chunk).map_err(e)?.as_bytes()).collect::<String>());
        if let Ok(v) = get_json(client, &url).await {
            for pr in v.as_array().into_iter().flatten() {
                if let Some(id) = pr["id"].as_str() {
                    projects.insert(id.to_string(), pr.clone());
                }
            }
        }
    }
    let mut teams: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let team_ids: Vec<String> = projects.values().filter_map(|pr| pr["team"].as_str().map(String::from)).collect();
    for chunk in team_ids.chunks(100) {
        let url = format!("{MODRINTH}/teams?ids={}", url::form_urlencoded::byte_serialize(serde_json::to_string(chunk).map_err(e)?.as_bytes()).collect::<String>());
        if let Ok(v) = get_json(client, &url).await {
            for members in v.as_array().into_iter().flatten() {
                let names: Vec<String> = members.as_array().into_iter().flatten().filter_map(|m| m["user"]["username"].as_str().map(String::from)).collect();
                if let Some(t) = members.as_array().and_then(|a| a.first()).and_then(|m| m["team_id"].as_str()) {
                    teams.insert(t.to_string(), names);
                }
            }
        }
    }
    let mut out = vec![];
    for (h, (kind, file)) in by_hash {
        let Some(pid) = of_hash.get(&h) else { continue };
        let Some(pr) = projects.get(pid) else { continue };
        let names = pr["team"].as_str().and_then(|t| teams.get(t)).cloned().unwrap_or_default();
        out.push(FileMeta {
            kind,
            file,
            project: pid.clone(),
            title: pr["title"].as_str().unwrap_or("").into(),
            icon: pr["icon_url"].as_str().map(String::from),
            author: names.first().cloned().unwrap_or_default(),
            more_authors: names.len().saturating_sub(1),
            client: pr["client_side"].as_str().unwrap_or("unknown").into(),
            server: pr["server_side"].as_str().unwrap_or("unknown").into(),
        });
    }
    if let Ok(t) = serde_json::to_string(&out) {
        let _ = std::fs::write(p.instance(&inst.id).join("filemeta.json"), t);
    }
    Ok(out)
}

#[cfg(test)]
mod code_tests {
    use super::*;

    #[test]
    fn the_short_code_round_trips_and_is_short() {
        let c = ShareCode { v: 1, name: "Minecraft 1.20.1".into(), mc: "1.20.1".into(), loader: "fabric".into(), lv: Some("0.20.1".into()), mods: vec!["AANobbMI".into(), "u6dRKJwZ".into()], rp: vec![], sh: vec!["00000000".into()] };
        let text = encode_v2(&c).unwrap();
        assert!(text.len() < 60, "{text}");
        let back = decode_code(&text).unwrap();
        assert_eq!((back.name, back.mc, back.loader, back.lv, back.mods, back.rp, back.sh), (c.name, c.mc, c.loader, c.lv, c.mods, c.rp, c.sh));
    }

    #[test]
    fn the_old_code_still_reads() {
        let old = "KAE1-eyJ2IjoxLCJuYW1lIjoiTWluZWNyYWZ0IDI2LjQtcHJlLTEiLCJtYyI6IjI2LjQtcHJlLTEiLCJsb2FkZXIiOiJmYWJyaWMiLCJsdiI6IjAuMjAuMSIsIm1vZHMiOltdLCJycCI6W10sInNoIjpbXX0";
        assert_eq!(decode_code(old).unwrap().mc, "26.4-pre-1");
    }
}
