//! Minimal, surgical edit of `gameinfo.gi`: make sure `Game citadel/addons` is listed in
//! SearchPaths before the `Game citadel` line. Nothing else in the file is touched, so edits made
//! by other managers (deadworks, Deadlock Mod Manager) are preserved.
//!
//! The file is KeyValues text. It is tokenized properly (line and block comments, quoted strings,
//! braces anywhere on a line) instead of being scanned line by line, and the edit is refused
//! (`Unrecognized`) whenever the layout is not exactly what we can patch with certainty.
use std::io::Write;
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum PatchResult {
    AlreadyPresent,
    Patched(String),
    /// SearchPaths block or the `Game citadel` anchor was not found; do not edit.
    Unrecognized,
}

const ADDONS: &str = "citadel/addons";

#[derive(Debug, PartialEq, Eq)]
enum Tok {
    Open,
    Close,
    Word(String),
}

struct Token {
    tok: Tok,
    line: usize,
}

/// Split KeyValues text into tokens. `None` = malformed (unterminated comment or string).
fn lex(text: &str) -> Option<Vec<Token>> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let (mut i, mut line) = (0usize, 0usize);
    while i < b.len() {
        match b[i] {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b' ' | b'\t' | b'\r' => i += 1,
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                loop {
                    if i + 1 >= b.len() {
                        return None; // unterminated block comment
                    }
                    if b[i] == b'*' && b[i + 1] == b'/' {
                        i += 2;
                        break;
                    }
                    if b[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
            b'{' => {
                out.push(Token { tok: Tok::Open, line });
                i += 1;
            }
            b'}' => {
                out.push(Token { tok: Tok::Close, line });
                i += 1;
            }
            b'"' => {
                i += 1;
                let start = i;
                while i < b.len() && b[i] != b'"' && b[i] != b'\n' {
                    i += 1;
                }
                if i >= b.len() || b[i] != b'"' {
                    return None; // unterminated string
                }
                out.push(Token { tok: Tok::Word(text[start..i].to_string()), line });
                i += 1;
            }
            _ => {
                let start = i;
                while i < b.len()
                    && !matches!(b[i], b' ' | b'\t' | b'\r' | b'\n' | b'{' | b'}' | b'"')
                    && !(b[i] == b'/' && matches!(b.get(i + 1), Some(b'/') | Some(b'*')))
                {
                    i += 1;
                }
                out.push(Token { tok: Tok::Word(text[start..i].to_string()), line });
            }
        }
    }
    Some(out)
}

fn same_path(value: &str, want: &str) -> bool {
    let v = value.replace('\\', "/");
    v.trim_end_matches('/').eq_ignore_ascii_case(want)
}

/// What one SearchPaths block contains.
struct Block {
    has_addons: bool,
    /// Line index of the first clean `Game citadel` line (exactly those two words, nothing else).
    anchor: Option<usize>,
    /// (line, path) of every `Game <path>` line.
    games: Vec<(usize, String)>,
    /// The block has `Mod` or `Write` lines, which tell the engine which folder is the game's own.
    pinned: bool,
}

/// Parse every `SearchPaths { ... }` block. `None` = the block structure is malformed.
fn parse_blocks(toks: &[Token]) -> Option<Vec<Block>> {
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let is_sp = matches!(&toks[i].tok, Tok::Word(w) if w.eq_ignore_ascii_case("SearchPaths"));
        if !(is_sp && toks.get(i + 1).is_some_and(|t| t.tok == Tok::Open)) {
            i += 1;
            continue;
        }
        let mut depth = 1;
        let mut j = i + 2;
        // line -> (words at depth 1, whether any brace sits on that line)
        let mut lines: Vec<(usize, Vec<&str>, bool)> = Vec::new();
        while j < toks.len() && depth > 0 {
            let t = &toks[j];
            let entry = match lines.last_mut() {
                Some(l) if l.0 == t.line => l,
                _ => {
                    lines.push((t.line, Vec::new(), false));
                    lines.last_mut().unwrap()
                }
            };
            match &t.tok {
                Tok::Open => {
                    depth += 1;
                    entry.2 = true;
                }
                Tok::Close => {
                    depth -= 1;
                    entry.2 = true;
                }
                Tok::Word(w) if depth == 1 => entry.1.push(w),
                Tok::Word(_) => {}
            }
            j += 1;
        }
        if depth != 0 {
            return None; // never closed
        }
        let mut block = Block { has_addons: false, anchor: None, games: Vec::new(), pinned: false };
        for (line, words, braces) in &lines {
            if words.len() >= 2 && (words[0].eq_ignore_ascii_case("Mod") || words[0].eq_ignore_ascii_case("Write")) {
                block.pinned = true;
            }
            if words.len() >= 2 && words[0].eq_ignore_ascii_case("Game") {
                block.games.push((*line, words[1].to_string()));
                if same_path(words[1], ADDONS) {
                    block.has_addons = true;
                }
                if block.anchor.is_none() && words.len() == 2 && !braces && words[1].eq_ignore_ascii_case("citadel") {
                    block.anchor = Some(*line);
                }
            }
        }
        blocks.push(block);
        i = j;
    }
    Some(blocks)
}

pub fn ensure_addons_path(text: &str) -> PatchResult {
    let Some(toks) = lex(text) else { return PatchResult::Unrecognized };
    let Some(blocks) = parse_blocks(&toks) else { return PatchResult::Unrecognized };
    if blocks.is_empty() {
        return PatchResult::Unrecognized;
    }
    if blocks.iter().any(|b| b.has_addons) {
        return PatchResult::AlreadyPresent;
    }
    let Some(anchor) = blocks.iter().find_map(|b| b.anchor) else { return PatchResult::Unrecognized };

    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let Some(anchor_line) = lines.get(anchor) else { return PatchResult::Unrecognized };

    let indent: String = anchor_line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
    let eol = if anchor_line.ends_with("\r\n") {
        "\r\n"
    } else if anchor_line.ends_with('\n') || !text.contains("\r\n") {
        "\n"
    } else {
        "\r\n" // last line without a terminator: follow the rest of the file
    };
    let new_line = format!("{indent}Game                {ADDONS}{eol}");

    let mut out = String::with_capacity(text.len() + new_line.len());
    for (i, l) in lines.iter().enumerate() {
        if i == anchor {
            out.push_str(&new_line);
        }
        out.push_str(l);
    }
    with_pin(&out, PatchResult::Patched(out.clone()))
}

/// The engine takes the first `Game` folder as the game's own (where `cfg` is read and written, where binaries and
/// shaders are looked for) unless `Mod` and `Write` lines say otherwise. A folder listed before `Game citadel`
/// (addons, the compatibility package) would take that role and the game stops with "Unable to read default
/// keybinding configuration". Deadlock Mod Manager pins both to citadel for that reason, and so does this.
/// `None` = nothing to do (already pinned, nothing listed before `Game citadel`, or the layout is not clear).
fn pin_mod_paths(text: &str) -> Option<String> {
    let toks = lex(text)?;
    let blocks = parse_blocks(&toks)?;
    let (block, anchor) = blocks.iter().find_map(|b| b.anchor.map(|a| (b, a)))?;
    if block.pinned || !block.games.iter().any(|(l, _)| *l < anchor) {
        return None;
    }
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let anchor_line = lines.get(anchor)?;
    let indent: String = anchor_line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
    let eol = if anchor_line.ends_with("\r\n") {
        "\r\n"
    } else if anchor_line.ends_with('\n') || !text.contains("\r\n") {
        "\n"
    } else {
        "\r\n"
    };
    let mut out = String::with_capacity(text.len() + 96);
    for (i, l) in lines.iter().enumerate() {
        if i == anchor {
            out.push_str(&format!("{indent}Mod                 citadel{eol}{indent}Write               citadel{eol}"));
        }
        out.push_str(l);
    }
    Some(out)
}

/// Whether `Mod`/`Write` lines pin the game's own folder to citadel where an earlier folder would take that role.
/// `Patched` carries the text with the lines added (a file edited by an older version of this app lacks them).
pub fn ensure_pinned(text: &str) -> PatchResult {
    if lex(text).and_then(|t| parse_blocks(&t)).is_none() {
        return PatchResult::Unrecognized;
    }
    pin_mod_paths(text).map_or(PatchResult::AlreadyPresent, PatchResult::Patched)
}

/// `result`, with the `Mod`/`Write` lines added to whatever text it carries (or to `text` when nothing was patched).
fn with_pin(text: &str, result: PatchResult) -> PatchResult {
    match result {
        PatchResult::Patched(t) => PatchResult::Patched(pin_mod_paths(&t).unwrap_or(t)),
        PatchResult::AlreadyPresent => pin_mod_paths(text).map_or(PatchResult::AlreadyPresent, PatchResult::Patched),
        other => other,
    }
}

/// Make sure `Game <rel>` is listed in SearchPaths right before the first `citadel/addons` line, so files in
/// `<rel>` win over every addon. An earlier search path has priority in the engine.
/// A `<rel>` line that sits after an addons line (another manager wrote its own addons line above ours) does
/// nothing: it is moved up in front of the addons line.
pub fn ensure_path_before_addons(text: &str, rel: &str) -> PatchResult {
    let Some(toks) = lex(text) else { return PatchResult::Unrecognized };
    let Some(blocks) = parse_blocks(&toks) else { return PatchResult::Unrecognized };
    let at = blocks.iter().filter_map(|b| b.games.iter().filter(|(_, p)| same_path(p, ADDONS)).map(|(l, _)| *l).min()).min();
    let rel_lines: Vec<usize> = blocks.iter().flat_map(|b| b.games.iter().filter(|(_, p)| same_path(p, rel)).map(|(l, _)| *l)).collect();
    let Some(at) = at else {
        return if rel_lines.is_empty() { PatchResult::Unrecognized } else { with_pin(text, PatchResult::AlreadyPresent) };
    };
    if rel_lines.iter().any(|l| *l < at) {
        return with_pin(text, PatchResult::AlreadyPresent);
    }
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let Some(line) = lines.get(at) else { return PatchResult::Unrecognized };
    let indent: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
    let crlf = line.ends_with("\r\n") || (!line.ends_with('\n') && text.contains("\r\n"));
    let eol = if crlf { "\r\n" } else { "\n" };
    let new_line = format!("{indent}Game                {rel} // guddion-compat{eol}");
    let mut out = String::with_capacity(text.len() + new_line.len());
    for (i, l) in lines.iter().enumerate() {
        if i == at {
            out.push_str(&new_line);
        }
        if rel_lines.contains(&i) {
            continue; // the misplaced line is replaced by the one above
        }
        out.push_str(l);
    }
    with_pin(&out, PatchResult::Patched(out.clone()))
}

/// `ensure_path_before_addons`, and when the file has no `citadel/addons` line at all (a game update
/// restores the stock file) the addons line is put back first, so mods load again as well.
pub fn ensure_compat_path(text: &str, rel: &str) -> PatchResult {
    match ensure_path_before_addons(text, rel) {
        PatchResult::Unrecognized => match ensure_addons_path(text) {
            PatchResult::Patched(with_addons) => match ensure_path_before_addons(&with_addons, rel) {
                PatchResult::Patched(t) => PatchResult::Patched(t),
                PatchResult::AlreadyPresent => PatchResult::Patched(with_addons),
                PatchResult::Unrecognized => PatchResult::Unrecognized,
            },
            _ => PatchResult::Unrecognized,
        },
        other => other,
    }
}

/// Replace `path` with `data` so that a crash can never leave a half-written file: write a sibling
/// temp file, flush it to disk, then rename it over the original.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".ddmm-tmp");
    let tmp = std::path::PathBuf::from(tmp);
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        drop(f);
        // Keep the original's permissions (e.g. a read-only flag would be a user choice to respect).
        if let Ok(meta) = std::fs::metadata(path) {
            let _ = std::fs::set_permissions(&tmp, meta.permissions());
        }
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "GameInfo\r\n{\r\n\tSearchPaths\r\n\t{\r\n\t\tGame_LowViolence    citadel_lv\r\n\t\tMod                 citadel\r\n\t\tWrite               citadel\r\n\t\tGame                citadel\r\n\t\tGame                core\r\n\t}\r\n}\r\n";

    #[test]
    fn compat_path_also_restores_a_missing_addons_line() {
        let PatchResult::Patched(s) = ensure_compat_path(BASE, "citadel/guddion_compat") else { panic!() };
        let lines: Vec<&str> = s.lines().map(str::trim).collect();
        let c = lines.iter().position(|l| l.contains("guddion_compat")).unwrap();
        assert!(lines[c + 1].contains("citadel/addons"), "{lines:?}");
        assert!(matches!(ensure_compat_path(&s, "citadel/guddion_compat"), PatchResult::AlreadyPresent));
    }

    #[test]
    fn a_compat_line_below_another_managers_addons_line_moves_above_it() {
        let t = "GameInfo
{
	SearchPaths
	{
		Game		citadel/addons // dmm
		Mod		\"citadel\"
		Write		\"citadel\"
		Game citadel/guddion_compat // guddion-compat
		Game citadel/addons
		Game citadel
	}
}
";
        let PatchResult::Patched(s) = ensure_path_before_addons(t, "citadel/guddion_compat") else { panic!("{}", "not patched") };
        let lines: Vec<&str> = s.lines().map(str::trim).collect();
        let c = lines.iter().position(|l| l.contains("guddion_compat")).unwrap();
        let a = lines.iter().position(|l| l.contains("citadel/addons")).unwrap();
        assert!(c < a && lines.iter().filter(|l| l.contains("guddion_compat")).count() == 1, "{lines:?}");
        assert!(matches!(ensure_path_before_addons(&s, "citadel/guddion_compat"), PatchResult::AlreadyPresent));
    }

    const STOCK: &str = "GameInfo\n{\n\tSearchPaths\n\t{\n\t\tGame_UILanguage\t\tcitadel_*LANGUAGE*\n\t\tGame_LowViolence\tcitadel_lv\n\n\t\tGame\t\t\t\tcitadel\n\t\tGame\t\t\t\tcore\n\t}\n}\n";

    fn order(s: &str) -> Vec<String> {
        s.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|l| l.starts_with("Game ") || l.starts_with("Mod ") || l.starts_with("Write ")).collect()
    }

    #[test]
    fn folders_before_citadel_do_not_become_the_games_own_folder() {
        // the stock file has no Mod/Write lines: the first Game folder would become the mod folder
        let PatchResult::Patched(s) = ensure_compat_path(STOCK, "citadel/guddion_compat") else { panic!() };
        assert_eq!(order(&s), ["Game citadel/guddion_compat // guddion-compat", "Game citadel/addons", "Mod citadel", "Write citadel", "Game citadel", "Game core"]);
        assert!(matches!(ensure_compat_path(&s, "citadel/guddion_compat"), PatchResult::AlreadyPresent));
        assert!(matches!(ensure_addons_path(&s), PatchResult::AlreadyPresent));
        // addons alone, the way installing a mod does it
        let PatchResult::Patched(a) = ensure_addons_path(STOCK) else { panic!() };
        assert_eq!(order(&a), ["Game citadel/addons", "Mod citadel", "Write citadel", "Game citadel", "Game core"]);
    }

    #[test]
    fn an_earlier_edit_without_pins_is_repaired() {
        let broken = STOCK.replace("\t\tGame\t\t\t\tcitadel\n", "\t\tGame citadel/guddion_compat\n\t\tGame citadel/addons\n\t\tGame\t\t\t\tcitadel\n");
        let PatchResult::Patched(fixed) = ensure_compat_path(&broken, "citadel/guddion_compat") else { panic!("not repaired") };
        assert_eq!(order(&fixed), ["Game citadel/guddion_compat", "Game citadel/addons", "Mod citadel", "Write citadel", "Game citadel", "Game core"]);
    }

    #[test]
    fn files_that_pin_their_own_folders_are_left_alone() {
        // Deadlock Mod Manager's layout already has Mod and Write lines
        let PatchResult::Patched(s) = ensure_compat_path(BASE, "citadel/guddion_compat") else { panic!() };
        assert_eq!(s.matches("Mod ").count(), 1);
        assert_eq!(s.matches("Write ").count(), 1);
        // nothing listed before `Game citadel`: nothing to pin
        assert_eq!(pin_mod_paths(STOCK), None);
    }

    #[test]
    fn inserts_before_game_citadel_keeping_crlf() {
        let PatchResult::Patched(s) = ensure_addons_path(BASE) else { panic!() };
        let lines: Vec<&str> = s.lines().collect();
        let i = lines.iter().position(|l| l.contains("citadel/addons")).unwrap();
        assert!(lines[i + 1].trim_start().starts_with("Game") && lines[i + 1].trim_end().ends_with("citadel"));
        assert!(s.contains("citadel/addons\r\n"));
        assert_eq!(s.lines().count(), BASE.lines().count() + 1);
    }

    #[test]
    fn compat_path_goes_right_before_addons_and_only_once() {
        let text = "GameInfo\r\n{\r\n\tSearchPaths\r\n\t{\r\n\t\t// DMM\r\n\t\tGame\t\tcitadel/dmm_localization_default // dmm-search-path\r\n\t\tGame\t\tcitadel/addons // dmm-search-path\r\n\t\tGame                citadel\r\n\t}\r\n}\r\n";
        let PatchResult::Patched(new) = ensure_path_before_addons(text, "citadel/guddion_compat") else { panic!() };
        let a = new.find("guddion_compat").unwrap();
        let b = new.find("citadel/addons").unwrap();
        assert!(new.find("dmm_localization_default").unwrap() < a && a < b);
        assert!(new.contains("// guddion-compat\r\n"));
        assert_eq!(ensure_path_before_addons(&new, "citadel/guddion_compat"), PatchResult::AlreadyPresent);
        let added = "\t\tGame                citadel/guddion_compat // guddion-compat\r\n";
        let pins = "\t\tMod                 citadel\r\n\t\tWrite               citadel\r\n";
        // besides the new folder only the Mod and Write lines were added, right before `Game citadel`
        assert_eq!(new.replace(added, "").replace(pins, ""), text);
        assert!(new.contains(&format!("{pins}\t\tGame                citadel\r\n")));
        assert_eq!(ensure_path_before_addons("GameInfo\n{\n}\n", "x"), PatchResult::Unrecognized);
    }

    #[test]
    fn idempotent() {
        let PatchResult::Patched(s) = ensure_addons_path(BASE) else { panic!() };
        assert_eq!(ensure_addons_path(&s), PatchResult::AlreadyPresent);
    }

    #[test]
    fn respects_foreign_manager_edits() {
        // Shape taken from a real machine patched by other managers.
        let t = "SearchPaths\n{\n Game citadel/deadworks_mods\n addonroot citadel/deadworks_addons\n Game citadel/addons\n Mod citadel\n Game citadel\n}\n";
        assert_eq!(ensure_addons_path(t), PatchResult::AlreadyPresent);
    }

    #[test]
    fn refuses_unknown_layout() {
        assert_eq!(ensure_addons_path("nothing here"), PatchResult::Unrecognized);
        assert_eq!(ensure_addons_path("SearchPaths\n{\n Mod core\n}\n"), PatchResult::Unrecognized);
    }

    #[test]
    fn quoted_values_are_understood() {
        let present = "SearchPaths\n{\n Game \"citadel/addons\"\n Game \"citadel\"\n}\n";
        assert_eq!(ensure_addons_path(present), PatchResult::AlreadyPresent);
        let missing = "SearchPaths\n{\n\t\"Game\"\t\"citadel\"\n}\n";
        let PatchResult::Patched(s) = ensure_addons_path(missing) else { panic!() };
        assert!(s.starts_with("SearchPaths\n{\n\tGame                citadel/addons\n\tMod                 citadel\n\tWrite               citadel\n\t\"Game\"\t\"citadel\"\n"));
    }

    #[test]
    fn backslash_and_trailing_slash_count_as_present() {
        assert_eq!(ensure_addons_path("SearchPaths\n{\n Game citadel\\addons\\\n Game citadel\n}\n"), PatchResult::AlreadyPresent);
    }

    #[test]
    fn commented_out_entries_are_ignored() {
        let t = "SearchPaths\n{\n // Game citadel/addons\n /* Game citadel/addons\n Game citadel/addons */\n Game citadel\n}\n";
        let PatchResult::Patched(s) = ensure_addons_path(t) else { panic!("should patch") };
        // Three mentions sit inside comments; exactly one real line was added.
        assert_eq!(s.matches("citadel/addons").count(), 4);
        assert!(s.contains(" Game                citadel/addons\n Mod                 citadel\n Write               citadel\n Game citadel\n}"));
        assert_eq!(ensure_addons_path(&s), PatchResult::AlreadyPresent);
    }

    #[test]
    fn braces_inside_block_comments_do_not_confuse_depth() {
        let t = "SearchPaths\n{\n /* } */\n Game citadel\n}\n";
        assert!(matches!(ensure_addons_path(t), PatchResult::Patched(_)));
    }

    #[test]
    fn braces_on_the_same_line_as_entries() {
        let t = "SearchPaths { Game citadel/addons Game citadel }\n";
        assert_eq!(ensure_addons_path(t), PatchResult::AlreadyPresent);
        // An anchor sharing its line with other tokens cannot be patched line-wise: refuse.
        assert_eq!(ensure_addons_path("SearchPaths { Game citadel }\n"), PatchResult::Unrecognized);
    }

    #[test]
    fn multiple_blocks_and_nested_blocks() {
        let t = "Other\n{\n SearchPaths\n {\n Mod core\n }\n}\nSearchPaths\n{\n Sub\n {\n Game citadel\n }\n Game citadel\n}\n";
        let PatchResult::Patched(s) = ensure_addons_path(t) else { panic!() };
        // Inserted next to the depth-1 anchor of the second block, not the nested one.
        assert!(s.contains(" Game                citadel/addons\n Mod                 citadel\n Write               citadel\n Game citadel\n}\n"));
        assert_eq!(s.matches("citadel/addons").count(), 1);
    }

    #[test]
    fn addons_in_any_block_counts() {
        let t = "SearchPaths\n{\n Game citadel\n}\nSearchPaths\n{\n Game citadel/addons\n}\n";
        assert_eq!(ensure_addons_path(t), PatchResult::AlreadyPresent);
    }

    #[test]
    fn malformed_input_is_refused() {
        assert_eq!(ensure_addons_path("SearchPaths\n{\n Game citadel\n"), PatchResult::Unrecognized); // never closed
        assert_eq!(ensure_addons_path("SearchPaths\n{\n /* Game citadel\n}\n"), PatchResult::Unrecognized); // open comment
        assert_eq!(ensure_addons_path("SearchPaths\n{\n Game \"citadel\n}\n"), PatchResult::Unrecognized); // open string
    }

    #[test]
    fn mixed_line_endings_and_missing_final_newline() {
        // The inserted line copies the line ending of the anchor line it precedes.
        let crlf_anchor = "SearchPaths\n{\n Mod core\n Game citadel\r\n}";
        let PatchResult::Patched(s) = ensure_addons_path(crlf_anchor) else { panic!() };
        assert!(s.contains(" Game                citadel/addons\r\n Game citadel\r\n}"));
        let lf_anchor = "SearchPaths\r\n{\r\n Game citadel\n}\r\n";
        let PatchResult::Patched(s) = ensure_addons_path(lf_anchor) else { panic!() };
        assert!(s.contains(" Game                citadel/addons\n Mod                 citadel\n Write               citadel\n Game citadel\n}"));
    }

    #[test]
    fn non_ascii_text_is_preserved() {
        let t = "// Заметка\nSearchPaths\n{\n Game citadel\n}\n";
        let PatchResult::Patched(s) = ensure_addons_path(t) else { panic!() };
        assert!(s.starts_with("// Заметка\n"));
    }

    #[test]
    fn write_atomic_replaces_and_cleans_up() {
        let dir = std::env::temp_dir().join(format!("ddmm-gi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("gameinfo.gi");
        std::fs::write(&p, "old").unwrap();
        write_atomic(&p, b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "new");
        assert!(!dir.join("gameinfo.gi.ddmm-tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
