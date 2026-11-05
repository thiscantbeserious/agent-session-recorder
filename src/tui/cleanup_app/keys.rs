//! Keyboard handlers for the cleanup app modes.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{CleanupApp, Mode};
use crate::tui::app::{classify_confirm_key, ConfirmAction};

impl CleanupApp {
    ///
    /// Navigation (up/down/pgup/pgdn/home/end) and mode transitions
    /// (`/`, `f`, `?`) are handled by `handle_shared_key`. This only
    /// handles app-specific keys: Space, a, g, Enter, Esc, q.
    pub(super) fn handle_normal_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            // Selection (skip locked items)
            KeyCode::Char(' ') => {
                if let Some(item) = self.shared.explorer.selected_item() {
                    if item.lock_info.is_some() {
                        self.shared.status_message =
                            Some("Session is locked (being recorded)".to_string());
                        return Ok(());
                    }
                }
                self.shared.explorer.toggle_select();
            }
            KeyCode::Char('a') => {
                self.shared.explorer.toggle_all();
            }
            KeyCode::Char('g') => {
                self.mode = Mode::GlobSelect;
                self.glob_input.clear();
            }

            // Actions
            KeyCode::Enter => {
                if self.shared.explorer.selected_count() > 0 {
                    self.mode = Mode::ConfirmDelete;
                }
            }

            // Clear/Cancel
            KeyCode::Esc => {
                if self.shared.explorer.selected_count() > 0 {
                    // First Esc clears selection
                    self.shared.explorer.select_none();
                } else {
                    // Second Esc clears filters
                    self.shared.explorer.clear_filters();
                    self.shared.search_input.clear();
                    self.shared.agent_filter_idx = 0;
                }
            }

            // Quit
            KeyCode::Char('q') => self.app.quit(),

            _ => {}
        }
        Ok(())
    }

    /// Handle keys in glob select mode.
    pub(super) fn handle_glob_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                // Select items matching glob pattern
                if !self.glob_input.is_empty() {
                    let pattern = self.glob_input.clone();
                    let matched = self.select_by_glob(&pattern);
                    self.shared.status_message =
                        Some(format!("Selected {} matching files", matched));
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.glob_input.pop();
            }
            KeyCode::Char(c)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.glob_input.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    /// Handle keys in confirm delete mode (first confirmation).
    pub(super) fn handle_confirm_delete_key(&mut self, key: KeyEvent) -> Result<()> {
        match classify_confirm_key(&key) {
            ConfirmAction::Confirmed => self.mode = Mode::ConfirmDeleteFinal,
            ConfirmAction::Cancelled => self.mode = Mode::Normal,
            ConfirmAction::Ignored => {}
        }
        Ok(())
    }

    /// Handle keys in final delete confirmation ("Are you sure?").
    pub(super) fn handle_confirm_delete_final_key(&mut self, key: KeyEvent) -> Result<()> {
        match classify_confirm_key(&key) {
            ConfirmAction::Confirmed => {
                self.delete_selected()?;
                self.mode = Mode::Normal;
            }
            ConfirmAction::Cancelled => self.mode = Mode::Normal,
            ConfirmAction::Ignored => {}
        }
        Ok(())
    }
}
