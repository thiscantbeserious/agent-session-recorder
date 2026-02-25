//! Session action methods for `ListApp`.
//!
//! Contains all operations that mutate session files on disk or launch external
//! processes: play, copy, delete, restore, optimize, analyze, rename, import.

use std::path::Path;

use anyhow::Result;

use super::{ListApp, Mode, OptimizeResultState};
use crate::asciicast::apply_transforms;
use crate::files::backup::{backup_path_for, create_backup, has_backup, restore_from_backup};
use crate::files::filename;

impl ListApp {
    /// Execute the currently selected context menu action.
    pub(super) fn execute_context_menu_action(&mut self) -> Result<()> {
        use super::ContextMenuItem;

        let action = ContextMenuItem::ALL[self.context_menu_idx];

        // Guard: check if Restore is disabled (no backup)
        if matches!(action, ContextMenuItem::Restore) {
            if let Some(item) = self.shared.explorer.selected_item() {
                let path = std::path::Path::new(&item.path);
                if !has_backup(path) {
                    self.mode = Mode::Normal;
                    self.shared.status_message =
                        Some(format!("No backup exists for: {}", item.name.clone()));
                    return Ok(());
                }
            }
        }

        self.mode = Mode::Normal; // Close menu first

        match action {
            ContextMenuItem::Play => self.play_session()?,
            ContextMenuItem::Copy => self.copy_to_clipboard()?,
            ContextMenuItem::Rename => self.enter_rename_mode(),
            ContextMenuItem::Optimize => self.optimize_session()?,
            ContextMenuItem::Analyze => self.analyze_session()?,
            ContextMenuItem::Restore => self.restore_session()?,
            ContextMenuItem::Delete => {
                if self.shared.explorer.selected_item().is_some() {
                    self.mode = Mode::ConfirmDelete;
                }
            }
        }
        Ok(())
    }

    /// Play the selected session with asciinema.
    pub(super) fn play_session(&mut self) -> Result<()> {
        use crate::player;

        if let Some(item) = self.shared.explorer.selected_item() {
            let path = Path::new(&item.path);

            // Suspend TUI - restores normal terminal mode
            self.app.suspend()?;

            // Play the session
            let result = player::play_session(path)?;

            // Resume TUI - re-enters alternate screen and raw mode
            self.app.resume()?;
            self.shared.status_message = Some(result.message());
        }
        Ok(())
    }

    /// Copy the selected session to the clipboard.
    pub(super) fn copy_to_clipboard(&mut self) -> Result<()> {
        use crate::clipboard::copy_file_to_clipboard;

        if let Some(item) = self.shared.explorer.selected_item() {
            let path = Path::new(&item.path);

            // Extract filename without .cast extension
            let filename = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("recording");

            match copy_file_to_clipboard(path) {
                Ok(result) => {
                    self.shared.status_message = Some(result.message(filename));
                }
                Err(e) => {
                    self.shared.status_message = Some(format!("Copy failed: {}", e));
                }
            }
        }
        Ok(())
    }

    /// Delete the selected session.
    pub(super) fn delete_session(&mut self) -> Result<()> {
        if let Some(item) = self.shared.explorer.selected_item() {
            let path = item.path.clone();
            let name = item.name.clone();

            // Delete the file
            if let Err(e) = std::fs::remove_file(&path) {
                self.shared.status_message = Some(format!("Failed to delete: {}", e));
            } else {
                // Also delete backup if it exists (remove_file returns Err if not found)
                let backup = backup_path_for(std::path::Path::new(&path));
                let backup_deleted = std::fs::remove_file(&backup).is_ok();

                // Remove from explorer to keep UI in sync
                self.shared.explorer.remove_item(&path);

                // Update status message
                self.shared.status_message = Some(if backup_deleted {
                    format!("Deleted: {} (and backup)", name)
                } else {
                    format!("Deleted: {}", name)
                });
            }
        }
        Ok(())
    }

    /// Restore the selected session from its backup.
    pub(super) fn restore_session(&mut self) -> Result<()> {
        if let Some(item) = self.shared.explorer.selected_item() {
            let path = std::path::Path::new(&item.path);
            let name = item.name.clone();
            let path_str = item.path.clone();

            // Attempt restore (restore_from_backup handles missing backup case)
            match restore_from_backup(path) {
                Ok(()) => {
                    // Invalidate the preview cache for this file
                    self.shared.preview_cache.invalidate(&path_str);
                    // Refresh file metadata in explorer
                    self.shared.explorer.update_item_metadata(&path_str);
                    self.shared.status_message = Some(format!("Restored from backup: {}", name));
                }
                Err(e) => {
                    self.shared.status_message = Some(format!("Failed to restore: {}", e));
                }
            }
        }
        Ok(())
    }

    /// Optimize the selected session (apply silence removal).
    pub(super) fn optimize_session(&mut self) -> Result<()> {
        if let Some(item) = self.shared.explorer.selected_item() {
            let path = std::path::Path::new(&item.path);
            let name = item.name.clone();
            let path_str = item.path.clone();

            // Apply transforms and store result for modal display
            let result = match apply_transforms(path) {
                Ok(result) => {
                    // Invalidate the preview cache for this file
                    self.shared.preview_cache.invalidate(&path_str);
                    // Refresh file metadata in explorer
                    self.shared.explorer.update_item_metadata(&path_str);
                    Ok(result)
                }
                Err(e) => Err(e.to_string()),
            };

            // Store result and show modal
            self.optimize_result = Some(OptimizeResultState {
                filename: name,
                result,
            });
            self.mode = Mode::OptimizeResult;
        }
        Ok(())
    }

    /// Analyze the selected session using the analyze subcommand.
    pub(super) fn analyze_session(&mut self) -> Result<()> {
        if let Some(item) = self.shared.explorer.selected_item() {
            let path = item.path.clone();

            // Create backup before analysis
            let file_path = std::path::Path::new(&path);
            if let Err(e) = create_backup(file_path) {
                self.shared.status_message =
                    Some(format!("ERROR: Backup failed for {}: {}", path, e));
                return Ok(());
            }

            // Suspend TUI - restores normal terminal mode
            self.app.suspend()?;

            // Run the analyze subcommand (--wait pauses before returning to TUI)
            let status = std::process::Command::new(std::env::current_exe()?)
                .args(["analyze", &path, "--wait"])
                .status();

            // Resume TUI - re-enters alternate screen and raw mode
            self.app.resume()?;

            self.handle_analyze_result(status, &path, file_path);
        }
        Ok(())
    }

    /// Process the result of the analyze subprocess call.
    fn handle_analyze_result(
        &mut self,
        status: std::io::Result<std::process::ExitStatus>,
        path: &str,
        file_path: &std::path::Path,
    ) {
        match status {
            Ok(s) if s.success() => self.handle_analyze_success(path, file_path),
            Ok(s) => {
                self.shared.status_message = Some(format!(
                    "Analyze exited with code {}",
                    s.code().unwrap_or(-1)
                ));
            }
            Err(e) => {
                self.shared.status_message = Some(format!("Failed to run analyze: {}", e));
            }
        }
    }

    /// Update explorer state after a successful analyze run.
    fn handle_analyze_success(&mut self, path: &str, file_path: &std::path::Path) {
        if !file_path.exists() {
            self.handle_analyze_file_renamed(path, file_path);
        } else {
            // File still exists at original path — just invalidate cache
            self.shared.preview_cache.invalidate(&path.to_string());
            self.shared.explorer.update_item_metadata(path);
            self.shared.status_message = Some("Analysis complete".to_string());
        }
    }

    /// Handle the case where analyze renamed the file.
    fn handle_analyze_file_renamed(&mut self, path: &str, file_path: &std::path::Path) {
        // Check if the original file was renamed by the analyze command
        // Find the newest .cast file in the same directory
        let new_file = file_path.parent().and_then(|parent| {
            std::fs::read_dir(parent).ok().and_then(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.path().extension().and_then(|ext| ext.to_str()) == Some("cast"))
                    .max_by_key(|e| {
                        e.metadata()
                            .and_then(|m| m.modified())
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                    })
                    .map(|e| e.path())
            })
        });

        if let Some(new_path) = new_file {
            let new_path_str = new_path.to_string_lossy().to_string();
            self.shared.preview_cache.invalidate(&new_path_str);
            self.shared.explorer.update_item_path(path, &new_path_str);
            self.shared.status_message = Some(format!(
                "Analysis complete (renamed to {})",
                new_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
            ));
        } else {
            // Couldn't find any .cast file — remove the stale item
            self.shared.explorer.remove_item(path);
            self.shared.status_message = Some("Analysis complete (file was renamed)".to_string());
        }
    }

    /// Enter rename input mode with current filename stem pre-filled.
    pub(super) fn enter_rename_mode(&mut self) {
        if let Some(item) = self.shared.explorer.selected_item() {
            let path = std::path::Path::new(&item.path);
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            self.rename_cursor = stem.len();
            self.rename_input = stem;
            self.rename_selected_all = true;
            self.mode = Mode::RenameInput;
        }
    }

    /// Execute the import operation for all paths in import_state.
    pub(super) fn execute_import(&mut self) -> Result<()> {
        use super::import;

        let state = self.import_state.as_mut().expect("import_state must exist");
        let storage = self.shared.storage.as_ref().expect("storage must exist");
        let agent = state.selected_agent().to_string();

        for path in &state.paths {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            let outcome = storage
                .import_cast_file(path, &agent)
                .map_err(|e| e.to_string());

            state
                .results
                .push(import::ImportResult { filename, outcome });
        }

        state.phase = import::ImportPhase::Done;
        Ok(())
    }

    /// Rename the selected session file on disk.
    ///
    /// Returns `true` on success (or no-op), `false` on error (so caller can
    /// keep the user in rename mode for correction).
    pub(super) fn rename_session(&mut self) -> Result<bool> {
        if let Some(item) = self.shared.explorer.selected_item() {
            let path = std::path::Path::new(&item.path);
            let old_path_str = item.path.clone();

            match filename::rename_file(path, &self.rename_input) {
                Ok(new_path) => {
                    let new_path_str = new_path.to_string_lossy().to_string();
                    if new_path_str != old_path_str {
                        // Invalidate preview cache for old path
                        self.shared.preview_cache.invalidate(&old_path_str);
                        // Update explorer with new path and re-sort/re-filter
                        self.shared
                            .explorer
                            .update_item_path(&old_path_str, &new_path_str);
                        self.shared.explorer.reindex_after_rename(&new_path_str);
                        let new_name = new_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown");
                        self.shared.status_message = Some(format!("Renamed to {}", new_name));
                    }
                    return Ok(true);
                }
                Err(e) => {
                    self.shared.status_message = Some(e.to_string());
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}
