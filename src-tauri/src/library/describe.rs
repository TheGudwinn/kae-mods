//! Works out what an addon actually contains from the file paths inside its VPK, so the library
//! can say "Haze skin: models and textures" instead of just showing `569650_pak06_dir.vpk`.
//!
//! This is a heuristic on path names only; nothing is executed or decoded. The full path list stays
//! available in the UI so the user can always check the guess.
use crate::install::vpk::VpkFile;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// What kind of content a path is. The string is a stable key the UI translates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Models,
    Textures,
    Sounds,
    Interface,
    Effects,
    Maps,
    Scripts,
    Other,
}

impl Kind {
    pub fn key(self) -> &'static str {
        match self {
            Kind::Models => "models",
            Kind::Textures => "textures",
            Kind::Sounds => "sounds",
            Kind::Interface => "interface",
            Kind::Effects => "effects",
            Kind::Maps => "maps",
            Kind::Scripts => "scripts",
            Kind::Other => "other",
        }
    }
}

fn kind_of(path: &str) -> Kind {
    let p = path.to_ascii_lowercase();
    let top = p.split('/').next().unwrap_or("");
    match top {
        "models" => return Kind::Models,
        "materials" | "textures" => return Kind::Textures,
        "sounds" | "soundevents" => return Kind::Sounds,
        "panorama" => return Kind::Interface,
        "particles" => return Kind::Effects,
        "maps" => return Kind::Maps,
        "scripts" | "cfg" | "gamedata" => return Kind::Scripts,
        _ => {}
    }
    match p.rsplit('.').next().unwrap_or("") {
        "vmdl_c" | "vanim_c" | "vagrp_c" | "vphys_c" => Kind::Models,
        "vmat_c" | "vtex_c" => Kind::Textures,
        "vsnd_c" | "vsndevts_c" => Kind::Sounds,
        "vxml_c" | "vcss_c" | "vjs_c" | "vsvg_c" => Kind::Interface,
        "vpcf_c" => Kind::Effects,
        "vmap_c" | "vwrld_c" => Kind::Maps,
        _ => Kind::Other,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KindCount {
    pub kind: String,
    pub count: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HeroHit {
    /// Internal folder name found in the paths (e.g. `hornet`).
    pub code: String,
    /// Display name (e.g. `Vindicta`); the code itself, prettified, when the hero is not in the table.
    pub name: String,
    pub files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Content {
    pub total_files: usize,
    pub total_bytes: u64,
    /// Most files first.
    pub kinds: Vec<KindCount>,
    /// Heroes the files belong to, most files first (at most 4).
    pub heroes: Vec<HeroHit>,
    /// Heroes whose own model (`models/heroes*/<hero>/*.vmdl_c`), or its materials and textures, the files replace:
    /// the real skins. Voice packs,
    /// particles and card art also name a hero, but do not change how it looks.
    pub model_heroes: Vec<HeroHit>,
    /// Up to `SAMPLE` paths, sorted, so the user can see what is inside.
    pub sample: Vec<String>,
    /// Top-level folders (`models`, `materials`, ...) with file counts.
    pub folders: Vec<(String, usize)>,
}

const SAMPLE: usize = 80;
const MAX_HEROES: usize = 4;

/// Internal folder name -> hero. Only names that are well established are listed; any other hero
/// folder is still reported, under its prettified folder name.
const HEROES: &[(&str, &str)] = &[
    ("inferno", "Infernus"),
    ("gigawatt", "Seven"),
    ("hornet", "Vindicta"),
    ("spectre", "Lady Geist"),
    ("atlas", "Abrams"),
    ("wraith", "Wraith"),
    ("engineer", "McGinnis"),
    ("chrono", "Paradox"),
    ("dynamo", "Dynamo"),
    ("kelvin", "Kelvin"),
    ("haze", "Haze"),
    ("astro", "Holliday"),
    ("bebop", "Bebop"),
    ("nano", "Calico"),
    ("orion", "Grey Talon"),
    ("krill", "Mo & Krill"),
    ("shiv", "Shiv"),
    ("tengu", "Ivy"),
    ("warden", "Warden"),
    ("yamato", "Yamato"),
    ("lash", "Lash"),
    ("viscous", "Viscous"),
    ("synth", "Pocket"),
    ("mirage", "Mirage"),
    ("magician", "Sinclair"),
    ("viper", "Vyper"),
    ("bookworm", "Paige"),
    ("frank", "Victor"),
    ("doorman", "The Doorman"),
    ("punkgoat", "Billy"),
    ("drifter", "Drifter"),
];

/// Folders that sit under `heroes*` but are not a hero.
const NOT_A_HERO: &[&str] = &[
    "base", "common", "shared", "generic", "default", "textures", "materials", "models", "vfx", "fx", "anims", "animations", "weapons",
    "ui", "sounds", "particles", "props", "wip", "staging", "test", "tests", "old", "new",
];

fn hero_name(code: &str) -> String {
    if let Some((_, n)) = HEROES.iter().find(|(c, _)| *c == code) {
        return (*n).to_string();
    }
    let pretty = code.trim_start_matches("hero_").replace('_', " ");
    let mut cs = pretty.chars();
    cs.next().map(|f| f.to_uppercase().collect::<String>() + cs.as_str()).unwrap_or(pretty)
}

/// The hero a path belongs to, and whether that is a strong signal (`.../heroes*/<hero>/...`).
fn hero_of(path: &str) -> Option<(String, bool)> {
    let p = path.to_ascii_lowercase();
    let segs: Vec<&str> = p.split('/').collect();
    let dirs = &segs[..segs.len().saturating_sub(1)];
    // Strong: the folder right after a `heroes`-like folder.
    for w in dirs.windows(2) {
        if w[0].starts_with("heroes") && !w[1].is_empty() && !NOT_A_HERO.contains(&w[1]) {
            return Some((w[1].trim_start_matches("hero_").to_string(), true));
        }
    }
    // Weak: any folder named exactly like a known hero (sounds/vo/<hero>, panorama/.../<hero>.png folders).
    dirs.iter().find_map(|d| {
        let d = d.trim_start_matches("hero_");
        HEROES.iter().any(|(c, _)| *c == d).then(|| (d.to_string(), false))
    })
}

pub fn describe(files: &[VpkFile]) -> Content {
    let mut by_kind: BTreeMap<Kind, (usize, u64)> = BTreeMap::new();
    let mut strong: HashMap<String, usize> = HashMap::new();
    let mut weak: HashMap<String, usize> = HashMap::new();
    let mut modeled: HashMap<String, usize> = HashMap::new();
    let mut folders: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_bytes = 0u64;

    for f in files {
        let e = by_kind.entry(kind_of(&f.path)).or_default();
        e.0 += 1;
        e.1 += f.size;
        total_bytes += f.size;
        let top = f.path.split('/').next().filter(|_| f.path.contains('/')).unwrap_or("(root)");
        *folders.entry(top.to_ascii_lowercase()).or_default() += 1;
        match hero_of(&f.path) {
            Some((h, true)) => {
                // a model, or the materials and textures of the hero's model (a texture-only skin has no model)
                let lower = f.path.to_ascii_lowercase();
                let hero_folder = lower.starts_with("models/heroes") || lower.starts_with("materials/models/heroes");
                if hero_folder && [".vmdl_c", ".vmat_c", ".vtex_c"].iter().any(|ext| lower.ends_with(ext)) {
                    *modeled.entry(h.clone()).or_default() += 1;
                }
                *strong.entry(h).or_default() += 1
            }
            Some((h, false)) => *weak.entry(h).or_default() += 1,
            None => {}
        }
    }

    // Folder-based hits are trusted first; name-only hits are used only when there are none.
    let source = if strong.is_empty() { weak } else { strong };
    let mut heroes: Vec<HeroHit> = source.into_iter().map(|(code, files)| HeroHit { name: hero_name(&code), code, files }).collect();
    heroes.sort_by(|a, b| b.files.cmp(&a.files).then_with(|| a.code.cmp(&b.code)));
    heroes.truncate(MAX_HEROES);
    let mut model_heroes: Vec<HeroHit> = modeled.into_iter().map(|(code, files)| HeroHit { name: hero_name(&code), code, files }).collect();
    model_heroes.sort_by(|a, b| b.files.cmp(&a.files).then_with(|| a.code.cmp(&b.code)));
    model_heroes.truncate(MAX_HEROES);

    let mut kinds: Vec<KindCount> =
        by_kind.into_iter().map(|(k, (count, bytes))| KindCount { kind: k.key().to_string(), count, bytes }).collect();
    kinds.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.kind.cmp(&b.kind)));

    let mut folders: Vec<(String, usize)> = folders.into_iter().collect();
    folders.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    folders.truncate(8);

    let mut sample: BTreeSet<&str> = BTreeSet::new();
    for f in files {
        sample.insert(&f.path);
    }
    let sample = sample.into_iter().take(SAMPLE).map(str::to_string).collect();

    Content { total_files: files.len(), total_bytes, kinds, heroes, model_heroes, sample, folders }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(path: &str, size: u64) -> VpkFile {
        VpkFile { path: path.into(), crc: 0, size }
    }

    #[test]
    fn classifies_by_folder_and_by_extension() {
        assert_eq!(kind_of("models/heroes_wip/haze/haze.vmdl_c"), Kind::Models);
        assert_eq!(kind_of("materials/models/x.vmat_c"), Kind::Textures);
        assert_eq!(kind_of("sounds/vo/a.vsnd_c"), Kind::Sounds);
        assert_eq!(kind_of("panorama/layout/hud.vxml_c"), Kind::Interface);
        assert_eq!(kind_of("particles/a.vpcf_c"), Kind::Effects);
        assert_eq!(kind_of("maps/dl_midtown.vpk"), Kind::Maps);
        assert_eq!(kind_of("weird/a.vtex_c"), Kind::Textures, "unknown folder falls back to the extension");
        assert_eq!(kind_of("readme.txt"), Kind::Other);
    }

    #[test]
    fn a_texture_only_skin_counts_as_a_skin() {
        let c = describe(&[f("models/heroes_wip/dynamo/materials/dynamo_clothes_color_png_8f9257e5.vtex_c", 16_000_000)]);
        assert_eq!(c.model_heroes.iter().map(|h| h.code.as_str()).collect::<Vec<_>>(), ["dynamo"]);
        // voice and card art still do not
        let v = describe(&[f("sounds/vo/dynamo/a.vsnd_c", 10), f("panorama/images/heroes/dynamo_card.vtex_c", 10)]);
        assert!(v.model_heroes.is_empty());
    }

    #[test]
    fn finds_the_hero_a_skin_belongs_to() {
        let c = describe(&[
            f("models/heroes_wip/hornet/hornet.vmdl_c", 100),
            f("models/heroes_wip/hornet/hornet_arms.vmdl_c", 100),
            f("materials/models/heroes_wip/hornet/body.vmat_c", 10),
            f("materials/models/heroes_wip/hornet/body_color.vtex_c", 1000),
        ]);
        assert_eq!(c.heroes.len(), 1);
        assert_eq!((c.heroes[0].code.as_str(), c.heroes[0].name.as_str(), c.heroes[0].files), ("hornet", "Vindicta", 4));
        assert_eq!(c.total_files, 4);
        assert_eq!(c.total_bytes, 1210);
        let models = c.kinds.iter().find(|k| k.kind == "models").unwrap();
        assert_eq!((models.count, models.bytes), (2, 200));
    }

    #[test]
    fn strong_hits_beat_name_only_hits_and_generic_folders_are_ignored() {
        let c = describe(&[
            f("models/heroes_staging/haze/a.vmdl_c", 1),
            f("sounds/vo/shiv/line.vsnd_c", 1),
            f("sounds/vo/shiv/line2.vsnd_c", 1),
            f("models/heroes_wip/common/b.vmdl_c", 1),
        ]);
        assert_eq!(c.heroes.iter().map(|h| h.code.as_str()).collect::<Vec<_>>(), ["haze"]);
        // Without any heroes folder the name-only hits are used.
        let c = describe(&[f("sounds/vo/shiv/line.vsnd_c", 1)]);
        assert_eq!(c.heroes[0].name, "Shiv");
    }

    #[test]
    fn unknown_hero_folders_are_still_reported_prettily() {
        let c = describe(&[f("models/heroes_wip/hero_new_guy/a.vmdl_c", 1)]);
        assert_eq!((c.heroes[0].code.as_str(), c.heroes[0].name.as_str()), ("new_guy", "New guy"));
    }

    #[test]
    fn hud_mods_have_no_hero_and_report_folders() {
        let c = describe(&[f("panorama/layout/hud/a.vxml_c", 1), f("panorama/styles/a.vcss_c", 1), f("loose.txt", 1)]);
        assert!(c.heroes.is_empty());
        assert_eq!(c.kinds[0].kind, "interface");
        assert_eq!(c.folders[0], ("panorama".to_string(), 2));
        assert!(c.folders.contains(&("(root)".to_string(), 1)));
    }

    #[test]
    fn sample_is_sorted_deduped_and_bounded() {
        let files: Vec<VpkFile> = (0..200).map(|i| f(&format!("models/m{i:03}.vmdl_c"), 1)).collect();
        let c = describe(&files);
        assert_eq!(c.sample.len(), SAMPLE);
        assert_eq!(c.sample[0], "models/m000.vmdl_c");
        assert_eq!(c.total_files, 200);
        assert!(describe(&[]).kinds.is_empty());
    }

    #[test]
    fn heroes_are_limited_and_ordered_by_file_count() {
        let mut files = Vec::new();
        for (i, h) in ["haze", "shiv", "lash", "kelvin", "bebop", "dynamo"].iter().enumerate() {
            for j in 0..=i {
                files.push(f(&format!("models/heroes_wip/{h}/{j}.vmdl_c"), 1));
            }
        }
        let c = describe(&files);
        assert_eq!(c.heroes.len(), MAX_HEROES);
        assert_eq!(c.heroes[0].code, "dynamo");
    }
}
