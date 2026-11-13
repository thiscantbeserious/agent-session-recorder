//! Selection and deletion actions for the cleanup app.

use super::glob::{matches_glob_pattern, parse_glob_pattern};
use super::status::format_size;
use anyhow::Result;

use super::CleanupApp;
use crate::files::remove_auxiliary_files;

/// Delete session files with their auxiliary lock/backup files.
///
/// Returns the number of removed sessions and the on-disk bytes freed.
pub fn delete_paths(paths: &[String]) -> (usize, u64) {
    let mut deleted = 0;
    let mut freed = 0;
    for path in paths {
        let size = std::fs::metadata(path).map_or(0, |m| m.len());
        // Always attempt auxiliary cleanup, even if session file removal fails
        remove_auxiliary_files(std::path::Path::new(path));
        if std::fs::remove_file(path).is_ok() {
            deleted += 1;
            freed += size;
        }
    }
    (deleted, freed)
}

impl CleanupApp {
    /// Select items matching a glob-like pattern.
    /// Supports: * (any chars), ? (single char), agent/pattern syntax
    pub(super) fn select_by_glob(&mut self, pattern: &str) -> usize {
        // Parse agent/pattern syntax (e.g., "claude/*.cast" or "*2024*")
        let (agent_filter, file_pattern) = parse_glob_pattern(pattern);

        // Collect matching items that aren't already selected
        let items_to_select: Vec<(usize, String, String, bool, bool)> = self
            .shared
            .explorer
            .visible_items()
            .map(|(vis_idx, item, is_selected)| {
                (
                    vis_idx,
                    item.agent.clone(),
                    item.name.clone(),
                    is_selected,
                    item.lock_info.is_some(),
                )
            })
            .collect();

        // Track original position
        let original_selected = self.shared.explorer.selected();
        let mut actual_count = 0;

        // Select matching items (skip locked sessions)
        for (vis_idx, agent, name, is_selected, is_locked) in items_to_select {
            if is_locked {
                continue;
            }
            if matches_glob_pattern(&agent, &name, agent_filter, file_pattern) && !is_selected {
                // Navigate to this item and select it
                self.shared.explorer.home();
                for _ in 0..vis_idx {
                    self.shared.explorer.down();
                }
                self.shared.explorer.toggle_select();
                actual_count += 1;
            }
        }

        // Restore original position
        self.shared.explorer.home();
        for _ in 0..original_selected.min(self.shared.explorer.len().saturating_sub(1)) {
            self.shared.explorer.down();
        }

        actual_count
    }

    /// Delete all selected sessions.
    pub(super) fn delete_selected(&mut self) -> Result<()> {
        let paths: Vec<String> = self
            .shared
            .explorer
            .selected_items()
            .iter()
            .map(|i| i.path.clone())
            .collect();
        if paths.is_empty() {
            return Ok(());
        }
        let (deleted, freed) = delete_paths(&paths);
        for path in &paths {
            self.shared.explorer.remove_item(path);
        }
        self.shared.status_message = Some(deletion_status(deleted, paths.len(), freed));
        self.files_deleted |= deleted > 0;
        Ok(())
    }
}

/// Status line after a delete run, the freed size is the on-disk size.
pub fn deletion_status(deleted: usize, count: usize, freed: u64) -> String {
    if deleted == count {
        return format!(
            "Deleted {} sessions (freed {})",
            deleted,
            format_size(freed)
        );
    }
    format!(
        "Deleted {}/{} sessions (some files could not be removed)",
        deleted, count
    )
}
