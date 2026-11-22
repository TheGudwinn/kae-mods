//! 3D preview of heroes: the base game's model or the model a mod puts in its place.
use super::library::game;
use super::AppState;
use std::path::Path;
use std::sync::Arc;
use sha2::Digest;
use tauri::State;

const BINDINGS: &str = include_str!("../../assets/hero-model-bindings.json");

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// The model the game itself uses for a hero, from the table of hero code -> model.
fn base_models(code: &str) -> Vec<String> {
    let v: serde_json::Value = serde_json::from_str(BINDINGS).unwrap_or_default();
    v["bindings"][format!("hero_{code}")]
        .as_array()
        .map(|a| a.iter().filter_map(|p| p.as_str()).map(|p| format!("{p}_c")).collect())
        .unwrap_or_default()
}

/// The model the game's own hero table names (`scripts/heroes.vdata`), which follows game updates; the
/// bundled table is only the fallback.
fn game_models(base: &source2_model::vpk_extract::VpkArchive, code: &str) -> Vec<String> {
    let from_game = (|| {
        let bytes = base.extract_entry("scripts/heroes.vdata_c").ok()?;
        let res = source2_model::Resource::parse(bytes).ok()?;
        let data = source2_model::kv3::parse(res.block_bytes("DATA")?).ok()?;
        let path = data.get(&format!("hero_{code}"))?.get("m_strModelName")?.as_string()?.to_string();
        Some(format!("{}_c", path.to_ascii_lowercase()))
    })();
    match from_game {
        Some(p) => vec![p],
        None => base_models(code),
    }
}

/// The base game's model path of a hero.
pub fn base_model(base: &source2_model::vpk_extract::VpkArchive, code: &str) -> Option<String> {
    game_models(base, code).into_iter().next()
}

/// The main model of a hero inside one VPK.
fn pick_model(entries: &[String], codes: &[String], wanted: Vec<String>) -> Option<String> {
    let lower: Vec<String> = entries.iter().map(|e| e.to_ascii_lowercase()).collect();
    for w in &wanted {
        if let Some(i) = lower.iter().position(|e| e == w) {
            return Some(entries[i].clone());
        }
    }
    let stems: Vec<String> = wanted.iter().filter_map(|w| w.rsplit('/').next().map(str::to_string)).collect();
    let skip = ["arm", "gib", "ragdoll", "lod", "weapon", "viewmodel", "wip_"];
    let mut best: Option<&String> = None;
    for (i, e) in lower.iter().enumerate() {
        if !e.ends_with(".vmdl_c") || !e.starts_with("models/heroes") {
            continue;
        }
        let in_hero = codes.iter().any(|code| e.split('/').any(|s| s == code || s.starts_with(&format!("{code}_")) || s.starts_with(&format!("hero_{code}"))));
        if !in_hero {
            continue;
        }
        let file = e.rsplit('/').next().unwrap_or(e);
        if stems.iter().any(|s| s == file) {
            return Some(entries[i].clone());
        }
        if skip.iter().any(|s| file.contains(s)) {
            continue;
        }
        if best.map_or(true, |b| entries[i].len() < b.len()) {
            best = Some(&entries[i]);
        }
    }
    best.cloned()
}

/// Whether the files of a VPK include any under a model folder of the hero (materials, textures, ...).
fn touches_hero(entries: &[String], codes: &[String]) -> bool {
    entries.iter().any(|e| {
        let e = e.to_ascii_lowercase();
        e.starts_with("models/heroes") && codes.iter().any(|code| e.split('/').any(|s| s == code || s.starts_with(&format!("{code}_")) || s.starts_with(&format!("hero_{code}"))))
    })
}

/// The game's archive, the mod's archive when one is asked for (it falls back to the game's files), and the
/// model entry to draw: the mod's own model, or the game's when the mod only swaps materials and textures.
fn open_hero(citadel: &Path, addons: &Path, hero: &str, codes: &[String], addon: Option<&str>) -> Result<(Arc<source2_model::vpk_extract::VpkArchive>, Option<source2_model::vpk_extract::VpkArchive>, String), String> {
    use source2_model::vpk_extract::VpkArchive;
    let code = hero.trim_start_matches("hero_").to_ascii_lowercase();
    if code.is_empty() || !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("bad_hero".into());
    }
    let base = Arc::new(VpkArchive::open(&citadel.join("pak01_dir.vpk")).map_err(err)?);
    let wanted = game_models(&base, &code);
    let mut names: Vec<String> = codes.iter().map(|c| c.to_ascii_lowercase()).filter(|c| !c.is_empty() && c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_')).collect();
    names.push(code.clone());
    let own = match addon {
        None => None,
        Some(name) => {
            if name.contains(['/', '\\']) || name.contains("..") {
                return Err("bad_addon".into());
            }
            let mut path = addons.join(name);
            if !path.is_file() {
                path = addons.join(format!("{name}.disabled"));
            }
            if !path.is_file() {
                return Err("addon_missing".into());
            }
            Some(VpkArchive::open(Path::new(&path)).map_err(err)?.with_fallbacks(vec![base.clone()]))
        }
    };
    let entries = own.as_ref().unwrap_or(&*base).list_entries();
    let model = match pick_model(&entries, &names, wanted.clone()) {
        Some(m) => m,
        // A mod that only swaps textures or materials of the hero carries no model: the game's own model is
        // drawn, and the mod's files take the place of the game's where they have the same path.
        None if addon.is_some() && touches_hero(&entries, &names) => match wanted.into_iter().next() {
            Some(m) => m,
            None => return Err("no_model".into()),
        },
        None if addon.is_some() => return Err("no_model".into()),
        None => return Err("no_base_model".into()),
    };
    Ok((base, own, model))
}

/// Decodes one hero, standing still in the first frame of the game's idle clip; see `hero_model`. Its animations
/// are decoded separately (`hero_animation`) and played on top.
pub fn render(citadel: &Path, addons: &Path, hero: &str, codes: &[String], addon: Option<&str>) -> Result<Vec<u8>, String> {
    let (base, own, model) = open_hero(citadel, addons, hero, codes, addon)?;
    let archive = own.as_ref().unwrap_or(&*base);
    let out = source2_model::decode_model_glb_from_archive(archive, &model).map_err(err)?;
    // stand the hero in the game's idle pose instead of the stiff bind pose; the plain model is the fallback
    let out = source2_model::pose_model_still(out, &base, archive, &model);
    Ok(out.glb)
}

/// Like `render`, with the game's animation `clip` (a file stem) baked into the glTF instead of the still pose.
pub fn render_clip(citadel: &Path, addons: &Path, hero: &str, codes: &[String], addon: Option<&str>, clip: Option<&str>) -> Result<Vec<u8>, String> {
    let (base, own, model) = open_hero(citadel, addons, hero, codes, addon)?;
    let archive = own.as_ref().unwrap_or(&*base);
    let out = source2_model::decode_model_glb_from_archive(archive, &model).map_err(err)?;
    let out = source2_model::pose_model_with_clip(out, &base, archive, &model, clip);
    Ok(out.glb)
}

/// The hero as a binary glTF. `addon` is the file name of a mod in the game's `addons` folder; without it
/// the base game's model is drawn. Textures a mod does not carry are taken from the base game.
#[tauri::command]
pub async fn hero_model(state: State<'_, AppState>, hero: String, codes: Option<Vec<String>>, addon: Option<String>) -> Result<tauri::ipc::Response, String> {
    let g = game(&state)?;
    let cache = state.app_dir.join("hero-cache");
    tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let codes = codes.unwrap_or_default();
        // A decoded hero is kept on disk: the key changes when the game or the mod file changes.
        let stamp = |p: &Path| std::fs::metadata(p).ok().map(|m| format!("{}:{:?}", m.len(), m.modified().ok())).unwrap_or_default();
        let mut key = format!("v{CACHE_VERSION}|{hero}|{codes:?}|{}", stamp(&g.citadel_dir.join("pak01_dir.vpk")));
        if let Some(name) = &addon {
            let on = g.addons_dir.join(name);
            let path = if on.is_file() { on } else { g.addons_dir.join(format!("{name}.disabled")) };
            key.push_str(&format!("|{name}|{}", stamp(&path)));
        }
        let file = cache.join(format!("{}.glb", hex::encode(&sha2::Sha256::digest(key.as_bytes())[..12])));
        if let Ok(bytes) = std::fs::read(&file) {
            return Ok(bytes);
        }
        let glb = render(&g.citadel_dir, &g.addons_dir, &hero, &codes, addon.as_deref())?;
        if std::fs::create_dir_all(&cache).is_ok() {
            let _ = std::fs::write(&file, &glb);
            prune(&cache);
        }
        Ok(glb)
    })
    .await
    .map_err(err)?
    .map(tauri::ipc::Response::new)
}

/// One thing the hero can be shown doing: a movement, or an ability made of several clips (start, loop, end)
/// that play back to back.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimChoice {
    pub id: String,
    /// `movement` or `ability`.
    pub kind: &'static str,
    pub label: String,
    pub clips: Vec<String>,
    /// The ability name the clips were found by, when their names spell one.
    pub ability: Option<String>,
}

/// The bone tracks of one choice, on one timeline (see `hero_animation`).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimTracks {
    pub duration: f32,
    pub times: Vec<f32>,
    pub tracks: Vec<BoneTrack>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoneTrack {
    pub name: String,
    pub positions: Vec<f32>,
    pub rotations: Vec<f32>,
    pub scales: Vec<f32>,
}

fn clean_hero(hero: &str) -> Result<String, String> {
    let code = hero.trim_start_matches("hero_").to_ascii_lowercase();
    if code.is_empty() || !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("bad_hero".into());
    }
    Ok(code)
}

/// The movements and abilities of a hero's base-game model. Mods replace models, not animations, so every skin
/// of a hero plays the same ones. `ability_names` (the hero's abilities in English) find abilities whose clips are
/// not prefixed `ability_`.
#[tauri::command]
pub async fn hero_animations(state: State<'_, AppState>, hero: String, ability_names: Vec<String>) -> Result<Vec<AnimChoice>, String> {
    let g = game(&state)?;
    tokio::task::spawn_blocking(move || -> Result<Vec<AnimChoice>, String> {
        use source2_model::nm_anim::{self, PreviewActionKind};
        use source2_model::vpk_extract::VpkArchive;
        let code = clean_hero(&hero)?;
        let base = VpkArchive::open(&g.citadel_dir.join("pak01_dir.vpk")).map_err(err)?;
        let model = base_model(&base, &code).ok_or("no_base_model")?;
        let listing = base.list_entries();
        let paths = nm_anim::model_clip_paths(&base, &model, &listing);
        Ok(nm_anim::preview_actions(paths, &ability_names)
            .into_iter()
            .map(|a| {
                let (kind, label) = match a.kind {
                    PreviewActionKind::Movement => ("movement", a.id.clone()),
                    PreviewActionKind::Ability => {
                        let name = a.id.trim_start_matches("ability:").replace('_', " ");
                        let mut chars = name.chars();
                        ("ability", chars.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + chars.as_str()))
                    }
                };
                AnimChoice { id: a.id, kind, label, clips: a.clips, ability: a.ability }
            })
            .collect())
    })
    .await
    .map_err(err)?
}

const SAMPLE_RATE: f32 = 30.0;
const STATIC_CLIP_SECONDS: f32 = 0.1;
const MAX_SEQUENCE_SECONDS: f32 = 30.0;
const MAX_SAMPLED_TRANSFORMS: usize = 180_000;
const MAX_ANIMATED_BONES: usize = 2048;

/// Decodes `clip_paths` back to back into one timeline of bone tracks, retargeted to the skeleton of the hero's
/// model (the mod's own when the skin ships one), so an action made of several clips plays as one animation.
#[tauri::command]
pub async fn hero_animation(state: State<'_, AppState>, hero: String, codes: Option<Vec<String>>, addon: Option<String>, clip_paths: Vec<String>) -> Result<AnimTracks, String> {
    let g = game(&state)?;
    tokio::task::spawn_blocking(move || -> Result<AnimTracks, String> {
        use source2_model::{nm_anim, skeleton, Resource};
        if clip_paths.is_empty() || clip_paths.len() > nm_anim::MAX_PREVIEW_SEQUENCE_CLIPS {
            return Err("bad_clips".into());
        }
        if clip_paths.iter().any(|p| !p.ends_with(".vnmclip_c") || p.contains("..")) {
            return Err("bad_clips".into());
        }
        let (base, own, model) = open_hero(&g.citadel_dir, &g.addons_dir, &hero, &codes.unwrap_or_default(), addon.as_deref())?;
        let archive = own.as_ref().unwrap_or(&*base);
        let resource = Resource::parse(archive.extract_entry(&model).map_err(err)?).map_err(err)?;
        let skeleton = skeleton::parse_model_skeleton(&resource).map_err(err)?.ok_or("no_skeleton")?;
        let mut clips = Vec::with_capacity(clip_paths.len());
        for path in &clip_paths {
            let animation = nm_anim::load_nm_animation_from_archive(archive, path).map_err(err)?;
            if animation.clip.additive {
                return Err("additive_clip".into());
            }
            if !skeleton.bones.iter().any(|b| animation.skeleton.bone_names.iter().any(|n| n.eq_ignore_ascii_case(&b.name))) {
                return Err("skeleton_mismatch".into());
            }
            clips.push(animation);
        }
        // the bones that need tracks: those the clips move and the cloth roots pinned to them; the rest hold their
        // pose, which the loaded model already has
        let mut driven: Vec<bool> = skeleton.bones.iter().map(skeleton::Bone::is_procedural_cloth_root).collect();
        for animation in &clips {
            for (d, r) in driven.iter_mut().zip(nm_anim::retargeted_bones(&animation.skeleton.bone_names, &skeleton)) {
                *d |= r;
            }
        }
        let driven_count = driven.iter().filter(|d| **d).count();
        if driven_count == 0 || driven_count > MAX_ANIMATED_BONES {
            return Err("too_many_bones".into());
        }
        let plan = sampling_plan(&clips.iter().map(|a| (a.clip.frame_count, a.clip.duration_seconds)).collect::<Vec<_>>(), driven_count)?;
        let total_frames: usize = plan.iter().map(|(_, n)| *n).sum();
        let mut times = Vec::with_capacity(total_frames);
        let mut tracks: Vec<BoneTrack> = skeleton
            .bones
            .iter()
            .zip(&driven)
            .filter(|(_, d)| **d)
            .map(|(b, _)| BoneTrack { name: b.name.clone(), positions: Vec::with_capacity(total_frames * 3), rotations: Vec::with_capacity(total_frames * 4), scales: Vec::with_capacity(total_frames * 3) })
            .collect();
        let mut offset = 0.0f32;
        for (animation, (duration, samples)) in clips.iter().zip(plan) {
            for frame in 0..samples {
                let time = duration * frame as f32 / (samples - 1) as f32;
                times.push(offset + time);
                let sampled = animation.sample_pose(time, false).map_err(err)?;
                let mut pose = nm_anim::retarget_nm_pose(animation, &sampled, &skeleton);
                skeleton.pin_procedural_cloth_roots(&mut pose);
                let driven_pose = pose.into_iter().zip(&driven).filter_map(|(t, d)| d.then_some(t));
                for (track, t) in tracks.iter_mut().zip(driven_pose) {
                    if !t.translation.iter().chain(&t.rotation).chain(&t.scale).all(|v| v.is_finite()) {
                        return Err("bad_transform".into());
                    }
                    track.positions.extend(t.translation);
                    track.rotations.extend(t.rotation);
                    track.scales.extend(t.scale);
                }
            }
            // one frame of blend between clips keeps keyframe times strictly increasing
            offset += duration + 1.0 / SAMPLE_RATE;
        }
        Ok(AnimTracks { duration: offset - 1.0 / SAMPLE_RATE, times, tracks })
    })
    .await
    .map_err(err)?
}

/// Per clip: its length in seconds and how many samples it gets. Every clip keeps both endpoints; when the whole
/// sequence would be too heavy the samples are shared out proportionally, never shortening the motion.
fn sampling_plan(clips: &[(usize, f32)], bone_count: usize) -> Result<Vec<(f32, usize)>, String> {
    if clips.is_empty() || bone_count == 0 {
        return Err("bad_clips".into());
    }
    let mut plan = Vec::with_capacity(clips.len());
    for &(frames, duration) in clips {
        if frames == 0 {
            return Err("empty_clip".into());
        }
        let duration = if frames == 1 && duration == 0.0 {
            STATIC_CLIP_SECONDS
        } else if !duration.is_finite() || duration <= f32::EPSILON {
            return Err("bad_duration".into());
        } else {
            duration
        };
        let samples = if frames > 1 { (duration * SAMPLE_RATE).ceil() as usize + 1 } else { 2 };
        plan.push((duration, samples));
    }
    let total_seconds = plan.iter().map(|(d, _)| *d).sum::<f32>() + (clips.len() - 1) as f32 / SAMPLE_RATE;
    if total_seconds > MAX_SEQUENCE_SECONDS {
        return Err("too_long".into());
    }
    let budget = (MAX_SAMPLED_TRANSFORMS / bone_count).checked_sub(2 * clips.len()).ok_or("too_complex")?;
    let wanted: usize = plan.iter().map(|(_, n)| n - 2).sum();
    if wanted > budget {
        for (_, n) in &mut plan {
            *n = 2 + (*n - 2) * budget / wanted;
        }
    }
    Ok(plan)
}

/// Animations that mean something to a person looking at a skin. Directional run variants, aim offsets and
/// additive layers are only building blocks of the game's animation graph.
fn playable(stem: &str) -> bool {
    const SKIP_START: &[&str] = &["aim_", "loco_", "corner_lean", "item_", "weapon_run", "weapon_crouch", "out_of_combat_run", "out_of_combat_crouch_run", "airdrop_", "zipline", "rope_", "mantle", "wall_", "turn_", "death_", "flinch", "knockdown_in_air", "kockdown"];
    const SKIP_END: &[&str] = &["_n", "_ne", "_nw", "_e", "_w", "_s", "_se", "_sw", "_center", "_c", "_copy"];
    !stem.contains('+') && !SKIP_START.iter().any(|p| stem.starts_with(p)) && !SKIP_END.iter().any(|p| stem.ends_with(p))
}

/// The clips of a hero's base-game model that are worth showing, by file stem. Mods replace models, not
/// animations, so every skin of a hero plays the same ones.
#[tauri::command]
pub async fn hero_clips(state: State<'_, AppState>, hero: String) -> Result<Vec<String>, String> {
    let g = game(&state)?;
    tokio::task::spawn_blocking(move || -> Result<Vec<String>, String> {
        use source2_model::vpk_extract::VpkArchive;
        let code = hero.trim_start_matches("hero_").to_ascii_lowercase();
        if code.is_empty() || !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err("bad_hero".into());
        }
        let base = VpkArchive::open(&g.citadel_dir.join("pak01_dir.vpk")).map_err(err)?;
        let model = game_models(&base, &code).into_iter().next().ok_or("no_base_model")?;
        let listing = base.list_entries();
        let mut out: Vec<String> = source2_model::nm_anim::model_clip_paths(&base, &model, &listing)
            .iter()
            .filter_map(|p| p.rsplit('/').next())
            .filter_map(|f| f.strip_suffix(".vnmclip_c"))
            .filter(|s| playable(s))
            .map(str::to_string)
            .collect();
        out.sort();
        out.dedup();
        Ok(out)
    })
    .await
    .map_err(err)?
}

/// Bumped when the decoder changes what it writes, so old cache files are not used.
const CACHE_VERSION: u32 = 7;
const CACHE_KEEP: usize = 40;

/// Keeps the newest cached models only.
fn prune(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<_> = rd.flatten().filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).collect();
    if files.len() <= CACHE_KEEP {
        return;
    }
    files.sort();
    for (_, path) in &files[..files.len() - CACHE_KEEP] {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    /// Needs the real game; writes the model to `D:/ref` so it can be opened in a viewer.
    #[test]
    #[ignore]
    fn real_hero() {
        let citadel = std::path::Path::new("D:/SteamLibrary/steamapps/common/Deadlock/game/citadel");
        let addons = citadel.join("addons");
        for h in ["tengu", "lash", "orion", "necro"] {
            let t0 = std::time::Instant::now();
            let glb = super::render(citadel, &addons, h, &[], None).unwrap();
            println!("{h}: {:?}", t0.elapsed());
            std::fs::write(format!("D:/ref/{h}.glb"), &glb).unwrap();
            println!("{h}: {} bytes", glb.len());
        }
    }

    #[test]
    #[ignore]
    fn dump_model() {
        let citadel = std::path::Path::new("D:/SteamLibrary/steamapps/common/Deadlock/game/citadel");
        let a = source2_model::vpk_extract::VpkArchive::open(&citadel.join("pak01_dir.vpk")).unwrap();
        let b = a.extract_entry("scripts/heroes.vdata_c").unwrap();
        let res = source2_model::Resource::parse(b).unwrap();
        let v = source2_model::kv3::parse(res.block_bytes("DATA").unwrap()).unwrap();
        std::fs::write("D:/ref/heroes_vdata.json", serde_json::to_string_pretty(&v.to_json()).unwrap()).unwrap();
    }

    #[test]
    #[ignore]
    fn survey() {
        use source2_model::vpk_extract::VpkArchive;
        use std::collections::BTreeMap;
        let dir = std::path::Path::new("D:/SteamLibrary/steamapps/common/Deadlock/game/citadel/addons");
        for a in crate::library::scan::scan_addons(dir).unwrap() {
            let Ok(files) = &a.entries else { continue };
            let content = crate::library::describe::describe(files);
            let heroes: Vec<String> = content.heroes.iter().map(|h| format!("{}:{}", h.code, h.files)).collect();
            if heroes.is_empty() { continue; }
            let Ok(arc) = VpkArchive::open(&a.path) else { continue };
            let mut formats: BTreeMap<String, usize> = BTreeMap::new();
            let mut failed = 0;
            for e in arc.list_entries().iter().filter(|e| e.ends_with(".vtex_c") && e.contains("models/")) {
                let Ok(bytes) = arc.extract_entry(e) else { continue };
                let Ok(res) = source2_model::Resource::parse(bytes) else { failed += 1; continue };
                match source2_model::VtexHeader::parse(&res) {
                    Ok(h) => {
                        let ok = h.decode(&res.data).is_ok();
                        *formats.entry(format!("{:?}{}", h.format, if ok { "" } else { "(FAIL)" })).or_default() += 1;
                    }
                    Err(_) => failed += 1,
                }
            }
            println!("SURVEY {} {:?} {:?} hdrfail={}", a.file_name, heroes, formats, failed);
        }
    }

    /// Lists every material of the mods for the heroes named in DIAG_HEROES and whether it got a colour texture.
    #[test]
    #[ignore]
    fn diag() {
        let citadel = std::path::Path::new("D:/SteamLibrary/steamapps/common/Deadlock/game/citadel");
        let addons = citadel.join("addons");
        let want = ["chrono", "mirage", "vampirebat", "familiar"];
        for a in crate::library::scan::scan_addons(&addons).unwrap() {
            let Ok(files) = &a.entries else { continue };
            let content = crate::library::describe::describe(files);
            for h in &content.model_heroes {
                if !want.iter().any(|w| h.code.starts_with(w)) { continue; }
                let hero = want.iter().find(|w| h.code.starts_with(*w)).unwrap();
                match super::render(citadel, &addons, hero, &[h.code.clone()], Some(&a.file_name)) {
                    Ok(glb) => {
                        let len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
                        let json: serde_json::Value = serde_json::from_slice(&glb[20..20 + len]).unwrap();
                        std::fs::write(format!("D:/ref/d_{}.glb", a.file_name), &glb).unwrap(); println!("DIAG {} {} {} KB", a.file_name, h.code, glb.len() / 1024);
                        for m in json["materials"].as_array().unwrap() {
                            let pbr = &m["pbrMetallicRoughness"];
                            println!("   mat {} tex={} factor={} extras={}", m["name"], pbr["baseColorTexture"], pbr["baseColorFactor"], m["extras"]);
                        }
                    }
                    Err(e) => println!("DIAG {} {} ERR {e}", a.file_name, h.code),
                }
            }
        }
    }

    /// Dumps the materials and textures of the Paradox mod whose names contain `pentagram` / `rune` / `headglass`.
    #[test]
    #[ignore]
    fn paradox() {
        use source2_model::vpk_extract::VpkArchive;
        let arc = VpkArchive::open(std::path::Path::new("D:/SteamLibrary/steamapps/common/Deadlock/game/citadel/addons/pak34_dir.vpk")).unwrap();
        for e in arc.list_entries() {
            let hit = ["pentagram", "rune_glow", "headglass", "horns"].iter().any(|k| e.contains(k));
            if !hit { continue; }
            let name = e.rsplit('/').next().unwrap().to_string();
            let res = source2_model::Resource::parse(arc.extract_entry(&e).unwrap()).unwrap();
            if e.ends_with(".vmat_c") {
                let v = source2_model::kv3::parse(res.block_bytes("DATA").unwrap()).unwrap();
                std::fs::write(format!("D:/ref/pd_{name}.json"), serde_json::to_string_pretty(&v.to_json()).unwrap()).unwrap();
            } else if e.ends_with(".vtex_c") {
                let h = source2_model::VtexHeader::parse(&res).unwrap();
                if let Ok(d) = h.decode(&res.data) {
                    std::fs::write(format!("D:/ref/pd_{name}.{}x{}.rgba", d.width, d.height), &d.rgba).unwrap();
                    println!("PD {name} {}x{}", d.width, d.height);
                }
            }
        }
    }

    #[test]
    #[ignore]
    fn dump_mods() {
        let citadel = std::path::Path::new("D:/SteamLibrary/steamapps/common/Deadlock/game/citadel");
        let addons = citadel.join("addons");
        for (hero, code, file) in [
            ("hornet", "hornet_v3", "pak04_dir.vpk"),
            ("necro", "necro", "pak06_dir.vpk"),
            ("vampirebat", "vampirebat", "pak42_dir.vpk"),
            ("mirage", "mirage_v2", "pak38_dir.vpk"),
            ("chrono", "chrono", "pak34_dir.vpk"),
            ("wraith", "wraith", "pak29_dir.vpk"),
            ("familiar", "familiar", "pak27_dir.vpk"),
        ] {
            let t0 = std::time::Instant::now();
            match super::render(citadel, &addons, hero, &[code.to_string()], Some(file)) {
                Ok(glb) => {
                    std::fs::write(format!("D:/ref/m_{hero}.glb"), &glb).unwrap();
                    println!("DUMP {hero} {file} {} KB {:?}", glb.len() / 1024, t0.elapsed());
                }
                Err(e) => println!("DUMP {hero} {file} ERR {e}"),
            }
        }
    }

    #[test]
    #[ignore]
    fn tex_header() {
        use source2_model::vpk_extract::VpkArchive;
        let dir = std::path::Path::new("D:/SteamLibrary/steamapps/common/Deadlock/game/citadel/addons");
        for (file, needle) in [("pak34_dir.vpk", "chrono_gun_v2_vmat_g_tcolor"), ("pak34_dir.vpk", "headglass_vmat_g_tcolor"), ("pak27_dir.vpk", "watcher_face_vmat_g_tcolor"), ("pak42_dir.vpk", "t_r2t1lucymd10011face01_d"), ("pak38_dir.vpk", "gun_color")] {
            let arc = VpkArchive::open(&dir.join(file)).unwrap();
            for e in arc.list_entries().iter().filter(|e| e.contains(needle) && e.ends_with(".vtex_c")).take(2) {
                let res = source2_model::Resource::parse(arc.extract_entry(e).unwrap()).unwrap();
                let h = source2_model::VtexHeader::parse(&res).unwrap();
                let mut unc = 0;
                for l in 0..h.num_mip_levels.max(1) as u32 {
                    unc += h.format.mip_byte_size((h.width as u32 >> l).max(1), (h.height as u32 >> l).max(1)).unwrap_or(0);
                }
                println!("TEX {file} {e}\n   v{} flags={:#x} {}x{} d{} {:?} mips={} extra={:?} data_off={} data_len={} sum_unc={unc}",
                    h.version, h.flags, h.width, h.height, h.depth, h.format, h.num_mip_levels, h.extra, h.data_offset, res.data.len(), );
            }
        }
    }

    #[test]
    #[ignore]
    fn clips() {
        use source2_model::vpk_extract::VpkArchive;
        let citadel = std::path::Path::new("D:/SteamLibrary/steamapps/common/Deadlock/game/citadel");
        let base = VpkArchive::open(&citadel.join("pak01_dir.vpk")).unwrap();
        for code in ["lash", "orion", "inferno", "tengu", "hornet", "nano", "necro", "vampirebat", "mirage", "wraith"] {
            let model = super::game_models(&base, code).remove(0);
            for clip in [Some("weapon_stand_idle"), Some("idle"), Some("stand_idle"), None] {
                match source2_model::nm_anim::resolve_nm_clip_from_archive(&base, &model, clip, None) {
                    Ok(Some(r)) => {
                        let a = source2_model::nm_anim::load_nm_animation_from_archive(&base, &r.clip_path).unwrap();
                        println!("CLIP {code} want={clip:?} -> {} frames={} dur={:.2}s fps={:.1}", r.clip_path, a.clip.frame_count, a.clip.duration_seconds, a.fps());
                    }
                    Ok(None) => println!("CLIP {code} want={clip:?} -> none"),
                    Err(e) => println!("CLIP {code} want={clip:?} ERR {e}"),
                }
            }
        }
    }

    #[test]
    #[ignore]
    fn clip_names() {
        use source2_model::vpk_extract::VpkArchive;
        let citadel = std::path::Path::new("D:/SteamLibrary/steamapps/common/Deadlock/game/citadel");
        let base = VpkArchive::open(&citadel.join("pak01_dir.vpk")).unwrap();
        let listing = base.list_entries();
        for code in ["lash", "tengu", "orion"] {
            let model = super::game_models(&base, code).remove(0);
            let clips = source2_model::nm_anim::model_clip_paths(&base, &model, &listing);
            let names: Vec<&str> = clips.iter().map(|c| c.rsplit('/').next().unwrap()).collect();
            println!("NAMES {code} {} clips: {}", names.len(), names.join(" "));
        }
    }

    #[test]
    #[ignore]
    fn clip_render() {
        let citadel = std::path::Path::new("D:/SteamLibrary/steamapps/common/Deadlock/game/citadel");
        let addons = citadel.join("addons");
        for (h, c) in [("lash", Some("ability_flog")), ("lash", None), ("orion", Some("ability_guided_arrow_fire")), ("orion", None), ("orion", Some("ability_generic_cast")), ("orion", Some("ability_power_jump_leap_up"))] {
            let t0 = std::time::Instant::now();
            let glb = super::render_clip(citadel, &addons, h, &[], None, c).unwrap();
            let len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
            let json: serde_json::Value = serde_json::from_slice(&glb[20..20 + len]).unwrap();
            println!("CLIP {h} {c:?} {:?} {} KB anims={}", t0.elapsed(), glb.len() / 1024, json["animations"].as_array().map(|a| a.len()).unwrap_or(0));
        }
    }

    #[test]
    #[ignore]
    fn dump_abilities() {
        let citadel = std::path::Path::new("D:/SteamLibrary/steamapps/common/Deadlock/game/citadel");
        let a = source2_model::vpk_extract::VpkArchive::open(&citadel.join("pak01_dir.vpk")).unwrap();
        for e in a.list_entries().iter().filter(|e| e.contains("vdata_c") && e.starts_with("scripts/")) {
            println!("VDATA {e}");
        }
        let b = a.extract_entry("scripts/abilities.vdata_c").unwrap();
        let res = source2_model::Resource::parse(b).unwrap();
        let v = source2_model::kv3::parse(res.block_bytes("DATA").unwrap()).unwrap();
        std::fs::create_dir_all("D:/ref").unwrap();
        std::fs::write("D:/ref/abilities_vdata.json", serde_json::to_string_pretty(&v.to_json()).unwrap()).unwrap();
    }

    #[test]
    #[ignore]
    fn skel_cmp() {
        use source2_model::vpk_extract::VpkArchive;
        use std::collections::HashMap;
        let citadel = std::path::Path::new("D:/SteamLibrary/steamapps/common/Deadlock/game/citadel");
        let base = VpkArchive::open(&citadel.join("pak01_dir.vpk")).unwrap();
        let modp = VpkArchive::open(&citadel.join("guddion_compat/pak01_dir.vpk")).unwrap();
        let path = "models/heroes_wip/inferno/inferno.vmdl_c";
        let load = |a: &VpkArchive| {
            let res = source2_model::Resource::parse(a.extract_entry(path).unwrap()).unwrap();
            source2_model::skeleton::parse_model_skeleton(&res).unwrap().unwrap()
        };
        let (g, m) = (load(&base), load(&modp));
        println!("SK bones game={} mod={}", g.bones.len(), m.bones.len());
        let same_idx = g.bones.iter().zip(&m.bones).take_while(|(a, b)| a.name == b.name).count();
        let moved = g.bones.iter().enumerate().filter(|(i, b)| m.bones.get(*i).map(|x| x.name != b.name).unwrap_or(true)).count();
        println!("SK first_index_mismatch={same_idx} bones_at_other_index={moved} mod[{same_idx}]={:?} game[{same_idx}]={:?}", m.bones.get(same_idx).map(|b| &b.name), g.bones.get(same_idx).map(|b| &b.name));
        let gi: HashMap<&str, usize> = g.bones.iter().enumerate().map(|(i, b)| (b.name.as_str(), i)).collect();
        let mi: HashMap<&str, usize> = m.bones.iter().enumerate().map(|(i, b)| (b.name.as_str(), i)).collect();
        let only_g: Vec<&&str> = gi.keys().filter(|k| !mi.contains_key(**k)).collect();
        let only_m: Vec<&&str> = mi.keys().filter(|k| !gi.contains_key(**k)).collect();
        println!("SK only_game({})={:?}", only_g.len(), &only_g[..only_g.len().min(25)]);
        println!("SK only_mod({})={:?}", only_m.len(), &only_m[..only_m.len().min(25)]);
        let mut parent_diff = 0; let mut rot_diff = 0; let mut pos_diff = 0;
        for (name, &i) in &mi {
            let Some(&j) = gi.get(name) else { continue };
            let (a, b) = (&m.bones[i], &g.bones[j]);
            let pn = |s: &source2_model::Skeleton, b: &source2_model::Bone| if b.parent < 0 { "-".to_string() } else { s.bones[b.parent as usize].name.clone() };
            if pn(&m, a) != pn(&g, b) { parent_diff += 1; if parent_diff <= 5 { println!("SK parent {name}: mod={} game={}", pn(&m, a), pn(&g, b)); } }
            let dr: f32 = a.bind_rot.iter().zip(&b.bind_rot).map(|(x, y)| (x - y).abs()).sum();
            let dp: f32 = a.bind_pos.iter().zip(&b.bind_pos).map(|(x, y)| (x - y).abs()).sum();
            if dr > 0.05 { rot_diff += 1; if rot_diff <= 8 { println!("SK rot {name}: mod={:?} game={:?}", a.bind_rot, b.bind_rot); } }
            if dp > 0.5 { pos_diff += 1; if pos_diff <= 8 { println!("SK pos {name}: mod={:?} game={:?}", a.bind_pos, b.bind_pos); } }
        }
        println!("SK parent_diff={parent_diff} rot_diff={rot_diff} pos_diff={pos_diff}");
        println!("SK root mod={:?} game={:?}", (&m.bones[0].name, m.bones[0].bind_rot), (&g.bones[0].name, g.bones[0].bind_rot));
    }

    #[test]
    #[ignore]
    fn watcher_glb() {
        let citadel = std::path::Path::new("D:/SteamLibrary/steamapps/common/Deadlock/game/citadel");
        let addons = citadel.join("addons");
        let hero = std::env::var("MDUMP_HERO").unwrap_or("familiar".into());
        let file = std::env::var("MDUMP_FILE").unwrap_or("pak27_dir.vpk".into());
        let glb = super::render(citadel, &addons, &hero, &[hero.clone()], Some(&file)).unwrap();
        let len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
        let j: serde_json::Value = serde_json::from_slice(&glb[20..20 + len]).unwrap();
        for (i, m) in j["meshes"].as_array().unwrap().iter().enumerate() {
            let prims: Vec<String> = m["primitives"].as_array().unwrap().iter().map(|p| format!("mat{}", p["material"])).collect();
            println!("WG mesh{i} {} {prims:?}", m["name"]);
        }
        for (i, m) in j["materials"].as_array().unwrap().iter().enumerate() {
            let fx = &m["extras"]["source2Effect"];
            let pick = |o: &serde_json::Value, pat: &[&str]| o.as_object().map(|o| o.iter().filter(|(k, _)| pat.iter().any(|p| k.to_ascii_lowercase().contains(p))).map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(",")).unwrap_or_default();
            let flags = fx["flags"].as_object().map(|o| o.iter().filter(|(k, _)| k.starts_with("F_")).map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(",")).unwrap_or_default();
            println!("WG mat{i} {} alpha={} base={} emissive={} emStr={} emTex={} cull2={}\n     flags={flags}\n     vec={}\n     float={}",
                m["name"].as_str().unwrap_or("").rsplit('/').next().unwrap_or(""), m["alphaMode"], m["pbrMetallicRoughness"]["baseColorFactor"], m["emissiveFactor"], m["extensions"]["KHR_materials_emissive_strength"]["emissiveStrength"], m["emissiveTexture"], m["doubleSided"],
                pick(&fx["vectors"], &["selfillumtint", "colortint", "transmiss", "fresnel"]), pick(&fx["floats"], &["selfillumscale", "opacity", "albedofactor"]));
        }
    }

    #[test]
    #[ignore]
    fn hero_flags() {
        let citadel = std::path::Path::new("D:/SteamLibrary/steamapps/common/Deadlock/game/citadel");
        let base = source2_model::vpk_extract::VpkArchive::open(&citadel.join("pak01_dir.vpk")).unwrap();
        let res = source2_model::Resource::parse(base.extract_entry("scripts/heroes.vdata_c").unwrap()).unwrap();
        let data = source2_model::kv3::parse(res.block_bytes("DATA").unwrap()).unwrap();
        let j = data.to_json();
        for (k, v) in j.as_object().unwrap() {
            if !k.starts_with("hero_") { continue; }
            let flags: Vec<String> = v.as_object().map(|o| o.iter().filter(|(n, x)| n.starts_with("m_b") && x.is_boolean()).filter(|(n, _)| ["disabled", "selectable", "development", "released", "hidden", "enabled", "available", "playable", "testing"].iter().any(|w| n.to_ascii_lowercase().contains(w))).map(|(n, x)| format!("{n}={x}")).collect()).unwrap_or_default();
            println!("HF {k} {}", flags.join(" "));
        }
    }

    #[test]
    #[ignore]
    fn shader_list() {
        use source2_model::vpk_extract::VpkArchive;
        let game = std::path::Path::new("D:/SteamLibrary/steamapps/common/Deadlock/game");
        for f in ["citadel/shaders_vulkan_dir.vpk", "core/shaders_vulkan_dir.vpk", "citadel/shaders_pc_dir.vpk"] {
            let arc = VpkArchive::open(&game.join(f)).unwrap();
            let all = arc.list_entries();
            println!("SH {f} entries={}", all.len());
            for e in all.iter().filter(|e| e.to_ascii_lowercase().contains("pbr")).take(40) {
                println!("SH   {e}");
            }
        }
    }

    #[test]
    #[ignore]
    fn rem_tex() {
        use source2_model::vpk_extract::VpkArchive;
        let citadel = std::path::Path::new("D:/SteamLibrary/steamapps/common/Deadlock/game/citadel");
        for (label, file) in [("GAME", "pak01_dir.vpk"), ("MOD", "addons/pak27_dir.vpk")] {
            let arc = VpkArchive::open(&citadel.join(file)).unwrap();
            for e in arc.list_entries().iter().filter(|e| e.ends_with(".vtex_c") && e.contains("familiar") || e.ends_with(".vtex_c") && e.contains("watcher")) {
                if !(e.contains("rough") || e.contains("color") || e.contains("normal")) { continue; }
                let Ok(bytes) = arc.extract_entry(e) else { continue };
                let Ok(res) = source2_model::Resource::parse(bytes) else { continue };
                let Ok(h) = source2_model::VtexHeader::parse(&res) else { continue };
                let Ok(d) = h.decode(&res.data) else { continue };
                let n = (d.width * d.height) as usize;
                let mut st = [(255u8, 0u8, 0u64); 4];
                for p in d.rgba.chunks_exact(4).take(n) {
                    for c in 0..4 { st[c].0 = st[c].0.min(p[c]); st[c].1 = st[c].1.max(p[c]); st[c].2 += p[c] as u64; }
                }
                let s: Vec<String> = st.iter().map(|(lo, hi, sum)| format!("{lo}..{hi} avg {}", sum / n.max(1) as u64)).collect();
                println!("TEX {label} {} {:?} {}x{} R[{}] G[{}] B[{}] A[{}]", e.rsplit('/').next().unwrap(), h.format, d.width, d.height, s[0], s[1], s[2], s[3]);
            }
        }
    }

    #[test]
    #[ignore]
    fn rem_mats() {
        use source2_model::vpk_extract::VpkArchive;
        let citadel = std::path::Path::new("D:/SteamLibrary/steamapps/common/Deadlock/game/citadel");
        for (label, file) in [("GAME", "pak01_dir.vpk"), ("MOD", "addons/pak27_dir.vpk")] {
            let arc = VpkArchive::open(&citadel.join(file)).unwrap();
            for e in arc.list_entries().iter().filter(|e| e.ends_with(".vmat_c") && e.contains("familiar") && e.contains("heroes")).take(40) {
                let Ok(bytes) = arc.extract_entry(e) else { continue };
                let Ok(res) = source2_model::Resource::parse(bytes) else { continue };
                let Some(b) = res.block_bytes("DATA") else { continue };
                let Ok(v) = source2_model::kv3::parse(b) else { continue };
                let j = v.to_json();
                let tex: Vec<String> = j["m_textureParams"].as_array().map(|a| a.iter().map(|t| format!("{}={}", t["m_name"].as_str().unwrap_or(""), t["m_pValue"].as_str().unwrap_or("").rsplit('/').next().unwrap_or(""))).collect()).unwrap_or_default();
                let fl: Vec<String> = j["m_floatParams"].as_array().map(|a| a.iter().map(|t| format!("{}={}", t["m_name"].as_str().unwrap_or(""), t["m_flValue"])).collect()).unwrap_or_default();
                let ints: Vec<String> = j["m_intParams"].as_array().map(|a| a.iter().map(|t| format!("{}={}", t["m_name"].as_str().unwrap_or(""), t["m_nValue"])).collect()).unwrap_or_default();
                println!("REM {label} {e}\n   shader={}\n   tex={tex:?}\n   float={fl:?}\n   int={ints:?}", j["m_shaderName"]);
            }
        }
    }

    #[test]
    #[ignore]
    fn all_mods() {
        use source2_model::vpk_extract::VpkArchive;
        let citadel = std::path::Path::new("D:/SteamLibrary/steamapps/common/Deadlock/game/citadel");
        let addons = citadel.join("addons");
        let mut files: Vec<String> = std::fs::read_dir(&addons).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.to_ascii_lowercase().contains(".vpk") && n.contains(&std::env::var("ALL_FILTER").unwrap_or_default())).collect();
        files.sort();
        for f in files {
            let arc = match VpkArchive::open(&addons.join(&f)) {
                Ok(a) => a,
                Err(e) => {
                    println!("ALL {f} OPEN_ERR {e}");
                    continue;
                }
            };
            let mut codes: Vec<String> = arc.list_entries().iter().filter(|e| e.starts_with("models/heroes")).filter_map(|e| e.split('/').nth(2).map(str::to_string)).collect();
            codes.sort();
            codes.dedup();
            if codes.is_empty() {
                let all = arc.list_entries();
                let models: Vec<&String> = all.iter().filter(|e| e.ends_with(".vmdl_c")).take(12).collect();
                println!("ALL {f} NO_HERO_MODEL entries={} vmdl={models:?} first={:?}", all.len(), all.iter().take(8).collect::<Vec<_>>());
            }
            for code in codes {
                let t0 = std::time::Instant::now();
                let hero = code.clone();
                match std::panic::catch_unwind(|| super::render(citadel, &addons, &hero, &[code.clone()], Some(&f))) {
                    Ok(Ok(g)) => println!("ALL {f} {code} OK {} KB {:?}", g.len() / 1024, t0.elapsed()),
                    Ok(Err(e)) => println!("ALL {f} {code} ERR {e}"),
                    Err(_) => println!("ALL {f} {code} PANIC"),
                }
            }
        }
    }
}
