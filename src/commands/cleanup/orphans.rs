//! Orphaned backups and stale locks: status line, preview and the confirmed removal.

use anyhow::Result;
use humansize::{format_size, BINARY};
use std::path::Path;

use agr::storage::orphans::{self, Orphans};

use crate::commands::compress::print_lines;

/// "2 orphaned backup(s) and 1 stale lock(s), 3 KiB, remove with agr cleanup", none when clean.
pub fn status_line(found: &Orphans) -> Option<String> {
    if found.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if !found.backups.is_empty() {
        parts.push(format!("{} orphaned backup(s)", found.backups.len()));
    }
    if !found.locks.is_empty() {
        parts.push(format!("{} stale lock(s)", found.locks.len()));
    }
    let size = format_size(found.bytes, BINARY);
    Some(format!(
        "{}, {size}, remove with agr cleanup",
        parts.join(" and ")
    ))
}

/// One line per file that would be removed.
pub fn preview_lines(found: &Orphans) -> Vec<String> {
    let mut lines = vec![status_line(found)
        .unwrap_or_default()
        .replace(", remove with agr cleanup", "")];
    lines.extend(
        found
            .backups
            .iter()
            .chain(&found.locks)
            .map(|p| format!("  {}", p.display())),
    );
    lines
}

/// Show the preview, ask once, remove on yes. `yes` skips the question.
pub fn offer_removal(found: &Orphans, ask: impl FnOnce(&str) -> Result<bool>) -> Result<()> {
    if found.is_empty() {
        return Ok(());
    }
    print_lines(&preview_lines(found));
    if ask("Remove these files?")? {
        let n = orphans::remove(found);
        print_lines(&[format!("Removed {n} file(s).")]);
    } else {
        print_lines(&["No changes made.".to_string()]);
    }
    Ok(())
}

/// Find and offer removal for a storage directory.
#[cfg(not(tarpaulin_include))]
pub fn handle(storage_dir: &Path, yes: bool) -> Result<()> {
    let found = orphans::find(storage_dir)?;
    offer_removal(&found, |q| crate::commands::prompt::should_proceed(q, yes))
}

#[cfg(test)]
mod tests {
    use super::{offer_removal, preview_lines, status_line};
    use agr::storage::orphans::{self, Orphans};
    use std::fs;
    use std::path::Path;

    fn found_in(dir: &Path) -> Orphans {
        fs::create_dir_all(dir.join("a")).unwrap();
        fs::write(dir.join("a/.x.cast.bak"), "12345").unwrap();
        fs::write(dir.join("a/.y.cast.lock"), "junk").unwrap();
        orphans::find(dir).unwrap()
    }

    #[test]
    fn status_line_names_backups_locks_and_size() {
        let t = tempfile::tempdir().unwrap();
        let found = found_in(t.path());
        assert_eq!(
            status_line(&found).as_deref(),
            Some("1 orphaned backup(s) and 1 stale lock(s), 9 B, remove with agr cleanup")
        );
        assert_eq!(status_line(&Orphans::default()), None);
    }

    #[test]
    fn preview_lists_every_file() {
        let t = tempfile::tempdir().unwrap();
        let lines = preview_lines(&found_in(t.path()));
        assert_eq!(lines.len(), 3);
        assert!(lines[1].ends_with(".x.cast.bak"), "{lines:?}");
        assert!(!lines[0].contains("remove with"), "{lines:?}");
    }

    #[test]
    fn declined_removal_keeps_files_and_accepted_removes_them() {
        let t = tempfile::tempdir().unwrap();
        let found = found_in(t.path());
        offer_removal(&found, |_| Ok(false)).unwrap();
        assert_eq!(orphans::find(t.path()).unwrap(), found);
        offer_removal(&found, |_| Ok(true)).unwrap();
        assert!(orphans::find(t.path()).unwrap().is_empty());
        offer_removal(&Orphans::default(), |_| panic!("nothing to ask")).unwrap();
    }
}
