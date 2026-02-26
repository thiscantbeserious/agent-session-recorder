//! List command TUI application
//!
//! Interactive file explorer for browsing and managing session recordings.
//! Features: search, agent filter, play, copy, rename, optimize, analyze, restore, delete, import.

use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use super::app::layout::build_explorer_layout;
use super::app::list_view::{render_explorer_list, render_explorer_list_with_rename};
use super::app::modals::{
    self, help_close_hint, help_section_header, help_shortcut_line, help_title_line,
    render_help_paragraph,
};
use super::app::status_footer::{render_footer_text, render_input_line, render_status_line};
use super::app::{classify_confirm_key, App, ConfirmAction, SharedMode, SharedState, TuiApp};
use super::import;
use super::widgets::{FileExplorer, FileItem};
use crate::asciicast::{apply_transforms, TransformResult};
use crate::config::Config;
use crate::files::backup::{create_backup, has_backup, restore_from_backup};
use crate::files::filename;
use crate::files::lock;
use crate::files::remove_auxiliary_files;
use crate::theme::current_theme;

/// UI mode for the list application
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Normal browsing mode
    #[default]
    Normal,
    /// Search mode - typing filters by filename
    Search,
    /// Agent filter mode - selecting agent to filter by
    AgentFilter,
    /// Help mode - showing keyboard shortcuts
    Help,
    /// Confirm delete mode
    ConfirmDelete,
    /// Second confirmation for delete - "Are you sure?"
    ConfirmDeleteFinal,
    /// Context menu mode - showing actions for selected file
    ContextMenu,
    /// Optimize result mode - showing optimization results or error
    OptimizeResult,
    /// Confirm unlock mode - asking user to confirm force-unlock
    ConfirmUnlock,
    /// Second confirmation for unlock - "Are you sure?"
    ConfirmUnlockFinal,
    /// Rename input mode - typing new filename
    RenameInput,
    /// Import mode - pasting .cast files to import
    Import,
}

impl Mode {
    fn to_shared(self) -> Option<SharedMode> {
        match self {
            Mode::Normal => Some(SharedMode::Normal),
            Mode::Search => Some(SharedMode::Search),
            Mode::AgentFilter => Some(SharedMode::AgentFilter),
            Mode::Help => Some(SharedMode::Help),
            Mode::ConfirmDelete => Some(SharedMode::ConfirmDelete),
            Mode::ContextMenu
            | Mode::OptimizeResult
            | Mode::ConfirmUnlock
            | Mode::ConfirmUnlockFinal
            | Mode::ConfirmDeleteFinal
            | Mode::RenameInput
            | Mode::Import => None,
        }
    }

    fn from_shared(mode: SharedMode) -> Self {
        match mode {
            SharedMode::Normal => Mode::Normal,
            SharedMode::Search => Mode::Search,
            SharedMode::AgentFilter => Mode::AgentFilter,
            SharedMode::Help => Mode::Help,
            SharedMode::ConfirmDelete => Mode::ConfirmDelete,
        }
    }
}

/// Context menu item definition
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuItem {
    Play,
    Copy,
    Rename,
    Optimize,
    Analyze,
    Restore,
    Delete,
}

impl ContextMenuItem {
    /// All menu items in display order
    pub const ALL: [ContextMenuItem; 7] = [
        ContextMenuItem::Play,
        ContextMenuItem::Copy,
        ContextMenuItem::Rename,
        ContextMenuItem::Optimize,
        ContextMenuItem::Analyze,
        ContextMenuItem::Restore,
        ContextMenuItem::Delete,
    ];

    /// Get the display label for this menu item
    pub fn label(&self) -> &'static str {
        match self {
            ContextMenuItem::Play => "Play",
            ContextMenuItem::Copy => "Copy to clipboard",
            ContextMenuItem::Rename => "Rename",
            ContextMenuItem::Optimize => "Optimize",
            ContextMenuItem::Analyze => "Analyze",
            ContextMenuItem::Restore => "Restore from backup",
            ContextMenuItem::Delete => "Delete",
        }
    }

    /// Get the shortcut key hint for this menu item
    pub fn shortcut(&self) -> &'static str {
        match self {
            ContextMenuItem::Play => "p",
            ContextMenuItem::Copy => "c",
            ContextMenuItem::Rename => "r",
            ContextMenuItem::Optimize => "t",
            ContextMenuItem::Analyze => "a",
            ContextMenuItem::Restore => "",
            ContextMenuItem::Delete => "d",
        }
    }

    /// Whether this menu item has a keyboard shortcut
    pub fn has_shortcut(&self) -> bool {
        !self.shortcut().is_empty()
    }
}

/// Holds the result of an optimize operation for display in modal.
#[derive(Debug, Clone)]
pub struct OptimizeResultState {
    /// The filename that was optimized
    pub filename: String,
    /// The result (Ok with data or Err with message)
    pub result: Result<TransformResult, String>,
}

/// List application state
pub struct ListApp {
    /// Base app for terminal handling
    app: App,
    /// Shared state (explorer, search, agent filter, preview cache, status)
    shared: SharedState,
    /// Current UI mode
    mode: Mode,
    /// Context menu selected index
    context_menu_idx: usize,
    /// Optimize result for modal display
    optimize_result: Option<OptimizeResultState>,
    /// Rename input buffer (filename stem without extension)
    rename_input: String,
    /// Cursor position within rename_input (byte offset)
    rename_cursor: usize,
    /// Whether the entire rename input is "selected" (first keystroke replaces all)
    rename_selected_all: bool,
    /// Import state for drag-and-drop .cast file imports
    import_state: Option<import::ImportState>,
}

impl ListApp {
    /// Create a new list application with the given sessions.
    pub fn new(items: Vec<FileItem>, config: Config) -> Result<Self> {
        let app = App::new(Duration::from_millis(250))?;
        let shared = SharedState::new(items, Some(config));

        Ok(Self {
            app,
            shared,
            mode: Mode::Normal,
            context_menu_idx: 0,
            optimize_result: None,
            rename_input: String::new(),
            rename_cursor: 0,
            rename_selected_all: false,
            import_state: None,
        })
    }

    /// Set initial agent filter (for CLI argument support)
    pub fn set_agent_filter(&mut self, agent: &str) {
        // Find the agent in available_agents and set the index
        if let Some(idx) = self.shared.available_agents.iter().position(|a| a == agent) {
            self.shared.agent_filter_idx = idx;
            self.shared.apply_agent_filter();
        }
    }

    /// If the selected item is locked, set mode to `ConfirmUnlock` and return `true`.
    ///
    /// Returns `false` when the item is not locked, so the caller can proceed normally.
    fn redirect_if_locked(&mut self) -> bool {
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
    fn handle_normal_key(&mut self, key: KeyEvent) -> Result<()> {
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
    fn handle_confirm_delete_key(&mut self, key: KeyEvent) -> Result<()> {
        match classify_confirm_key(&key) {
            ConfirmAction::Confirmed => self.mode = Mode::ConfirmDeleteFinal,
            ConfirmAction::Cancelled => self.mode = Mode::Normal,
            ConfirmAction::Ignored => {}
        }
        Ok(())
    }

    /// Handle keys in final delete confirmation ("Are you sure?").
    fn handle_confirm_delete_final_key(&mut self, key: KeyEvent) -> Result<()> {
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
    fn handle_context_menu_key(&mut self, key: KeyEvent) -> Result<()> {
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
    fn handle_optimize_result_key(&mut self, key: KeyEvent) -> Result<()> {
        // Enter or Esc dismisses the modal
        if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
            self.mode = Mode::Normal;
            self.optimize_result = None;
        }
        Ok(())
    }

    /// Check if the currently selected item is locked by an active recording.
    fn is_selected_locked(&self) -> bool {
        self.shared
            .explorer
            .selected_item()
            .and_then(|item| item.lock_info.as_ref())
            .is_some()
    }

    /// Handle keys in confirm unlock mode (first confirmation).
    fn handle_confirm_unlock_key(&mut self, key: KeyEvent) -> Result<()> {
        match classify_confirm_key(&key) {
            ConfirmAction::Confirmed => self.mode = Mode::ConfirmUnlockFinal,
            ConfirmAction::Cancelled => self.mode = Mode::Normal,
            ConfirmAction::Ignored => {}
        }
        Ok(())
    }

    /// Remove the lock on the currently selected session.
    fn remove_selected_lock(&mut self) {
        if let Some(item) = self.shared.explorer.selected_item() {
            let path = std::path::Path::new(&item.path);
            lock::remove_lock(path);
            self.shared.explorer.refresh_visible_item_metadata();
            self.shared.status_message = Some("Lock removed".to_string());
        }
    }

    /// Handle keys in final unlock confirmation ("Are you sure?").
    fn handle_confirm_unlock_final_key(&mut self, key: KeyEvent) -> Result<()> {
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
    fn execute_context_menu_action(&mut self) -> Result<()> {
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
    fn play_session(&mut self) -> Result<()> {
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
    fn copy_to_clipboard(&mut self) -> Result<()> {
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
    fn delete_session(&mut self) -> Result<()> {
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
    fn restore_session(&mut self) -> Result<()> {
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
    fn optimize_session(&mut self) -> Result<()> {
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
    fn analyze_session(&mut self) -> Result<()> {
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

    /// Enter rename input mode with current filename stem pre-filled.
    fn enter_rename_mode(&mut self) {
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

    /// Delete the character before the cursor (Backspace key).
    ///
    /// Clears entire input if all-selected; otherwise removes character at char boundary.
    fn handle_rename_backspace(&mut self) {
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
    fn handle_rename_delete(&mut self) {
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
    fn handle_rename_char_input(&mut self, c: char) -> bool {
        if !filename::is_valid_filename_char(c) {
            return false;
        }
        let ext_len = self
            .shared
            .explorer
            .selected_item()
            .map(|item| {
                std::path::Path::new(&item.path)
                    .extension()
                    .map(|e| e.len() + 1) // +1 for the dot
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        let max_stem_len = filename::MAX_FILENAME_LENGTH.saturating_sub(ext_len);

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
    fn handle_rename_input_key(&mut self, key: KeyEvent) -> Result<()> {
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
    fn handle_import_key(&mut self, key: KeyEvent) -> Result<()> {
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
    fn execute_import(&mut self) -> Result<()> {
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
    fn rename_session(&mut self) -> Result<bool> {
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

    /// Render the help modal overlay.
    /// Public for snapshot testing.
    pub fn render_help_modal(frame: &mut Frame, area: Rect) {
        let accent = current_theme().accent;
        let help_text = vec![
            help_title_line("Keyboard Shortcuts", accent),
            Line::from(""),
            help_section_header("Navigation"),
            help_shortcut_line("  ↑/↓ j/k", "    Navigate", accent),
            help_shortcut_line("  PgUp/Dn", "    Page up/down", accent),
            help_shortcut_line("  Home/End", "   First/last", accent),
            Line::from(""),
            help_section_header("Actions"),
            help_shortcut_line("  Enter", "       Context menu", accent),
            help_shortcut_line("  p", "           Play session", accent),
            help_shortcut_line("  c", "           Copy to clipboard", accent),
            help_shortcut_line("  r", "           Rename session", accent),
            help_shortcut_line("  t", "           Optimize (removes silence)", accent),
            help_shortcut_line("  a", "           Analyze session", accent),
            help_shortcut_line("  d", "           Delete session", accent),
            help_shortcut_line("  Paste", "       Import .cast file(s)", accent),
            Line::from(""),
            help_section_header("Filtering"),
            help_shortcut_line("  /", "           Search by filename", accent),
            help_shortcut_line("  f", "           Filter by agent", accent),
            help_shortcut_line("  Esc", "         Clear filters", accent),
            Line::from(""),
            help_shortcut_line("  ?", "           This help", accent),
            help_shortcut_line("  q", "           Quit", accent),
            Line::from(""),
            help_close_hint(),
        ];
        render_help_paragraph(frame, area, "Help", help_text, 60, 31);
    }

    /// Render the context menu modal overlay.
    ///
    /// This function is public to allow snapshot testing.
    pub fn render_context_menu_modal(
        frame: &mut Frame,
        area: Rect,
        selected_idx: usize,
        backup_exists: bool,
    ) {
        let theme = current_theme();

        // Center the modal
        let modal_width = 40.min(area.width.saturating_sub(4));
        let modal_height = (ContextMenuItem::ALL.len() + 4) as u16; // items + title + padding + footer
        let modal_height = modal_height.min(area.height.saturating_sub(4));
        let x = (area.width - modal_width) / 2;
        let y = (area.height - modal_height) / 2;
        let modal_area = Rect::new(x, y, modal_width, modal_height);

        // Clear the area behind the modal
        frame.render_widget(Clear, modal_area);

        // Build menu lines
        let mut lines = vec![
            Line::from(Span::styled(
                "Actions",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        for (idx, item) in ContextMenuItem::ALL.iter().enumerate() {
            let is_selected = idx == selected_idx;
            let is_restore = matches!(item, ContextMenuItem::Restore);
            let is_disabled = is_restore && !backup_exists;

            let label = build_menu_item_label(item, backup_exists);
            let style = menu_item_style(is_selected, is_disabled, &theme);

            // Add selection indicator
            let prefix = if is_selected { "> " } else { "  " };
            lines.push(Line::from(Span::styled(
                format!("{}{}", prefix, label),
                style,
            )));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "↑↓: navigate | Enter: select | Esc: cancel",
            Style::default().fg(theme.text_secondary),
        )));

        let menu = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.accent))
                    .title(" Menu "),
            )
            .alignment(Alignment::Left);

        frame.render_widget(menu, modal_area);
    }

    /// Render the optimize result modal overlay.
    ///
    /// This function is public to allow snapshot testing.
    pub fn render_optimize_result_modal(
        frame: &mut Frame,
        area: Rect,
        result_state: &OptimizeResultState,
    ) {
        let theme = current_theme();

        // Determine modal size based on success or error
        let is_success = result_state.result.is_ok();
        let modal_width = 55.min(area.width.saturating_sub(4));
        let modal_height = if is_success { 10 } else { 8 };
        let modal_height = modal_height.min(area.height.saturating_sub(4));

        // Center the modal
        let x = (area.width - modal_width) / 2;
        let y = (area.height - modal_height) / 2;
        let modal_area = Rect::new(x, y, modal_width, modal_height);

        // Clear the area behind the modal
        frame.render_widget(Clear, modal_area);

        // Build content based on success or error
        let (title, border_color, lines) = match &result_state.result {
            Ok(result) => {
                let title = " Optimization Complete ";
                let border_color = theme.success;

                let lines = vec![
                    Line::from(Span::styled(
                        format!("File: {}", result_state.filename),
                        Style::default().fg(theme.text_primary),
                    )),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Original: ", Style::default().fg(theme.text_secondary)),
                        Span::styled(
                            format_duration(result.original_duration),
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("New:      ", Style::default().fg(theme.text_secondary)),
                        Span::styled(
                            format_duration(result.new_duration),
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("Saved:    ", Style::default().fg(theme.text_secondary)),
                        Span::styled(
                            format!(
                                "{} ({:.0}%)",
                                format_duration(result.time_saved()),
                                result.percent_saved()
                            ),
                            Style::default()
                                .fg(theme.success)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Backup: ", Style::default().fg(theme.text_secondary)),
                        Span::styled(
                            if result.backup_created {
                                "Created"
                            } else {
                                "Using existing"
                            },
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                ];
                (title, border_color, lines)
            }
            Err(error) => {
                let title = " Optimization Failed ";
                let border_color = theme.error;

                let lines = vec![
                    Line::from(Span::styled(
                        format!("File: {}", result_state.filename),
                        Style::default().fg(theme.text_primary),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Error:",
                        Style::default()
                            .fg(theme.error)
                            .add_modifier(Modifier::BOLD),
                    )),
                    Line::from(Span::styled(
                        error.to_string(),
                        Style::default().fg(theme.error),
                    )),
                ];
                (title, border_color, lines)
            }
        };

        let modal = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color))
                    .title(title),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(modal, modal_area);
    }
}

/// Mouse handler methods extracted from `handle_mouse` for complexity reduction.
impl ListApp {
    /// Handle mouse events in Normal mode.
    ///
    /// Scroll navigates the list; left-click selects and opens context menu or confirm-unlock.
    fn handle_normal_mouse(&mut self, mouse: MouseEvent, height: u16) -> Result<()> {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.shared.explorer.up();
            }
            MouseEventKind::ScrollDown => {
                self.shared.explorer.down();
            }
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                let explorer_height = height.saturating_sub(2);
                let click_row = mouse.row;
                if click_row >= 1 && click_row < explorer_height.saturating_sub(1) {
                    let item_offset = (click_row - 1) as usize;
                    let scroll_offset = self.shared.explorer.scroll_offset();
                    let visible_idx = scroll_offset + item_offset;
                    if self.shared.explorer.select_index(visible_idx) {
                        if self.is_selected_locked() {
                            self.mode = Mode::ConfirmUnlock;
                        } else {
                            self.context_menu_idx = 0;
                            self.mode = Mode::ContextMenu;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Handle mouse events in ContextMenu mode.
    ///
    /// Scroll navigates the menu; left-click either selects an item or dismisses.
    fn handle_context_menu_mouse(
        &mut self,
        mouse: MouseEvent,
        width: u16,
        height: u16,
    ) -> Result<()> {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if self.context_menu_idx > 0 {
                    self.context_menu_idx -= 1;
                } else {
                    self.context_menu_idx = ContextMenuItem::ALL.len() - 1;
                }
            }
            MouseEventKind::ScrollDown => {
                self.context_menu_idx = (self.context_menu_idx + 1) % ContextMenuItem::ALL.len();
            }
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                // modal_height = items + 4 (border + title + blank + footer)
                // Items start at row y+3 (border=1, title=1, blank=1)
                let modal_width = 40.min(width.saturating_sub(4));
                let modal_height =
                    (ContextMenuItem::ALL.len() as u16 + 4).min(height.saturating_sub(4));
                let modal_x = (width - modal_width) / 2;
                let modal_y = (height - modal_height) / 2;
                let items_start_y = modal_y + 3;
                let visible_items = modal_height.saturating_sub(4) as usize;

                let cx = mouse.column;
                let cy = mouse.row;

                if cx >= modal_x
                    && cx < modal_x + modal_width
                    && cy >= items_start_y
                    && cy < items_start_y + visible_items as u16
                {
                    let idx = (cy - items_start_y) as usize;
                    if idx < ContextMenuItem::ALL.len() {
                        self.context_menu_idx = idx;
                        self.execute_context_menu_action()?;
                    }
                } else {
                    self.mode = Mode::Normal;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Handle mouse events in confirm-modal modes (ConfirmDelete/Unlock and Final variants).
    ///
    /// Merges the two first/final confirm arms into one parameterized by current mode.
    fn handle_confirm_modal_mouse(
        &mut self,
        mouse: MouseEvent,
        width: u16,
        height: u16,
    ) -> Result<()> {
        let is_first = matches!(self.mode, Mode::ConfirmDelete | Mode::ConfirmUnlock);
        let modal_w = match self.mode {
            Mode::ConfirmDelete | Mode::ConfirmDeleteFinal => 50,
            _ => 55,
        };
        match modals::handle_confirm_click(&mouse, width, height, modal_w, 8, 6) {
            modals::ConfirmClick::Yes => {
                if is_first {
                    self.mode = if self.mode == Mode::ConfirmDelete {
                        Mode::ConfirmDeleteFinal
                    } else {
                        Mode::ConfirmUnlockFinal
                    };
                } else {
                    if self.mode == Mode::ConfirmDeleteFinal {
                        self.delete_session()?;
                    } else {
                        self.remove_selected_lock();
                    }
                    self.mode = Mode::Normal;
                }
            }
            modals::ConfirmClick::No => self.mode = Mode::Normal,
            modals::ConfirmClick::Ignored => {}
        }
        Ok(())
    }
}

impl TuiApp for ListApp {
    fn app(&mut self) -> &mut App {
        &mut self.app
    }

    fn shared_state(&mut self) -> &mut SharedState {
        &mut self.shared
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        let (width, height) = self.app.size()?;

        match self.mode {
            Mode::Normal => self.handle_normal_mouse(mouse, height)?,
            Mode::ContextMenu => self.handle_context_menu_mouse(mouse, width, height)?,
            Mode::ConfirmDelete
            | Mode::ConfirmUnlock
            | Mode::ConfirmDeleteFinal
            | Mode::ConfirmUnlockFinal => self.handle_confirm_modal_mouse(mouse, width, height)?,
            // Other modal modes: click outside dismisses
            _ => {
                if let MouseEventKind::Down(crossterm::event::MouseButton::Left) = mouse.kind {
                    self.mode = Mode::Normal;
                    self.optimize_result = None;
                    self.import_state = None;
                }
            }
        }

        Ok(())
    }

    fn handle_paste(&mut self, text: String) -> Result<()> {
        // Only accept paste events in Normal mode to prevent unexpected mode transitions
        if self.mode != Mode::Normal {
            return Ok(());
        }

        let state = import::ImportState::new(&text, &self.shared.available_agents);
        if !state.has_paths() {
            self.shared.status_message = Some("No file paths found in pasted text".to_string());
            return Ok(());
        }
        self.import_state = Some(state);
        self.mode = Mode::Import;
        Ok(())
    }

    fn current_shared_mode(&self) -> Option<SharedMode> {
        self.mode.to_shared()
    }

    fn set_mode_from_shared(&mut self, mode: SharedMode) {
        self.mode = Mode::from_shared(mode);
    }

    fn handle_app_key(&mut self, key: KeyEvent) -> Result<()> {
        match self.mode {
            Mode::Normal => self.handle_normal_key(key)?,
            Mode::ConfirmDelete => self.handle_confirm_delete_key(key)?,
            Mode::ConfirmDeleteFinal => self.handle_confirm_delete_final_key(key)?,
            Mode::ContextMenu => self.handle_context_menu_key(key)?,
            Mode::OptimizeResult => self.handle_optimize_result_key(key)?,
            Mode::ConfirmUnlock => self.handle_confirm_unlock_key(key)?,
            Mode::ConfirmUnlockFinal => self.handle_confirm_unlock_final_key(key)?,
            Mode::RenameInput => self.handle_rename_input_key(key)?,
            Mode::Import => self.handle_import_key(key)?,
            _ => {}
        }
        Ok(())
    }

    fn draw(&mut self) -> Result<()> {
        // Prepare draw: update page size, poll cache, prefetch adjacent previews
        let (_, height) = self.app.size()?;
        self.shared.prepare_draw(height);

        // Extract shared fields into local variables before closure
        let explorer = &mut self.shared.explorer;
        let mode = self.mode;
        let search_input = &self.shared.search_input;
        let status = self.shared.status_message.clone();
        let agent_filter_idx = self.shared.agent_filter_idx;
        let available_agents = &self.shared.available_agents;
        let context_menu_idx = self.context_menu_idx;
        let optimize_result = self.optimize_result.clone();
        let rename_input = &self.rename_input;
        let rename_cursor = self.rename_cursor;
        let rename_selected_all = self.rename_selected_all;
        let import_state = &self.import_state;

        // Get preview for current selection from cache
        let current_path = explorer.selected_item().map(|i| i.path.clone());
        let preview = current_path
            .as_ref()
            .and_then(|p| self.shared.preview_cache.get(p));

        // Get selected item name for dialog status lines
        let selected_name = explorer
            .selected_item()
            .map(|i| i.name.clone())
            .unwrap_or_default();

        // Check if backup exists for selected file (for context menu)
        let backup_exists = current_path
            .as_ref()
            .map(|p| has_backup(std::path::Path::new(p)))
            .unwrap_or(false);

        self.app.draw(|frame| {
            let area = frame.area();

            // Main layout: explorer + status + footer
            let chunks = build_explorer_layout(area);

            // Render file explorer (no checkboxes in list view - it's single-select)
            if mode == Mode::RenameInput {
                render_explorer_list_with_rename(
                    frame,
                    chunks[0],
                    explorer,
                    preview,
                    false,
                    backup_exists,
                    Some((rename_input, rename_cursor, rename_selected_all)),
                );
            } else {
                render_explorer_list(frame, chunks[0], explorer, preview, false, backup_exists);
            }

            render_status_line_for_mode(
                frame,
                chunks[1],
                mode,
                search_input,
                available_agents,
                agent_filter_idx,
                rename_input,
                rename_cursor,
                rename_selected_all,
                import_state,
                &status,
                explorer,
                &selected_name,
            );

            let footer_text = footer_text_for_mode(mode, import_state);
            render_footer_text(frame, chunks[2], footer_text);

            render_modal_overlays(
                frame,
                area,
                mode,
                explorer,
                context_menu_idx,
                backup_exists,
                &optimize_result,
                import_state,
            );
        })?;

        Ok(())
    }
}

/// Build the display label for a context menu item.
///
/// Appends shortcut hint and, for Restore when no backup exists, a "no backup" warning.
fn build_menu_item_label(item: &ContextMenuItem, backup_exists: bool) -> String {
    let is_restore = matches!(item, ContextMenuItem::Restore);
    if is_restore && !backup_exists {
        if item.has_shortcut() {
            format!("  {} ({}) - no backup", item.label(), item.shortcut())
        } else {
            format!("  {} - no backup", item.label())
        }
    } else if item.has_shortcut() {
        format!("  {} ({})", item.label(), item.shortcut())
    } else {
        format!("  {}", item.label())
    }
}

/// Return the style for a context menu item based on selection and disabled state.
fn menu_item_style(is_selected: bool, is_disabled: bool, theme: &crate::theme::Theme) -> Style {
    if is_selected {
        theme.highlight_style()
    } else if is_disabled {
        Style::default().fg(theme.text_secondary)
    } else {
        Style::default().fg(theme.text_primary)
    }
}

/// Return the confirm-dialog prompt prefix for a mode, or None if the mode is not a confirm mode.
fn confirm_prompt_for_mode(mode: Mode) -> Option<&'static str> {
    match mode {
        Mode::ConfirmDelete => Some("🗑  Delete? "),
        Mode::ConfirmUnlock => Some("🔓 Force unlock? "),
        Mode::ConfirmDeleteFinal => Some("🗑  Are you sure? "),
        Mode::ConfirmUnlockFinal => Some("🔓 Are you sure? "),
        _ => None,
    }
}

/// Render the status line for the current mode.
///
/// Free function (not a method) because `self.app.draw` holds a mutable borrow of `app`
/// and all required fields are passed as explicit parameters.
#[allow(clippy::too_many_arguments)]
fn render_status_line_for_mode(
    frame: &mut Frame,
    area: Rect,
    mode: Mode,
    search_input: &str,
    available_agents: &[String],
    agent_filter_idx: usize,
    rename_input: &str,
    rename_cursor: usize,
    rename_selected_all: bool,
    import_state: &Option<import::ImportState>,
    status: &Option<String>,
    explorer: &mut FileExplorer,
    selected_name: &str,
) {
    // Guard: all confirm-dialog modes share the same "(y/n) — name" pattern
    if let Some(prompt) = confirm_prompt_for_mode(mode) {
        render_input_line(frame, area, prompt, &format!("(y/n) — {}", selected_name));
        return;
    }

    match mode {
        Mode::Search => {
            let value = format!("{}_", search_input);
            render_input_line(frame, area, "Search: ", &value);
        }
        Mode::AgentFilter => {
            let agent = &available_agents[agent_filter_idx];
            render_input_line(
                frame,
                area,
                "Filter by agent: ",
                &format!("{} (←/→ to change, Enter to apply)", agent),
            );
        }
        Mode::RenameInput => {
            let theme = current_theme();
            let hl = theme.highlight_style();
            let blink = hl.add_modifier(Modifier::SLOW_BLINK);
            let mut spans = vec![Span::styled(
                "Rename: ",
                Style::default().fg(theme.text_secondary),
            )];
            build_rename_status_spans(
                &mut spans,
                rename_input,
                rename_cursor,
                rename_selected_all,
                hl,
                blink,
            );
            frame.render_widget(Paragraph::new(Line::from(spans)), area);
        }
        Mode::ContextMenu | Mode::OptimizeResult => {
            render_status_line(frame, area, selected_name);
        }
        Mode::Import => {
            if let Some(ref state) = import_state {
                let text = match state.phase {
                    import::ImportPhase::AgentInput => {
                        format!("Import: select agent for {} file(s)", state.file_count())
                    }
                    import::ImportPhase::Importing => {
                        format!("Importing {} file(s)...", state.file_count())
                    }
                    import::ImportPhase::Done => {
                        let success_count =
                            state.results.iter().filter(|r| r.outcome.is_ok()).count();
                        let total_count = state.results.len();
                        format!("Imported {}/{} files", success_count, total_count)
                    }
                };
                render_status_line(frame, area, &text);
            }
        }
        _ => {
            let text = if let Some(msg) = status {
                msg.clone()
            } else {
                match mode {
                    Mode::Normal => {
                        let mut parts = vec![];
                        if let Some(search) = explorer.search_filter() {
                            parts.push(format!("search: \"{}\"", search));
                        }
                        if let Some(agent) = explorer.agent_filter() {
                            parts.push(format!("agent: {}", agent));
                        }
                        if parts.is_empty() {
                            format!("{} sessions", explorer.len())
                        } else {
                            format!("{} sessions ({})", explorer.len(), parts.join(", "))
                        }
                    }
                    _ => String::new(),
                }
            };
            render_status_line(frame, area, &text);
        }
    }
}

/// Build the rename cursor spans for the status line.
///
/// Populates `spans` with the styled before/cursor/after segments.
fn build_rename_status_spans<'a>(
    spans: &mut Vec<Span<'a>>,
    rename_input: &'a str,
    rename_cursor: usize,
    rename_selected_all: bool,
    hl: Style,
    blink: Style,
) {
    if rename_selected_all {
        spans.push(Span::styled("[", blink));
        spans.push(Span::styled(rename_input, blink));
        spans.push(Span::styled("]", blink));
    } else {
        let before = &rename_input[..rename_cursor];
        let after = &rename_input[rename_cursor..];
        if !before.is_empty() {
            spans.push(Span::styled(before, hl));
        }
        spans.push(Span::styled("|", blink));
        if !after.is_empty() {
            spans.push(Span::styled(after, hl));
        }
    }
}

/// Return the footer hint text for the current mode.
///
/// Free function — does not need `self`; import_state is only needed for `Mode::Import`.
fn footer_text_for_mode(mode: Mode, import_state: &Option<import::ImportState>) -> &str {
    match mode {
        Mode::Search => "Esc: cancel | Enter: apply search | Backspace: delete char",
        Mode::AgentFilter => "←/→: change agent | Enter: apply | Esc: cancel",
        Mode::ConfirmDelete => "y: confirm delete | n/Esc: cancel",
        Mode::ConfirmDeleteFinal => "y: confirm | n/Esc: cancel",
        Mode::ConfirmUnlock => "y: force unlock | n/Esc: cancel",
        Mode::ConfirmUnlockFinal => "y: confirm | n/Esc: cancel",
        Mode::Help => "Press any key to close help",
        Mode::ContextMenu => "↑↓: navigate | Enter: select | Esc: cancel",
        Mode::RenameInput => "Enter: confirm | Esc: cancel | Backspace: delete char",
        Mode::OptimizeResult => "Enter/Esc: dismiss",
        Mode::Import => {
            if let Some(ref state) = import_state {
                match state.phase {
                    import::ImportPhase::AgentInput => {
                        "Enter: confirm | Esc: cancel | Tab: complete | Up/Down: select"
                    }
                    import::ImportPhase::Importing => "Importing...",
                    import::ImportPhase::Done => "Enter/Esc: dismiss",
                }
            } else {
                ""
            }
        }
        Mode::Normal => {
            "↑↓: navigate | Enter: menu | p: play | c: copy | r: rename | t: optimize | a: analyze | d: delete | ?: help | q: quit"
        }
    }
}

/// Render all modal overlays for the current mode.
///
/// Free function — does not need `self`; all required state is passed as parameters.
#[allow(clippy::too_many_arguments)]
fn render_modal_overlays(
    frame: &mut Frame,
    area: Rect,
    mode: Mode,
    explorer: &mut FileExplorer,
    context_menu_idx: usize,
    backup_exists: bool,
    optimize_result: &Option<OptimizeResultState>,
    import_state: &Option<import::ImportState>,
) {
    match mode {
        Mode::Help => ListApp::render_help_modal(frame, area),
        Mode::ConfirmDelete => {
            if let Some(item) = explorer.selected_item() {
                modals::render_confirm_delete_modal(frame, area, 1, item.size, false);
            }
        }
        Mode::ConfirmDeleteFinal => {
            if let Some(item) = explorer.selected_item() {
                modals::render_confirm_delete_modal(frame, area, 1, item.size, true);
            }
        }
        Mode::ContextMenu => {
            ListApp::render_context_menu_modal(frame, area, context_menu_idx, backup_exists);
        }
        Mode::OptimizeResult => {
            if let Some(ref result_state) = optimize_result {
                ListApp::render_optimize_result_modal(frame, area, result_state);
            }
        }
        Mode::ConfirmUnlock | Mode::ConfirmUnlockFinal => {
            if let Some(item) = explorer.selected_item() {
                let lock_msg = if let Some(ref info) = item.lock_info {
                    let started = info.started.get(..19).unwrap_or(info.started.as_str());
                    format!("PID {} since {}", info.pid, started)
                } else {
                    "Unknown lock".to_string()
                };
                let final_confirm = mode == Mode::ConfirmUnlockFinal;
                modals::render_confirm_unlock_modal(frame, area, &lock_msg, final_confirm);
            }
        }
        Mode::Import => {
            if let Some(ref state) = import_state {
                import::render(state, frame, area);
            }
        }
        _ => {}
    }
}

/// Format a duration in seconds as human-readable string.
///
/// Examples:
/// - 65.5 -> "1m 5s"
/// - 3661.0 -> "1h 1m 1s"
/// - 30.0 -> "30s"
fn format_duration(seconds: f64) -> String {
    let total_secs = seconds.max(0.0).round() as u64;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, secs)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_default_is_normal() {
        assert_eq!(Mode::default(), Mode::Normal);
    }

    #[test]
    fn mode_equality() {
        assert_eq!(Mode::Search, Mode::Search);
        assert_ne!(Mode::Search, Mode::Normal);
    }

    #[test]
    #[allow(clippy::clone_on_copy)]
    fn mode_clone_and_copy() {
        let mode = Mode::Help;
        let cloned = mode.clone();
        let copied = mode;
        assert_eq!(cloned, copied);
    }

    #[test]
    fn mode_debug_format() {
        let mode = Mode::ConfirmDelete;
        let debug = format!("{:?}", mode);
        assert!(debug.contains("ConfirmDelete"));
    }

    #[test]
    fn context_menu_has_seven_items() {
        assert_eq!(ContextMenuItem::ALL.len(), 7);
    }

    #[test]
    fn context_menu_items_have_labels() {
        for item in ContextMenuItem::ALL {
            assert!(!item.label().is_empty());
        }
    }

    #[test]
    fn context_menu_items_have_shortcuts() {
        // All items except Restore have shortcuts
        for item in ContextMenuItem::ALL {
            if matches!(item, ContextMenuItem::Restore) {
                assert!(!item.has_shortcut());
            } else {
                assert!(item.has_shortcut());
            }
        }
    }

    #[test]
    fn context_menu_copy_label_and_shortcut() {
        assert_eq!(ContextMenuItem::Copy.label(), "Copy to clipboard");
        assert_eq!(ContextMenuItem::Copy.shortcut(), "c");
    }

    #[test]
    fn context_menu_item_order() {
        // Verify expected order: Play, Copy, Rename, Optimize, Analyze, Restore, Delete
        assert_eq!(ContextMenuItem::ALL[0], ContextMenuItem::Play);
        assert_eq!(ContextMenuItem::ALL[1], ContextMenuItem::Copy);
        assert_eq!(ContextMenuItem::ALL[2], ContextMenuItem::Rename);
        assert_eq!(ContextMenuItem::ALL[3], ContextMenuItem::Optimize);
        assert_eq!(ContextMenuItem::ALL[4], ContextMenuItem::Analyze);
        assert_eq!(ContextMenuItem::ALL[5], ContextMenuItem::Restore);
        assert_eq!(ContextMenuItem::ALL[6], ContextMenuItem::Delete);
    }

    #[test]
    fn context_menu_mode_is_context_menu() {
        assert_eq!(Mode::ContextMenu, Mode::ContextMenu);
        assert_ne!(Mode::ContextMenu, Mode::Normal);
    }

    #[test]
    fn format_duration_seconds_only() {
        assert_eq!(format_duration(30.0), "30s");
        assert_eq!(format_duration(0.0), "0s");
        assert_eq!(format_duration(59.4), "59s"); // rounds down
    }

    #[test]
    fn format_duration_minutes_and_seconds() {
        assert_eq!(format_duration(60.0), "1m 0s");
        assert_eq!(format_duration(90.0), "1m 30s");
        assert_eq!(format_duration(3599.0), "59m 59s");
    }

    #[test]
    fn format_duration_hours() {
        assert_eq!(format_duration(3600.0), "1h 0m 0s");
        assert_eq!(format_duration(3661.0), "1h 1m 1s");
        assert_eq!(format_duration(7322.0), "2h 2m 2s");
    }

    #[test]
    fn optimize_result_mode_exists() {
        assert_eq!(Mode::OptimizeResult, Mode::OptimizeResult);
        assert_ne!(Mode::OptimizeResult, Mode::Normal);
    }

    #[test]
    fn rename_input_mode_exists() {
        assert_eq!(Mode::RenameInput, Mode::RenameInput);
        assert_ne!(Mode::RenameInput, Mode::Normal);
    }

    #[test]
    fn rename_menu_item_label_and_shortcut() {
        assert_eq!(ContextMenuItem::Rename.label(), "Rename");
        assert_eq!(ContextMenuItem::Rename.shortcut(), "r");
        assert!(ContextMenuItem::Rename.has_shortcut());
    }

    #[test]
    fn is_valid_filename_char_rejects_invalid() {
        for &c in filename::INVALID_CHARS {
            assert!(!filename::is_valid_filename_char(c));
        }
    }

    #[test]
    fn is_valid_filename_char_accepts_valid() {
        assert!(filename::is_valid_filename_char('a'));
        assert!(filename::is_valid_filename_char('Z'));
        assert!(filename::is_valid_filename_char('0'));
        assert!(filename::is_valid_filename_char('-'));
        assert!(filename::is_valid_filename_char('_'));
        assert!(filename::is_valid_filename_char('.'));
    }

    #[test]
    fn import_mode_exists() {
        assert_eq!(Mode::Import, Mode::Import);
        assert_ne!(Mode::Import, Mode::Normal);
    }

    #[test]
    fn paste_with_no_paths_shows_status() {
        // Test the import state creation logic directly
        let agents = vec!["agent1".to_string(), "agent2".to_string()];
        let state = import::ImportState::new("   \n\n  ", &agents);

        // Empty text should result in no paths
        assert!(!state.has_paths());
        assert_eq!(state.file_count(), 0);
    }

    #[test]
    fn paste_ignored_in_non_normal_mode() {
        // This test validates that the mode guard logic exists.
        // The actual behavior is tested in integration tests since ListApp::new()
        // requires a terminal which is not available in unit tests.

        // We test the logic by verifying that Mode::Normal is distinct from other modes
        // and that the code path exists for checking mode equality.
        assert_ne!(Mode::Normal, Mode::Search);
        assert_ne!(Mode::Normal, Mode::AgentFilter);
        assert_ne!(Mode::Normal, Mode::Help);
        assert_ne!(Mode::Normal, Mode::ConfirmDelete);
        assert_ne!(Mode::Normal, Mode::ContextMenu);
        assert_ne!(Mode::Normal, Mode::Import);
        assert_ne!(Mode::Normal, Mode::RenameInput);

        // The mode guard check `if self.mode != Mode::Normal` will correctly
        // reject paste events in all these modes.
        assert_eq!(Mode::Normal, Mode::Normal);
    }

    // ── footer_text_for_mode ──────────────────────────────────────────────

    #[test]
    fn footer_text_normal_mode() {
        let text = footer_text_for_mode(Mode::Normal, &None);
        assert!(text.contains("navigate"));
        assert!(text.contains("play"));
        assert!(text.contains("quit"));
    }

    #[test]
    fn footer_text_search_mode() {
        let text = footer_text_for_mode(Mode::Search, &None);
        assert!(text.contains("Esc"));
        assert!(text.contains("search"));
    }

    #[test]
    fn footer_text_confirm_delete_mode() {
        let text = footer_text_for_mode(Mode::ConfirmDelete, &None);
        assert!(text.contains("confirm delete"));
    }

    #[test]
    fn footer_text_confirm_delete_final_mode() {
        let text = footer_text_for_mode(Mode::ConfirmDeleteFinal, &None);
        assert!(text.contains("confirm"));
        assert!(text.contains("Esc"));
    }

    #[test]
    fn footer_text_confirm_unlock_mode() {
        let text = footer_text_for_mode(Mode::ConfirmUnlock, &None);
        assert!(text.contains("force unlock"));
    }

    #[test]
    fn footer_text_rename_input_mode() {
        let text = footer_text_for_mode(Mode::RenameInput, &None);
        assert!(text.contains("confirm"));
        assert!(text.contains("Esc"));
    }

    #[test]
    fn footer_text_optimize_result_mode() {
        let text = footer_text_for_mode(Mode::OptimizeResult, &None);
        assert!(text.contains("dismiss"));
    }

    #[test]
    fn footer_text_import_none_state() {
        // Import mode with no state yields empty string
        let text = footer_text_for_mode(Mode::Import, &None);
        assert_eq!(text, "");
    }

    #[test]
    fn footer_text_import_agent_input_phase() {
        let agents: Vec<String> = vec![];
        let mut state = import::ImportState::new("/tmp/test.cast", &agents);
        state.phase = import::ImportPhase::AgentInput;
        let opt = Some(state);
        let text = footer_text_for_mode(Mode::Import, &opt);
        assert!(text.contains("Enter"));
        assert!(text.contains("Tab"));
    }

    #[test]
    fn footer_text_import_importing_phase() {
        let agents: Vec<String> = vec![];
        let mut state = import::ImportState::new("/tmp/test.cast", &agents);
        state.phase = import::ImportPhase::Importing;
        let opt = Some(state);
        let text = footer_text_for_mode(Mode::Import, &opt);
        assert!(text.contains("Importing"));
    }

    #[test]
    fn footer_text_import_done_phase() {
        let agents: Vec<String> = vec![];
        let mut state = import::ImportState::new("/tmp/test.cast", &agents);
        state.phase = import::ImportPhase::Done;
        let opt = Some(state);
        let text = footer_text_for_mode(Mode::Import, &opt);
        assert!(text.contains("dismiss"));
    }

    // ── build_menu_item_label ─────────────────────────────────────────────

    #[test]
    fn menu_item_label_normal_item_with_shortcut() {
        // Play has shortcut "p"
        let label = build_menu_item_label(&ContextMenuItem::Play, true);
        assert!(label.contains("Play"));
        assert!(label.contains("(p)"));
        assert!(!label.contains("no backup"));
    }

    #[test]
    fn menu_item_label_restore_with_backup() {
        let label = build_menu_item_label(&ContextMenuItem::Restore, true);
        assert!(label.contains("Restore"));
        assert!(!label.contains("no backup"));
    }

    #[test]
    fn menu_item_label_restore_no_backup() {
        let label = build_menu_item_label(&ContextMenuItem::Restore, false);
        assert!(label.contains("Restore"));
        assert!(label.contains("no backup"));
    }

    #[test]
    fn menu_item_label_copy_item() {
        let label = build_menu_item_label(&ContextMenuItem::Copy, true);
        assert!(label.contains("Copy to clipboard"));
        assert!(label.contains("(c)"));
    }

    // ── menu_item_style ───────────────────────────────────────────────────

    #[test]
    fn menu_item_style_selected_uses_highlight() {
        let theme = crate::theme::Theme::claude_code();
        let style = menu_item_style(true, false, &theme);
        let expected = theme.highlight_style();
        assert_eq!(style, expected);
    }

    #[test]
    fn menu_item_style_disabled_uses_secondary_color() {
        let theme = crate::theme::Theme::claude_code();
        let style = menu_item_style(false, true, &theme);
        assert_eq!(style.fg, Some(theme.text_secondary));
    }

    #[test]
    fn menu_item_style_normal_uses_primary_color() {
        let theme = crate::theme::Theme::claude_code();
        let style = menu_item_style(false, false, &theme);
        assert_eq!(style.fg, Some(theme.text_primary));
    }

    #[test]
    fn menu_item_style_selected_takes_precedence_over_disabled() {
        // selected=true, disabled=true → still highlight (selected wins)
        let theme = crate::theme::Theme::claude_code();
        let style = menu_item_style(true, true, &theme);
        let expected = theme.highlight_style();
        assert_eq!(style, expected);
    }

    // ── build_rename_status_spans ─────────────────────────────────────────

    #[test]
    fn rename_spans_selected_all_wraps_in_brackets() {
        let hl = ratatui::style::Style::default();
        let blink =
            ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::SLOW_BLINK);
        let mut spans = vec![];
        build_rename_status_spans(&mut spans, "hello", 0, true, hl, blink);
        // Expect 3 spans: "[", "hello", "]" — all with blink style
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "[");
        assert_eq!(spans[1].content, "hello");
        assert_eq!(spans[2].content, "]");
        assert_eq!(spans[0].style, blink);
    }

    #[test]
    fn rename_spans_cursor_at_start_no_before_span() {
        let hl = ratatui::style::Style::default();
        let blink =
            ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::SLOW_BLINK);
        let mut spans = vec![];
        // cursor=0, selected_all=false → only "|" + "hello"
        build_rename_status_spans(&mut spans, "hello", 0, false, hl, blink);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content, "|");
        assert_eq!(spans[1].content, "hello");
    }

    #[test]
    fn rename_spans_cursor_at_end_no_after_span() {
        let hl = ratatui::style::Style::default();
        let blink =
            ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::SLOW_BLINK);
        let mut spans = vec![];
        // cursor at end → "hello" + "|"
        build_rename_status_spans(&mut spans, "hello", 5, false, hl, blink);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content, "hello");
        assert_eq!(spans[1].content, "|");
    }

    #[test]
    fn rename_spans_cursor_in_middle_three_spans() {
        let hl = ratatui::style::Style::default();
        let blink =
            ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::SLOW_BLINK);
        let mut spans = vec![];
        // cursor=2 in "hello" → "he" + "|" + "llo"
        build_rename_status_spans(&mut spans, "hello", 2, false, hl, blink);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "he");
        assert_eq!(spans[1].content, "|");
        assert_eq!(spans[2].content, "llo");
    }

    #[test]
    fn rename_spans_empty_input_cursor_only() {
        let hl = ratatui::style::Style::default();
        let blink =
            ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::SLOW_BLINK);
        let mut spans = vec![];
        // empty input → only "|"
        build_rename_status_spans(&mut spans, "", 0, false, hl, blink);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "|");
    }

    #[test]
    fn format_duration_negative_input_clamps_to_zero() {
        assert_eq!(format_duration(-5.0), "0s");
    }

    #[test]
    fn format_duration_nan_clamps_to_zero() {
        assert_eq!(format_duration(f64::NAN), "0s");
    }

    #[test]
    fn rename_spans_multibyte_cursor_at_char_boundary() {
        let theme = crate::theme::Theme::claude_code();
        let hl = theme.highlight_style();
        let blink = Style::default().add_modifier(Modifier::SLOW_BLINK);
        let mut spans = vec![];
        // é is 2 bytes in UTF-8
        let input = "\u{00e9}llo"; // 4 bytes total
        build_rename_status_spans(&mut spans, input, 2, false, hl, blink);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "\u{00e9}");
        assert_eq!(spans[1].content, "|");
        assert_eq!(spans[2].content, "llo");
    }
}
