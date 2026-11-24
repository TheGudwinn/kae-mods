//! Forge and NeoForge: their installers run once per version and leave a version profile behind.
use super::{e, fetch_file, java, versions, Instance, Paths, Report, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

const FORGE_META: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml";
const NEO_META: &str = "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";

fn xml_versions(xml: &str) -> Vec<String> {
    xml.split("<version>").skip(1).filter_map(|s| s.split("</version>").next()).map(|s| s.trim().to_string()).collect()
}

/// NeoForge numbers its builds after the game: `21.1.77` is for 1.21.1, `20.4.237` for 1.20.4.
fn neo_prefix(mc: &str) -> Option<String> {
    let mut it = mc.split('.');
    if it.next()? != "1" {
        return None;
    }
    let major = it.next()?;
    let minor = it.next().unwrap_or("0");
    Some(format!("{major}.{minor}."))
}

pub fn supports(loader: &str) -> bool {
    matches!(loader, "forge" | "neoforge")
}

/// Loader builds for one game version, newest first.
pub async fn versions_for(client: &reqwest::Client, loader: &str, mc: &str) -> Result<Vec<String>> {
    let (url, prefix) = match loader {
        "forge" => (FORGE_META, format!("{mc}-")),
        "neoforge" => (NEO_META, neo_prefix(mc).ok_or_else(|| format!("NeoForge has no build for Minecraft {mc}"))?),
        _ => return Ok(vec![]),
    };
    let xml = client.get(url).send().await.map_err(e)?.error_for_status().map_err(e)?.text().await.map_err(e)?;
    let mut out: Vec<String> = xml_versions(&xml).into_iter().filter(|v| v.starts_with(&prefix)).collect();
    out.reverse(); // the metadata lists the oldest first
    if loader == "forge" {
        out = out.into_iter().map(|v| v[prefix.len()..].to_string()).collect();
    }
    // releases before betas
    out.sort_by_key(|v| v.contains("beta") || v.contains("alpha"));
    Ok(out)
}

fn installer_url(loader: &str, mc: &str, lv: &str) -> String {
    match loader {
        "forge" => format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{mc}-{lv}/forge-{mc}-{lv}-installer.jar"),
        _ => format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{lv}/neoforge-{lv}-installer.jar"),
    }
}

/// Names the installer may give the version it creates.
fn candidate_ids(loader: &str, mc: &str, lv: &str) -> Vec<String> {
    match loader {
        "forge" => vec![format!("{mc}-forge-{lv}"), format!("{mc}-forge{mc}-{lv}")],
        _ => vec![format!("neoforge-{lv}")],
    }
}

fn existing(p: &Paths, ids: &[String]) -> Option<String> {
    ids.iter().find(|id| p.versions().join(id).join(format!("{id}.json")).exists()).cloned()
}

fn run_installer(java: &Path, jar: &Path, root: &Path) -> Result<()> {
    // the installer wants a launcher profile file in the target folder
    let profiles = root.join("launcher_profiles.json");
    if !profiles.exists() {
        std::fs::write(&profiles, r#"{"profiles":{},"selectedProfile":"","clientToken":"0","launcherVersion":{"name":"kae-mods","format":21}}"#).map_err(e)?;
    }
    let mut cmd = Command::new(java);
    cmd.arg("-jar").arg(jar).arg("--installClient").arg(root).current_dir(root);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|x| format!("could not start the installer: {x}"))?;
    if out.status.success() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    let tail: Vec<&str> = text.lines().rev().take(8).collect();
    Err(format!("the loader installer failed: {}", tail.into_iter().rev().collect::<Vec<_>>().join(" | ")))
}

/// Makes sure the version an instance launches exists on disk and returns its id.
/// Vanilla, Fabric and Quilt only need a JSON profile; Forge and NeoForge run their installer.
pub async fn ensure_version(client: &reqwest::Client, p: &Paths, inst: &Instance, report: Report<'_>) -> Result<String> {
    if !supports(&inst.loader) {
        return versions::ensure_version_json(client, p, &inst.mc_version, &inst.loader, inst.loader_version.as_deref()).await;
    }
    let mc = &inst.mc_version;
    let lv = match inst.loader_version.clone() {
        Some(v) => v,
        None => versions_for(client, &inst.loader, mc).await?.into_iter().next().ok_or_else(|| format!("no {} build for Minecraft {mc}", inst.loader))?,
    };
    let ids = candidate_ids(&inst.loader, mc, &lv);
    if let Some(id) = existing(p, &ids) {
        return Ok(id);
    }
    // the installer reads the vanilla game and needs the Java that game wants
    versions::ensure_version_json(client, p, mc, "vanilla", None).await?;
    let doc = versions::install_version(client, p, mc, report).await?;
    let major = doc["javaVersion"]["majorVersion"].as_u64().unwrap_or(8) as u32;
    let jv = java::ensure_java(client, p, major, inst.java_path.as_deref(), report).await?;
    let jar: PathBuf = p.root.join("installers").join(format!("{}-{mc}-{lv}-installer.jar", inst.loader));
    report("loader", 0, 1);
    fetch_file(client, &installer_url(&inst.loader, mc, &lv), &jar, None).await?;
    let root = p.root.clone();
    tokio::task::spawn_blocking(move || run_installer(&jv, &jar, &root)).await.map_err(e)??;
    report("loader", 1, 1);
    existing(p, &ids).ok_or_else(|| "the installer finished but left no version behind".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neoforge_numbers_follow_the_game() {
        assert_eq!(neo_prefix("1.21.1").as_deref(), Some("21.1."));
        assert_eq!(neo_prefix("1.21").as_deref(), Some("21.0."));
        assert_eq!(neo_prefix("1.20.4").as_deref(), Some("20.4."));
        assert_eq!(neo_prefix("26.1"), None);
    }

    #[test]
    fn metadata_is_parsed() {
        let v = xml_versions("<versions><version>1.20.1-47.2.0</version><version>1.20.1-47.3.0</version></versions>");
        assert_eq!(v, vec!["1.20.1-47.2.0", "1.20.1-47.3.0"]);
    }
}
