//! Minecraft support: game versions, Java, instances, launching and the Modrinth / CurseForge catalogs.
//!
//! Everything lives under `<app data>/minecraft`: shared `versions`, `libraries`, `assets` and `runtime`
//! folders plus one folder per instance (the game directory of that instance).
pub mod auth;
pub mod catalog;
pub mod java;
pub mod launch;
pub mod loader;
pub mod modpack;
pub mod share;
pub mod versions;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, String>;

pub fn e<E: std::fmt::Display>(x: E) -> String {
    x.to_string()
}

/// Layout of the Minecraft folder.
#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    app_dir: PathBuf,
}

impl Paths {
    /// The data folder is the one the user chose (stored in `minecraft_root.txt`), else `<app data>/minecraft`.
    pub fn new(app_dir: &Path) -> Self {
        let chosen = std::fs::read_to_string(app_dir.join("minecraft_root.txt")).ok().map(|s| PathBuf::from(s.trim())).filter(|p| !p.as_os_str().is_empty());
        Self { root: chosen.unwrap_or_else(|| app_dir.join("minecraft")), app_dir: app_dir.to_path_buf() }
    }
    pub fn default_root(&self) -> PathBuf {
        self.app_dir.join("minecraft")
    }
    /// `None` goes back to the default folder.
    pub fn set_root(&mut self, dir: Option<&Path>) -> Result<()> {
        let file = self.app_dir.join("minecraft_root.txt");
        match dir {
            Some(d) => {
                if !d.is_absolute() {
                    return Err("pick a full folder path".into());
                }
                std::fs::create_dir_all(d).map_err(e)?;
                // a folder we can write to, checked now rather than in the middle of a download
                let probe = d.join(".kae-write-test");
                std::fs::write(&probe, b"").map_err(|x| format!("the folder is not writable: {x}"))?;
                let _ = std::fs::remove_file(probe);
                std::fs::write(file, d.to_string_lossy().as_bytes()).map_err(e)?;
                self.root = d.to_path_buf();
            }
            None => {
                let _ = std::fs::remove_file(file);
                self.root = self.default_root();
            }
        }
        Ok(())
    }
    pub fn versions(&self) -> PathBuf {
        self.root.join("versions")
    }
    pub fn libraries(&self) -> PathBuf {
        self.root.join("libraries")
    }
    pub fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }
    pub fn runtime(&self) -> PathBuf {
        self.root.join("runtime")
    }
    pub fn instances(&self) -> PathBuf {
        self.root.join("instances")
    }
    pub fn instance(&self, id: &str) -> PathBuf {
        self.instances().join(id)
    }
    pub fn config(&self) -> PathBuf {
        self.app_dir.join("minecraft_config.json")
    }
}

/// How a player signs in. Offline play needs no account; a licensed account is added later.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Account {
    pub name: String,
    pub offline: bool,
    /// A Microsoft sign-in, when there is one: then the game starts as that licensed account.
    pub ms: Option<MsAccount>,
}

/// What a Microsoft sign-in leaves: the profile and the tokens to renew it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsAccount {
    pub name: String,
    /// The player id without dashes, as the Minecraft service gives it.
    pub uuid: String,
    pub refresh_token: String,
    pub mc_token: String,
    /// Unix seconds after which `mc_token` is no longer good.
    pub expires_at: u64,
}

impl Default for Account {
    fn default() -> Self {
        Self { name: "Player".into(), offline: true, ms: None }
    }
}

/// Settings that are not tied to one instance.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    /// CurseForge asks every app for its own key; without one only Modrinth is searched.
    pub curseforge_key: String,
    pub account: Account,
    /// The application (client) id of the Microsoft app registration used for the sign-in.
    pub ms_client_id: String,
}

pub fn load_config(p: &Paths) -> Config {
    std::fs::read(p.config()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_config(p: &Paths, c: &Config) -> Result<()> {
    std::fs::write(p.config(), serde_json::to_vec_pretty(c).map_err(e)?).map_err(e)
}

/// One game profile: a Minecraft version, a mod loader and its own mods.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    /// `vanilla`, `fabric`, `quilt`
    pub loader: String,
    pub loader_version: Option<String>,
    pub memory_mb: u32,
    pub java_path: Option<String>,
    /// The version id under `versions/` that is launched (the loader profile, or the vanilla version)
    pub version_id: String,
    pub created: u64,
    /// Extra JVM arguments, space separated.
    pub extra_args: String,
    /// Whether `memory_mb` is the person's own value; otherwise the default is used.
    pub memory_custom: bool,
    /// Unix seconds of the last start, 0 = never.
    pub last_played: u64,
    /// Total play time in seconds.
    pub play_secs: u64,
    /// Unix seconds when the running session started, 0 = not running.
    pub session_start: u64,
    /// Show the live game console when the game starts.
    pub debug_console: bool,
    /// The catalog project this instance was installed from (`modrinth:<id>` or `curseforge:<id>`), empty for an own one.
    pub source: String,
    /// The Modrinth version id of the pack that was installed, to tell when a newer one is out.
    pub source_version: String,
}

pub const DEFAULT_MEMORY_MB: u32 = 4096;

impl Default for Instance {
    fn default() -> Self {
        Self { id: String::new(), name: String::new(), mc_version: String::new(), loader: "vanilla".into(), loader_version: None, memory_mb: DEFAULT_MEMORY_MB, java_path: None, version_id: String::new(), created: 0, extra_args: String::new(), memory_custom: false, last_played: 0, play_secs: 0, session_start: 0, debug_console: false, source: String::new(), source_version: String::new() }
    }
}

pub fn game_dir(p: &Paths, id: &str) -> PathBuf {
    p.instance(id).join("minecraft")
}

pub fn list_instances(p: &Paths) -> Vec<Instance> {
    let mut out: Vec<Instance> = std::fs::read_dir(p.instances())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|d| std::fs::read(d.path().join("instance.json")).ok())
        .filter_map(|b| serde_json::from_slice(&b).ok())
        .collect();
    out.sort_by(|a, b| b.created.cmp(&a.created));
    out
}

pub fn get_instance(p: &Paths, id: &str) -> Result<Instance> {
    if id.is_empty() || id.contains(['/', '\\', '.']) {
        return Err("bad instance id".into());
    }
    let b = std::fs::read(p.instance(id).join("instance.json")).map_err(|_| "instance not found".to_string())?;
    serde_json::from_slice(&b).map_err(e)
}

pub fn save_instance(p: &Paths, i: &Instance) -> Result<()> {
    let dir = p.instance(&i.id);
    std::fs::create_dir_all(game_dir(p, &i.id)).map_err(e)?;
    std::fs::write(dir.join("instance.json"), serde_json::to_vec_pretty(i).map_err(e)?).map_err(e)
}

/// A folder name that is safe on every system, unique among the instances.
pub fn new_instance_id(p: &Paths, name: &str) -> String {
    let base: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    let base = base.trim_matches('-');
    let base = if base.is_empty() { "instance" } else { base };
    let mut id = base.to_string();
    let mut n = 2;
    while p.instance(&id).exists() {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Progress of a long operation, sent to the window as `mc-progress`.
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub id: String,
    pub stage: String,
    pub done: u64,
    pub total: u64,
}

pub type Report<'a> = &'a (dyn Fn(&str, u64, u64) + Send + Sync);

/// Downloads `url` to `dest`, checking the SHA-1 when one is given. Existing files with a matching hash are kept.
pub async fn fetch_file(client: &reqwest::Client, url: &str, dest: &Path, sha1: Option<&str>) -> Result<()> {
    use sha1::{Digest, Sha1};
    let ok = |bytes: &[u8]| sha1.is_none_or(|h| hex::encode(Sha1::digest(bytes)).eq_ignore_ascii_case(h));
    if let Ok(existing) = tokio::fs::read(dest).await {
        if sha1.is_some() && ok(&existing) {
            return Ok(());
        }
    }
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(e)?;
    }
    let mut last = String::new();
    for attempt in 0..3u64 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(600 * attempt)).await;
        }
        let res = async {
            let r = client.get(url).send().await.map_err(e)?;
            if !r.status().is_success() {
                return Err(format!("{url}: http {}", r.status().as_u16()));
            }
            r.bytes().await.map_err(e)
        }
        .await;
        match res {
            Ok(b) if ok(&b) => {
                let tmp = dest.with_extension("part");
                tokio::fs::write(&tmp, &b).await.map_err(e)?;
                tokio::fs::rename(&tmp, dest).await.map_err(e)?;
                return Ok(());
            }
            Ok(_) => last = format!("{url}: checksum mismatch"),
            Err(x) => last = x,
        }
    }
    Err(last)
}

pub async fn get_json(client: &reqwest::Client, url: &str) -> Result<serde_json::Value> {
    let r = client.get(url).send().await.map_err(e)?;
    if !r.status().is_success() {
        return Err(format!("{url}: http {}", r.status().as_u16()));
    }
    r.json().await.map_err(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_matches_the_well_known_value() {
        // the id every offline launcher gives the nickname "Notch"
        assert_eq!(launch::offline_uuid("Notch"), "b50ad385-829d-3141-a216-7e7d7539ba7f");
    }

    #[test]
    fn rules_pick_this_system() {
        let r = serde_json::json!([{"action": "allow"}, {"action": "disallow", "os": {"name": "osx"}}]);
        assert_eq!(versions::rules_allow(&r), !cfg!(target_os = "macos"));
        assert!(!versions::rules_allow(&serde_json::json!([{"action": "allow", "features": {"is_demo_user": true}}])));
    }

    /// Needs the network: a Fabric profile resolves, its libraries list is plausible and the command line builds.
    #[tokio::test]
    #[ignore]
    async fn fabric_profile_to_command_line() {
        let dir = tempfile::tempdir().unwrap();
        let p = Paths::new(dir.path());
        let client = reqwest::Client::builder().user_agent("kae-mods-test").build().unwrap();
        let vs = versions::list_versions(&client).await.unwrap();
        assert!(vs.iter().any(|v| v.id == "1.20.1"));
        let id = versions::ensure_version_json(&client, &p, "1.20.1", "fabric", None).await.unwrap();
        let doc = versions::load_merged(&p, &id).unwrap();
        let libs = versions::library_artifacts(&p, &doc);
        assert!(libs.len() > 30, "{} libraries", libs.len());
        assert!(libs.iter().any(|l| l.url.contains("fabricmc")));
        assert!(libs.iter().any(|l| l.native));
        let inst = Instance { id: "t".into(), name: "t".into(), mc_version: "1.20.1".into(), loader: "fabric".into(), version_id: id, ..Default::default() };
        let prep = launch::build(&p, &inst, &doc, "java".into(), &Account::default()).unwrap();
        let line = prep.args.join(" ");
        assert!(line.contains("net.fabricmc.loader.impl.launch.knot.KnotClient"), "{line}");
        assert!(line.contains("--username Player") && line.contains("--version 1.20.1") || line.contains("--version fabric"), "{line}");
        assert!(!line.contains("${"), "unresolved placeholder: {line}");

        // old (1.8.9) style with minecraftArguments
        let old = versions::ensure_version_json(&client, &p, "1.8.9", "vanilla", None).await.unwrap();
        let doc = versions::load_merged(&p, &old).unwrap();
        let inst = Instance { id: "o".into(), version_id: old, mc_version: "1.8.9".into(), ..Default::default() };
        let prep = launch::build(&p, &inst, &doc, "java".into(), &Account::default()).unwrap();
        assert!(!prep.args.join(" ").contains("${"), "{:?}", prep.args);

        // catalogs
        let page = catalog::search(&client, "", &catalog::SearchReq { provider: "modrinth".into(), query: "sodium".into(), kind: "mod".into(), mc_version: Some("1.20.1".into()), loader: Some("fabric".into()), page: 0, sort: None, category: None }).await.unwrap();
        assert!(page.hits.iter().any(|h| h.slug == "sodium"));
        let pick = catalog::pick_file(&client, "", "modrinth", "AANobbMI", "mod", "1.20.1", "fabric").await.unwrap();
        assert!(pick.name.ends_with(".jar"));
        let packs = catalog::search(&client, "", &catalog::SearchReq { provider: "modrinth".into(), query: "".into(), kind: "modpack".into(), mc_version: None, loader: None, page: 0, sort: None, category: None }).await.unwrap();
        assert!(!packs.hits.is_empty());
    }

    /// Real run on a roomy drive: `MC_TEST_DIR=D:\mc-test` (and optionally `CF_KEY`). Downloads about 500 MB.
    #[tokio::test]
    #[ignore]
    async fn download_and_start_the_game() {
        let dir = std::path::PathBuf::from(std::env::var("MC_TEST_DIR").expect("set MC_TEST_DIR"));
        let app = dir.join("app");
        std::fs::create_dir_all(&app).unwrap();
        let mut p = Paths::new(&app);
        p.set_root(Some(&dir.join("data"))).unwrap();
        let client = reqwest::Client::builder().user_agent("kae-mods-test").build().unwrap();
        if let Ok(key) = std::env::var("CF_KEY") {
            let page = catalog::search(&client, &key, &catalog::SearchReq { provider: "curseforge".into(), query: "jei".into(), kind: "mod".into(), mc_version: Some("1.20.1".into()), loader: Some("fabric".into()), page: 0, sort: None, category: None }).await.unwrap();
            assert!(!page.hits.is_empty(), "curseforge returned nothing");
            let pick = catalog::pick_file(&client, &key, "curseforge", &page.hits[0].id, "mod", "1.20.1", "fabric").await.unwrap();
            println!("curseforge: {} -> {}", page.hits[0].title, pick.name);
        }
        let rep = |s: &str, d: u64, t: u64| {
            if d == t || d % 200 == 0 {
                println!("{s} {d}/{t}");
            }
        };
        let id = versions::ensure_version_json(&client, &p, "1.20.1", "fabric", None).await.unwrap();
        let doc = versions::install_version(&client, &p, &id, &rep).await.unwrap();
        let major = doc["javaVersion"]["majorVersion"].as_u64().unwrap_or(8) as u32;
        let java = java::ensure_java(&client, &p, major, None, &rep).await.unwrap();
        println!("java: {}", java.display());
        let inst = Instance { id: "t".into(), name: "t".into(), mc_version: "1.20.1".into(), loader: "fabric".into(), version_id: id, ..Default::default() };
        save_instance(&p, &inst).unwrap();
        // the Fabric API is not installed, so the loader stops with a message; reaching it proves the class path works
        let prep = launch::build(&p, &inst, &doc, java, &Account::default()).unwrap();
        launch::spawn("t", &prep).unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(25)).await;
        let alive = launch::is_running("t");
        launch::stop("t");
        let log = std::fs::read_to_string(game_dir(&p, "t").join("launcher.log")).unwrap_or_default();
        println!("alive={alive}
{}", log.lines().rev().take(15).collect::<Vec<_>>().join("
"));
        assert!(alive || log.contains("Setting user: Player"), "the game did not start:
{log}");
    }

    /// Forge 1.20.1 end to end on `MC_TEST_DIR`: installer, class path, start.
    #[tokio::test]
    #[ignore]
    async fn forge_installs_and_starts() {
        let dir = std::path::PathBuf::from(std::env::var("MC_TEST_DIR").expect("set MC_TEST_DIR"));
        let app = dir.join("app");
        std::fs::create_dir_all(&app).unwrap();
        let mut p = Paths::new(&app);
        p.set_root(Some(&dir.join("data"))).unwrap();
        let client = reqwest::Client::builder().user_agent("kae-mods-test").build().unwrap();
        let fv = loader::versions_for(&client, "forge", "1.20.1").await.unwrap();
        println!("forge builds: {:?}", &fv[..3.min(fv.len())]);
        let nv = loader::versions_for(&client, "neoforge", "1.21.1").await.unwrap();
        println!("neoforge builds: {:?}", &nv[..3.min(nv.len())]);
        assert!(!fv.is_empty() && !nv.is_empty());
        let mut inst = Instance { id: "f".into(), name: "f".into(), mc_version: "1.20.1".into(), loader: "forge".into(), loader_version: Some(fv.iter().find(|v| v.starts_with("47.")).cloned().unwrap_or(fv[0].clone())), ..Default::default() };
        let rep = |s: &str, d: u64, t: u64| {
            if d == t {
                println!("{s} {d}/{t}");
            }
        };
        inst.version_id = loader::ensure_version(&client, &p, &inst, &rep).await.unwrap();
        println!("version: {}", inst.version_id);
        let doc = versions::install_version(&client, &p, &inst.version_id, &rep).await.unwrap();
        let major = doc["javaVersion"]["majorVersion"].as_u64().unwrap_or(8) as u32;
        let java = java::ensure_java(&client, &p, major, None, &rep).await.unwrap();
        save_instance(&p, &inst).unwrap();
        let prep = launch::build(&p, &inst, &doc, java, &Account::default()).unwrap();
        launch::spawn("f", &prep).unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(40)).await;
        let alive = launch::is_running("f");
        launch::stop("f");
        let log = std::fs::read_to_string(game_dir(&p, "f").join("launcher.log")).unwrap_or_default();
        println!("alive={alive}");
        println!("{}", log.lines().rev().take(12).collect::<Vec<_>>().join(" || "));
        assert!(alive, "forge did not stay up");
    }
}
