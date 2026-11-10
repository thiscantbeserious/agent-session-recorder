//! Session actions of the list app: play, copy, delete, restore, optimize, analyze.

use std::path::Path;

use anyhow::Result;

use super::{ListApp, Mode, OptimizeResultState};
use crate::asciicast::apply_transforms;
use crate::files::backup::{create_backup, has_backup, restore_from_backup};
use crate::files::{codec, remove_auxiliary_files};

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

            let filename = display_stem(path);
            self.shared.status_message = Some(match copy_file_to_clipboard(path) {
                Ok(result) => result.message(filename),
                Err(e) => format!("Copy failed: {}", e),
            });
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
        let Some(item) = self.shared.explorer.selected_item() else {
            return Ok(());
        };
        let path = item.path.clone();
        if let Err(e) = create_backup(Path::new(&path)) {
            self.shared.status_message = Some(format!("ERROR: Backup failed for {}: {}", path, e));
            return Ok(());
        }
        let status = self.run_analyze(&path)?;
        self.shared.status_message = Some(match status {
            Ok(s) if s.success() => self.after_analyze(&path),
            Ok(s) => format!("Analyze exited with code {}", s.code().unwrap_or(-1)),
            Err(e) => format!("Failed to run analyze: {}", e),
        });
        Ok(())
    }

    /// Run `agr analyze --wait` with the TUI suspended.
    fn run_analyze(&mut self, path: &str) -> Result<std::io::Result<std::process::ExitStatus>> {
        self.app.suspend()?;
        let status = std::process::Command::new(std::env::current_exe()?)
            .args(["analyze", path, "--wait"])
            .status();
        self.app.resume()?;
        Ok(status)
    }

    /// Sync explorer and preview cache after a successful analyze, returns the status text.
    fn after_analyze(&mut self, path: &str) -> String {
        if Path::new(path).exists() {
            self.shared.preview_cache.invalidate(&path.to_string());
            self.shared.explorer.update_item_metadata(path);
            return "Analysis complete".to_string();
        }
        // Renamed by analyze: the renamed session has the most recent mtime
        let Some(new_path) = newest_session_in(Path::new(path).parent()) else {
            self.shared.explorer.remove_item(path);
            return "Analysis complete (file was renamed)".to_string();
        };
        let new_path_str = new_path.to_string_lossy().to_string();
        self.shared.preview_cache.invalidate(&new_path_str);
        self.shared.explorer.update_item_path(path, &new_path_str);
        let name = new_path.file_name().and_then(std::ffi::OsStr::to_str);
        format!(
            "Analysis complete (renamed to {})",
            name.unwrap_or("unknown")
        )
    }
}

/// File name without the `.cast` / `.cast.gz` suffix, "recording" when there is none.
fn display_stem(path: &Path) -> &str {
    let name = path.file_name().and_then(std::ffi::OsStr::to_str);
    name.map_or("recording", codec::session_stem)
}

/// Newest `.cast` / `.cast.gz` session in `dir` by modification time.
pub fn newest_session_in(dir: Option<&Path>) -> Option<std::path::PathBuf> {
    let mtime = |e: &std::fs::DirEntry| {
        e.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    };
    std::fs::read_dir(dir?)
        .ok()?
        .flatten()
        .filter(|e| codec::is_session_path(&e.path()))
        .max_by_key(mtime)
        .map(|e| e.path())
}
