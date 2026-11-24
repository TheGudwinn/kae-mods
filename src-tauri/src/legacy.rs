//! One-time move of the data folders of the earlier builds (`ru.guddion.deadlockmm`) to `KaeMods`.
//!
//! It runs before anything opens a file in the app folder, so nothing is locked yet. The window's own
//! data (`Local\<id>\EBWebView`: settings kept by the page, wallpaper, preferences) moves with it.
//! Nothing is deleted: an item that already exists at the new place stays where it is in the old folder.
use std::path::{Path, PathBuf};

pub const OLD_ID: &str = "ru.guddion.deadlockmm";
pub const NEW_ID: &str = "KaeMods";

/// Moves what is in `old` into `new`. Returns whether the old folder is gone afterwards.
fn merge_move(old: &Path, new: &Path) -> bool {
    if !old.is_dir() {
        return true;
    }
    if !new.exists() {
        if std::fs::rename(old, new).is_ok() {
            return true;
        }
    }
    let _ = std::fs::create_dir_all(new);
    for entry in std::fs::read_dir(old).into_iter().flatten().flatten() {
        let to = new.join(entry.file_name());
        if to.exists() {
            if entry.path().is_dir() && to.is_dir() {
                merge_move(&entry.path(), &to);
            }
            continue;
        }
        let _ = std::fs::rename(entry.path(), &to);
    }
    std::fs::remove_dir(old).is_ok() // only succeeds when empty
}

/// Backups are listed in the journal by full path; point them at the new folder.
fn fix_journal(new_roaming: &Path, old_roaming: &Path) {
    let file = new_roaming.join("journal.jsonl");
    let Ok(text) = std::fs::read_to_string(&file) else { return };
    // paths inside JSON have their backslashes doubled
    let esc = |p: &Path| p.to_string_lossy().replace('\\', "\\\\");
    let (from, to) = (esc(old_roaming), esc(new_roaming));
    if text.contains(&from) {
        let _ = std::fs::write(&file, text.replace(&from, &to));
    }
}

pub fn migrate() {
    let dir = |var: &str| std::env::var_os(var).map(PathBuf::from);
    let (Some(roaming), Some(local)) = (dir("APPDATA"), dir("LOCALAPPDATA")) else { return };
    let (old_r, new_r) = (roaming.join(OLD_ID), roaming.join(NEW_ID));
    let had_old = old_r.is_dir();
    merge_move(&old_r, &new_r);
    merge_move(&local.join(OLD_ID), &local.join(NEW_ID));
    if had_old {
        fix_journal(&new_r, &old_r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_into_an_empty_target_and_merges_into_a_used_one() {
        let t = tempfile::tempdir().unwrap();
        let (old, new) = (t.path().join("old"), t.path().join("new"));
        std::fs::create_dir_all(old.join("backups")).unwrap();
        std::fs::write(old.join("a.json"), "1").unwrap();
        std::fs::write(old.join("backups").join("b"), "2").unwrap();
        // the target exists with its own folder, as when Minecraft data was already moved there
        std::fs::create_dir_all(new.join("minecraft")).unwrap();
        assert!(merge_move(&old, &new));
        assert_eq!(std::fs::read_to_string(new.join("a.json")).unwrap(), "1");
        assert_eq!(std::fs::read_to_string(new.join("backups").join("b")).unwrap(), "2");
        assert!(new.join("minecraft").is_dir() && !old.exists());
    }

    #[test]
    fn journal_paths_follow() {
        let t = tempfile::tempdir().unwrap();
        let (old, new) = (PathBuf::from(r"C:\Users\x\AppData\Roaming\ru.guddion.deadlockmm"), t.path().to_path_buf());
        let line = r#"{"backup":"C:\\Users\\x\\AppData\\Roaming\\ru.guddion.deadlockmm\\backups\\1"}"#;
        std::fs::write(new.join("journal.jsonl"), line).unwrap();
        fix_journal(&new, &old);
        let out = std::fs::read_to_string(new.join("journal.jsonl")).unwrap();
        assert!(!out.contains("guddion") && out.contains("backups"));
    }
}
