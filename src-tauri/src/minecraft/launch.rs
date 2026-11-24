//! Building the command line of a game and starting it.
use super::versions::{self, rules_allow};
use super::{e, game_dir, Account, Instance, Paths, Result};
use md5::{Digest, Md5};
use serde_json::Value;
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::{LazyLock, Mutex};

static RUNNING: LazyLock<Mutex<HashMap<String, Child>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// How the last run of an instance ended.
#[derive(Debug, Clone)]
pub struct Exit {
    pub code: Option<i32>,
    /// Stopped with the Stop button: not a crash.
    pub user: bool,
}

static EXITS: LazyLock<Mutex<HashMap<String, Exit>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn is_running(id: &str) -> bool {
    let mut m = RUNNING.lock().unwrap_or_else(|x| x.into_inner());
    let state = m.get_mut(id).map(|c| c.try_wait());
    match state {
        Some(Ok(None)) => true,
        Some(Ok(Some(status))) => {
            m.remove(id);
            EXITS.lock().unwrap_or_else(|x| x.into_inner()).insert(id.to_string(), Exit { code: status.code(), user: false });
            false
        }
        _ => {
            m.remove(id);
            false
        }
    }
}

pub fn stop(id: &str) {
    if let Some(mut c) = RUNNING.lock().unwrap_or_else(|x| x.into_inner()).remove(id) {
        let _ = c.kill();
        let _ = c.wait();
        EXITS.lock().unwrap_or_else(|x| x.into_inner()).insert(id.to_string(), Exit { code: None, user: true });
    }
}

/// The way the last run ended, once.
pub fn take_exit(id: &str) -> Option<Exit> {
    EXITS.lock().unwrap_or_else(|x| x.into_inner()).remove(id)
}

/// The same player id every offline launcher derives from the name, so worlds keep their owner.
pub fn offline_uuid(name: &str) -> String {
    let mut h: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
    h[6] = (h[6] & 0x0f) | 0x30;
    h[8] = (h[8] & 0x3f) | 0x80;
    let s = hex::encode(h);
    format!("{}-{}-{}-{}-{}", &s[..8], &s[8..12], &s[12..16], &s[16..20], &s[20..])
}

fn substitute(arg: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = arg.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("${{{k}}}"), v);
    }
    out
}

/// Arguments of one kind (`game` or `jvm`) after rules and placeholders.
fn collect(doc: &Value, kind: &str, vars: &HashMap<&str, String>) -> Vec<String> {
    let mut out = vec![];
    for a in doc["arguments"][kind].as_array().into_iter().flatten() {
        match a {
            Value::String(s) => out.push(substitute(s, vars)),
            Value::Object(_) if rules_allow(&a["rules"]) => match &a["value"] {
                Value::String(s) => out.push(substitute(s, vars)),
                Value::Array(list) => out.extend(list.iter().filter_map(|x| x.as_str()).map(|s| substitute(s, vars))),
                _ => {}
            },
            _ => {}
        }
    }
    out
}

pub struct Prepared {
    pub java: std::path::PathBuf,
    pub args: Vec<String>,
    pub cwd: std::path::PathBuf,
}

pub fn build(p: &Paths, inst: &Instance, doc: &Value, java: std::path::PathBuf, account: &Account) -> Result<Prepared> {
    let gdir = game_dir(p, &inst.id);
    std::fs::create_dir_all(&gdir).map_err(e)?;
    let libs = versions::library_artifacts(p, doc);
    let natives = p.instance(&inst.id).join("natives");
    let _ = std::fs::remove_dir_all(&natives);
    versions::extract_natives(&libs, &natives)?;
    // newer versions point `java.library.path` at `${natives_directory}/java`
    if doc["arguments"]["jvm"].to_string().contains("natives_directory}/java") {
        versions::extract_natives(&libs, &natives.join("java"))?;
    }

    let sep = if cfg!(windows) { ";" } else { ":" };
    let mut cp: Vec<String> = vec![];
    for a in libs.iter().filter(|a| !a.native) {
        let s = a.path.to_string_lossy().into_owned();
        if !cp.contains(&s) {
            cp.push(s);
        }
    }
    cp.push(versions::client_jar(p, doc).to_string_lossy().into_owned());
    let classpath = cp.join(sep);

    // a Microsoft sign-in starts the game as that account; otherwise it is an offline player
    let (player, uuid, token, user_type) = match &account.ms {
        Some(ms) => (ms.name.clone(), ms.uuid.clone(), ms.mc_token.clone(), "msa"),
        None => (account.name.clone(), offline_uuid(&account.name), "0".to_string(), "legacy"),
    };
    let assets_index = doc["assetIndex"]["id"].as_str().or(doc["assets"].as_str()).unwrap_or("legacy").to_string();
    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("auth_player_name", player);
    vars.insert("version_name", inst.version_id.clone());
    vars.insert("game_directory", gdir.to_string_lossy().into_owned());
    vars.insert("assets_root", p.assets().to_string_lossy().into_owned());
    vars.insert("game_assets", p.assets().join("virtual").join("legacy").to_string_lossy().into_owned());
    vars.insert("assets_index_name", assets_index);
    vars.insert("auth_uuid", uuid);
    vars.insert("auth_access_token", token);
    vars.insert("auth_session", "0".into());
    vars.insert("clientid", "0".into());
    vars.insert("auth_xuid", "0".into());
    vars.insert("user_type", user_type.into());
    vars.insert("user_properties", "{}".into());
    vars.insert("version_type", doc["type"].as_str().unwrap_or("release").into());
    vars.insert("natives_directory", natives.to_string_lossy().into_owned());
    vars.insert("launcher_name", "kae-mods".into());
    vars.insert("launcher_version", env!("CARGO_PKG_VERSION").into());
    vars.insert("classpath", classpath.clone());
    vars.insert("library_directory", p.libraries().to_string_lossy().into_owned());
    vars.insert("classpath_separator", sep.into());

    let main = doc["mainClass"].as_str().ok_or("the version has no main class")?;
    let memory = if inst.memory_custom { inst.memory_mb.max(512) } else { super::DEFAULT_MEMORY_MB };
    let mut args = vec![format!("-Xmx{memory}M"), "-Xms512M".to_string()];
    args.extend(inst.extra_args.split_whitespace().map(str::to_string));
    if doc["arguments"]["jvm"].is_array() {
        args.extend(collect(doc, "jvm", &vars));
    } else {
        // versions before 1.13 list no JVM arguments
        args.push(format!("-Djava.library.path={}", natives.display()));
        args.push("-cp".into());
        args.push(classpath);
    }
    args.push(main.to_string());
    if doc["arguments"]["game"].is_array() {
        args.extend(collect(doc, "game", &vars));
    } else if let Some(legacy) = doc["minecraftArguments"].as_str() {
        args.extend(legacy.split_whitespace().map(|s| substitute(s, &vars)));
    }
    Ok(Prepared { java, args, cwd: gdir })
}

pub fn spawn(id: &str, prep: &Prepared) -> Result<u32> {
    if is_running(id) {
        return Err("this instance is already running".into());
    }
    std::fs::create_dir_all(&prep.cwd).map_err(e)?;
    let log = std::fs::File::create(prep.cwd.join("launcher.log")).map_err(e)?;
    let log2 = log.try_clone().map_err(e)?;
    let mut cmd = Command::new(&prep.java);
    cmd.args(&prep.args).current_dir(&prep.cwd).stdout(Stdio::from(log)).stderr(Stdio::from(log2)).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let child = cmd.spawn().map_err(|x| format!("could not start Java: {x}"))?;
    let pid = child.id();
    RUNNING.lock().unwrap_or_else(|x| x.into_inner()).insert(id.to_string(), child);
    Ok(pid)
}
