//! Session actions of the list app: play, copy, delete, restore, optimize, analyze.

use std::path::Path;

use anyhow::Result;

use super::*;
use crate::asciicast::apply_transforms;
use crate::files::backup::{create_backup, has_backup, restore_from_backup};
use crate::files::remove_auxiliary_files;

impl ListApp {
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

            let cast_path = std::path::Path::new(&path);
            let had_backup = has_backup(cast_path);

            // Always attempt auxiliary cleanup, even if cast file removal fails
            remove_auxiliary_files(cast_path);

            // Delete the cast file
            if let Err(e) = std::fs::remove_file(&path) {
                self.shared.status_message = Some(format!("Failed to delete: {}", e));
            } else {
                // Remove from explorer to keep UI in sync
                self.shared.explorer.remove_item(&path);

                // Update status message
                self.shared.status_message = Some(if had_backup {
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

            match status {
                Ok(s) if s.success() => {
                    // Check if the original file was renamed by the analyze command
                    if !file_path.exists() {
                        // File was renamed — find the newest .cast file in the same directory
                        // (the renamed file will have the most recent mtime)
                        let new_file = file_path.parent().and_then(|parent| {
                            std::fs::read_dir(parent).ok().and_then(|entries| {
                                entries
                                    .flatten()
                                    .filter(|e| {
                                        e.path().extension().and_then(|ext| ext.to_str())
                                            == Some("cast")
                                    })
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
                            self.shared.explorer.update_item_path(&path, &new_path_str);
                            self.shared.status_message = Some(format!(
                                "Analysis complete (renamed to {})",
                                new_path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("unknown")
                            ));
                        } else {
                            // Couldn't find any .cast file — remove the stale item
                            self.shared.explorer.remove_item(&path);
                            self.shared.status_message =
                                Some("Analysis complete (file was renamed)".to_string());
                        }
                    } else {
                        // File still exists at original path — just invalidate cache
                        self.shared.preview_cache.invalidate(&path);
                        self.shared.explorer.update_item_metadata(&path);
                        self.shared.status_message = Some("Analysis complete".to_string());
                    }
                }
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
        Ok(())
    }
}
