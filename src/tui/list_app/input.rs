//! Rename and import input handling for the list app.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use super::*;
use crate::files::{codec, filename};
use crate::tui::import;

/// Rename input pre-filled for a session path: its stem without `.cast` / `.cast.gz`.
pub(super) fn rename_stem(path: &std::path::Path) -> String {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    codec::session_stem(name).to_string()
}

/// Longest stem that still fits `MAX_FILENAME_LENGTH` with the path's real suffix.
pub(super) fn max_stem_len(path: &std::path::Path) -> usize {
    filename::MAX_FILENAME_LENGTH.saturating_sub(codec::session_suffix(path).len())
}

impl ListApp {
    /// Enter rename input mode with current filename stem pre-filled.
    pub(super) fn enter_rename_mode(&mut self) {
        if let Some(item) = self.shared.explorer.selected_item() {
            let stem = rename_stem(std::path::Path::new(&item.path));
            self.rename_cursor = stem.len();
            self.rename_input = stem;
            self.rename_selected_all = true;
            self.mode = Mode::RenameInput;
        }
    }

    /// Delete the character before the cursor (Backspace key).
    ///
    /// Clears entire input if all-selected; otherwise removes character at char boundary.
    pub(super) fn handle_rename_backspace(&mut self) {
        if self.rename_selected_all {
            self.rename_input.clear();
            self.rename_cursor = 0;
            self.rename_selected_all = false;
        } else if self.rename_cursor > 0 {
            let prev = self.rename_input[..self.rename_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.rename_input.remove(prev);
            self.rename_cursor = prev;
        }
    }

    /// Delete the character at the cursor (Delete key).
    ///
    /// Clears entire input if all-selected; otherwise removes char at cursor position.
    pub(super) fn handle_rename_delete(&mut self) {
        if self.rename_selected_all {
            self.rename_input.clear();
            self.rename_cursor = 0;
            self.rename_selected_all = false;
        } else if self.rename_cursor < self.rename_input.len() {
            self.rename_input.remove(self.rename_cursor);
            // cursor stays — next char slides into position
        }
    }

    /// Insert a character at the cursor position, respecting max-length limits.
    ///
    /// Validates the char, computes the extension-aware stem limit, then either
    /// replaces the whole input (when all-selected) or inserts at cursor.
    pub(super) fn handle_rename_char_input(&mut self, c: char) -> bool {
        if !filename::is_valid_filename_char(c) {
            return false;
        }
        let max_stem_len = self
            .shared
            .explorer
            .selected_item()
            .map_or(0, |item| max_stem_len(std::path::Path::new(&item.path)));

        if self.rename_selected_all {
            if c.len_utf8() > max_stem_len {
                return false;
            }
            self.rename_input.clear();
            self.rename_input.push(c);
            self.rename_cursor = c.len_utf8();
            self.rename_selected_all = false;
        } else if self.rename_input.len().saturating_add(c.len_utf8()) <= max_stem_len {
            self.rename_input.insert(self.rename_cursor, c);
            self.rename_cursor += c.len_utf8();
        } else {
            return false;
        }
        true
    }

    /// Handle keys in rename input mode.
    pub(super) fn handle_rename_input_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                let success = self.rename_session()?;
                if success {
                    self.mode = Mode::Normal;
                }
                // On error, stay in RenameInput so user can correct
            }
            KeyCode::Backspace => self.handle_rename_backspace(),
            KeyCode::Delete => self.handle_rename_delete(),
            KeyCode::Left => {
                self.rename_selected_all = false;
                if self.rename_cursor > 0 {
                    self.rename_cursor = self.rename_input[..self.rename_cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                }
            }
            KeyCode::Right => {
                self.rename_selected_all = false;
                if self.rename_cursor < self.rename_input.len() {
                    self.rename_cursor += self.rename_input[self.rename_cursor..]
                        .chars()
                        .next()
                        .map(|c| c.len_utf8())
                        .unwrap_or(0);
                }
            }
            KeyCode::Home => {
                self.rename_selected_all = false;
                self.rename_cursor = 0;
            }
            KeyCode::End => {
                self.rename_selected_all = false;
                self.rename_cursor = self.rename_input.len();
            }
            KeyCode::Char(c) => {
                self.handle_rename_char_input(c);
            }
            _ => {}
        }
        Ok(())
    }

    /// Handle keys in import mode.
    pub(super) fn handle_import_key(&mut self, key: KeyEvent) -> Result<()> {
        let state = self
            .import_state
            .as_mut()
            .expect("import_state must exist in Import mode");

        match state.phase {
            import::ImportPhase::AgentInput => match key.code {
                KeyCode::Esc => {
                    self.mode = Mode::Normal;
                    self.import_state = None;
                }
                KeyCode::Enter => {
                    if !state.selected_agent().is_empty() {
                        state.phase = import::ImportPhase::Importing;
                        self.execute_import()?;
                    }
                }
                KeyCode::Up => {
                    state.autocomplete_up();
                }
                KeyCode::Down => {
                    state.autocomplete_down();
                }
                KeyCode::Tab => {
                    state.accept_autocomplete();
                    state.update_agent_filter(&self.shared.available_agents);
                }
                KeyCode::Backspace => {
                    state.agent_input_backspace();
                    state.update_agent_filter(&self.shared.available_agents);
                }
                KeyCode::Char(c) => {
                    state.agent_input_char(c);
                    state.update_agent_filter(&self.shared.available_agents);
                }
                _ => {}
            },
            import::ImportPhase::Importing => {
                // Ignore all keys during import (synchronous phase)
            }
            import::ImportPhase::Done => match key.code {
                KeyCode::Enter | KeyCode::Esc => {
                    self.mode = Mode::Normal;
                    self.import_state = None;
                }
                _ => {}
            },
        }
        Ok(())
    }

    /// Execute the import operation for all paths in import_state.
    pub(super) fn execute_import(&mut self) -> Result<()> {
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
