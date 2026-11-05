//! Keyboard handlers for the list app modes.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use super::{ContextMenuItem, ListApp, Mode};
use crate::files::backup::has_backup;
use crate::files::lock;
use crate::tui::app::{classify_confirm_key, ConfirmAction};

impl ListApp {
    /// If the selected item is locked, set mode to `ConfirmUnlock` and return `true`.
    ///
    /// Returns `false` when the item is not locked, so the caller can proceed normally.
    pub(super) fn redirect_if_locked(&mut self) -> bool {
        if self.is_selected_locked() {
            self.mode = Mode::ConfirmUnlock;
            true
        } else {
            false
        }
    }

    /// Handle keys in normal mode (app-specific only).
    ///
    /// Navigation (up/down/pgup/pgdn/home/end) and mode transitions
    /// (`/`, `f`, `?`) are handled by `handle_shared_key`. This only
    /// handles app-specific keys: Enter, shortcuts, and Esc.
    pub(super) fn handle_normal_key(&mut self, key: KeyEvent) -> Result<()> {
        // Guard: keys that require an unlocked session
        let needs_unlock = matches!(
            key.code,
            KeyCode::Enter
                | KeyCode::Char('p')
                | KeyCode::Char('t')
                | KeyCode::Char('a')
                | KeyCode::Char('r')
                | KeyCode::Char('d')
        );
        if needs_unlock && self.redirect_if_locked() {
            return Ok(());
        }

        match key.code {
            KeyCode::Enter => {
                if self.shared.explorer.selected_item().is_some() {
                    self.context_menu_idx = 0;
                    self.mode = Mode::ContextMenu;
                }
            }
            KeyCode::Char('p') => self.play_session()?,
            KeyCode::Char('c') => self.copy_to_clipboard()?,
            KeyCode::Char('t') => self.optimize_session()?,
            KeyCode::Char('a') => self.analyze_session()?,
            KeyCode::Char('r') => self.enter_rename_mode(),
            KeyCode::Char('d') => {
                if self.shared.explorer.selected_item().is_some() {
                    self.mode = Mode::ConfirmDelete;
                }
            }
            KeyCode::Esc => {
                self.shared.explorer.clear_filters();
                self.shared.search_input.clear();
                self.shared.agent_filter_idx = 0;
            }
            KeyCode::Char('q') => self.app.quit(),
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
                self.delete_session()?;
                self.mode = Mode::Normal;
            }
            ConfirmAction::Cancelled => self.mode = Mode::Normal,
            ConfirmAction::Ignored => {}
        }
        Ok(())
    }

    /// Handle keys in context menu mode.
    pub(super) fn handle_context_menu_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            // Navigation
            KeyCode::Up | KeyCode::Char('k') => {
                if self.context_menu_idx > 0 {
                    self.context_menu_idx -= 1;
                } else {
                    self.context_menu_idx = ContextMenuItem::ALL.len() - 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.context_menu_idx = (self.context_menu_idx + 1) % ContextMenuItem::ALL.len();
            }

            // Execute selected action
            KeyCode::Enter => {
                self.execute_context_menu_action()?;
            }

            // Shortcut keys for menu items
            KeyCode::Char('p') => {
                self.context_menu_idx = ContextMenuItem::ALL
                    .iter()
                    .position(|i| matches!(i, ContextMenuItem::Play))
                    .unwrap_or(0);
                self.execute_context_menu_action()?;
            }
            KeyCode::Char('c') => {
                self.context_menu_idx = ContextMenuItem::ALL
                    .iter()
                    .position(|i| matches!(i, ContextMenuItem::Copy))
                    .unwrap_or(0);
                self.execute_context_menu_action()?;
            }
            KeyCode::Char('t') => {
                self.context_menu_idx = ContextMenuItem::ALL
                    .iter()
                    .position(|i| matches!(i, ContextMenuItem::Optimize))
                    .unwrap_or(0);
                self.execute_context_menu_action()?;
            }
            KeyCode::Char('a') => {
                self.context_menu_idx = ContextMenuItem::ALL
                    .iter()
                    .position(|i| matches!(i, ContextMenuItem::Analyze))
                    .unwrap_or(0);
                self.execute_context_menu_action()?;
            }
            KeyCode::Char('r') => {
                self.context_menu_idx = ContextMenuItem::ALL
                    .iter()
                    .position(|i| matches!(i, ContextMenuItem::Rename))
                    .unwrap_or(0);
                self.execute_context_menu_action()?;
            }
            KeyCode::Char('d') => {
                self.context_menu_idx = ContextMenuItem::ALL
                    .iter()
                    .position(|i| matches!(i, ContextMenuItem::Delete))
                    .unwrap_or(0);
                self.execute_context_menu_action()?;
            }
            // Close menu
            KeyCode::Esc => {
                self.mode = Mode::Normal;
            }

            _ => {}
        }
        Ok(())
    }

    /// Handle keys in optimize result mode.
    pub(super) fn handle_optimize_result_key(&mut self, key: KeyEvent) -> Result<()> {
        // Enter or Esc dismisses the modal
        if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
            self.mode = Mode::Normal;
            self.optimize_result = None;
        }
        Ok(())
    }

    /// Check if the currently selected item is locked by an active recording.
    pub(super) fn is_selected_locked(&self) -> bool {
        self.shared
            .explorer
            .selected_item()
            .and_then(|item| item.lock_info.as_ref())
            .is_some()
    }

    /// Handle keys in confirm unlock mode (first confirmation).
    pub(super) fn handle_confirm_unlock_key(&mut self, key: KeyEvent) -> Result<()> {
        match classify_confirm_key(&key) {
            ConfirmAction::Confirmed => self.mode = Mode::ConfirmUnlockFinal,
            ConfirmAction::Cancelled => self.mode = Mode::Normal,
            ConfirmAction::Ignored => {}
        }
        Ok(())
    }

    /// Remove the lock on the currently selected session.
    pub(super) fn remove_selected_lock(&mut self) {
        if let Some(item) = self.shared.explorer.selected_item() {
            let path = std::path::Path::new(&item.path);
            lock::remove_lock(path);
            self.shared.explorer.refresh_visible_item_metadata();
            self.shared.status_message = Some("Lock removed".to_string());
        }
    }

    /// Handle keys in final unlock confirmation ("Are you sure?").
    pub(super) fn handle_confirm_unlock_final_key(&mut self, key: KeyEvent) -> Result<()> {
        match classify_confirm_key(&key) {
            ConfirmAction::Confirmed => {
                self.remove_selected_lock();
                self.mode = Mode::Normal;
            }
            ConfirmAction::Cancelled => self.mode = Mode::Normal,
            ConfirmAction::Ignored => {}
        }
        Ok(())
    }

    /// Execute the currently selected context menu action.
    pub(super) fn execute_context_menu_action(&mut self) -> Result<()> {
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
}
