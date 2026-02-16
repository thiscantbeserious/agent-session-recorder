//! List command TUI application
//!
//! Interactive file explorer for browsing and managing session recordings.
//! Features: search, agent filter, play, copy, rename, optimize, analyze, restore, delete.

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
use super::app::modals;
use super::app::status_footer::{render_footer_text, render_input_line, render_status_line};
use super::app::{handle_shared_key, App, KeyResult, SharedMode, SharedState, TuiApp};
use super::widgets::preview::prefetch_adjacent_previews;
use super::widgets::FileItem;
use crate::asciicast::{apply_transforms, TransformResult};
use crate::config::Config;
use crate::files::backup::{backup_path_for, create_backup, has_backup, restore_from_backup};
use crate::files::filename;
use crate::files::lock;
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
            | Mode::RenameInput => None,
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

    /// Handle keys in normal mode (app-specific only).
    ///
    /// Navigation (up/down/pgup/pgdn/home/end) and mode transitions
    /// (`/`, `f`, `?`) are handled by `handle_shared_key`. This only
    /// handles app-specific keys: Enter, shortcuts, and Esc.
    fn handle_normal_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            // Actions
            KeyCode::Enter => {
                if self.is_selected_locked() {
                    self.mode = Mode::ConfirmUnlock;
                    return Ok(());
                }
                if self.shared.explorer.selected_item().is_some() {
                    self.context_menu_idx = 0;
                    self.mode = Mode::ContextMenu;
                }
            }

            // Direct shortcuts (bypass context menu)
            KeyCode::Char('p') => {
                if self.is_selected_locked() {
                    self.mode = Mode::ConfirmUnlock;
                    return Ok(());
                }
                self.play_session()?;
            }
            KeyCode::Char('c') => self.copy_to_clipboard()?,
            KeyCode::Char('t') => {
                if self.is_selected_locked() {
                    self.mode = Mode::ConfirmUnlock;
                    return Ok(());
                }
                self.optimize_session()?;
            }
            KeyCode::Char('a') => {
                if self.is_selected_locked() {
                    self.mode = Mode::ConfirmUnlock;
                    return Ok(());
                }
                self.analyze_session()?;
            }
            KeyCode::Char('r') => {
                if self.is_selected_locked() {
                    self.mode = Mode::ConfirmUnlock;
                    return Ok(());
                }
                self.enter_rename_mode();
            }
            KeyCode::Char('d') => {
                if self.is_selected_locked() {
                    self.mode = Mode::ConfirmUnlock;
                    return Ok(());
                }
                if self.shared.explorer.selected_item().is_some() {
                    self.mode = Mode::ConfirmDelete;
                }
            }
            // Clear filters
            KeyCode::Esc => {
                self.shared.explorer.clear_filters();
                self.shared.search_input.clear();
                self.shared.agent_filter_idx = 0;
            }

            // Quit
            KeyCode::Char('q') => self.app.quit(),

            _ => {}
        }
        Ok(())
    }

    /// Handle keys in confirm delete mode (first confirmation).
    fn handle_confirm_delete_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.mode = Mode::ConfirmDeleteFinal;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        Ok(())
    }

    /// Handle keys in final delete confirmation ("Are you sure?").
    fn handle_confirm_delete_final_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.delete_session()?;
                self.mode = Mode::Normal;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            _ => {}
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
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.mode = Mode::ConfirmUnlockFinal;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            _ => {}
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
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.remove_selected_lock();
                self.mode = Mode::Normal;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            _ => {}
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
            KeyCode::Backspace => {
                if self.rename_selected_all {
                    self.rename_input.clear();
                    self.rename_cursor = 0;
                    self.rename_selected_all = false;
                } else if self.rename_cursor > 0 {
                    // Find previous char boundary
                    let prev = self.rename_input[..self.rename_cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.rename_input.remove(prev);
                    self.rename_cursor = prev;
                }
            }
            KeyCode::Delete => {
                if self.rename_selected_all {
                    self.rename_input.clear();
                    self.rename_cursor = 0;
                    self.rename_selected_all = false;
                } else if self.rename_cursor < self.rename_input.len() {
                    self.rename_input.remove(self.rename_cursor);
                    // cursor stays — next char slides into position
                }
            }
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
                if !filename::is_valid_filename_char(c) {
                    return Ok(());
                }
                // Compute max stem length: MAX_FILENAME_LENGTH minus extension length
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
                    self.rename_input.clear();
                    self.rename_input.push(c);
                    self.rename_cursor = c.len_utf8();
                    self.rename_selected_all = false;
                } else if self.rename_input.len() < max_stem_len {
                    self.rename_input.insert(self.rename_cursor, c);
                    self.rename_cursor += c.len_utf8();
                }
            }
            _ => {}
        }
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
        let theme = current_theme();

        // Center the modal
        let modal_width = 60.min(area.width.saturating_sub(4));
        let modal_height = 31.min(area.height.saturating_sub(2));
        let x = (area.width - modal_width) / 2;
        let y = (area.height - modal_height) / 2;
        let modal_area = Rect::new(x, y, modal_width, modal_height);

        // Clear the area behind the modal
        frame.render_widget(Clear, modal_area);

        let help_text = vec![
            Line::from(Span::styled(
                "Keyboard Shortcuts",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            // Navigation section
            Line::from(Span::styled(
                "Navigation",
                Style::default().fg(theme.text_secondary),
            )),
            Line::from(vec![
                Span::styled("  ↑/↓ j/k", Style::default().fg(theme.accent)),
                Span::raw("    Navigate"),
            ]),
            Line::from(vec![
                Span::styled("  PgUp/Dn", Style::default().fg(theme.accent)),
                Span::raw("    Page up/down"),
            ]),
            Line::from(vec![
                Span::styled("  Home/End", Style::default().fg(theme.accent)),
                Span::raw("   First/last"),
            ]),
            Line::from(""),
            // Actions section
            Line::from(Span::styled(
                "Actions",
                Style::default().fg(theme.text_secondary),
            )),
            Line::from(vec![
                Span::styled("  Enter", Style::default().fg(theme.accent)),
                Span::raw("       Context menu"),
            ]),
            Line::from(vec![
                Span::styled("  p", Style::default().fg(theme.accent)),
                Span::raw("           Play session"),
            ]),
            Line::from(vec![
                Span::styled("  c", Style::default().fg(theme.accent)),
                Span::raw("           Copy to clipboard"),
            ]),
            Line::from(vec![
                Span::styled("  r", Style::default().fg(theme.accent)),
                Span::raw("           Rename session"),
            ]),
            Line::from(vec![
                Span::styled("  t", Style::default().fg(theme.accent)),
                Span::raw("           Optimize (removes silence)"),
            ]),
            Line::from(vec![
                Span::styled("  a", Style::default().fg(theme.accent)),
                Span::raw("           Analyze session"),
            ]),
            Line::from(vec![
                Span::styled("  d", Style::default().fg(theme.accent)),
                Span::raw("           Delete session"),
            ]),
            Line::from(""),
            // Filter section
            Line::from(Span::styled(
                "Filtering",
                Style::default().fg(theme.text_secondary),
            )),
            Line::from(vec![
                Span::styled("  /", Style::default().fg(theme.accent)),
                Span::raw("           Search by filename"),
            ]),
            Line::from(vec![
                Span::styled("  f", Style::default().fg(theme.accent)),
                Span::raw("           Filter by agent"),
            ]),
            Line::from(vec![
                Span::styled("  Esc", Style::default().fg(theme.accent)),
                Span::raw("         Clear filters"),
            ]),
            Line::from(""),
            // Other section
            Line::from(vec![
                Span::styled("  ?", Style::default().fg(theme.accent)),
                Span::raw("           This help"),
            ]),
            Line::from(vec![
                Span::styled("  q", Style::default().fg(theme.accent)),
                Span::raw("           Quit"),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Press any key to close",
                Style::default().fg(theme.text_secondary),
            )),
        ];

        let help = Paragraph::new(help_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.accent))
                    .title(" Help "),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(help, modal_area);
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

            // Build the label with shortcut hint
            let label = if is_restore && !backup_exists {
                if item.has_shortcut() {
                    format!("  {} ({}) - no backup", item.label(), item.shortcut())
                } else {
                    format!("  {} - no backup", item.label())
                }
            } else if item.has_shortcut() {
                format!("  {} ({})", item.label(), item.shortcut())
            } else {
                format!("  {}", item.label())
            };

            let style = if is_selected {
                theme.highlight_style()
            } else if is_disabled {
                Style::default().fg(theme.text_secondary)
            } else {
                Style::default().fg(theme.text_primary)
            };

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
            Mode::Normal => {
                match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        self.shared.explorer.up();
                    }
                    MouseEventKind::ScrollDown => {
                        self.shared.explorer.down();
                    }
                    MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                        // Click selects the item and opens the context menu
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
            }
            Mode::ContextMenu => {
                match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        if self.context_menu_idx > 0 {
                            self.context_menu_idx -= 1;
                        } else {
                            self.context_menu_idx = ContextMenuItem::ALL.len() - 1;
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        self.context_menu_idx =
                            (self.context_menu_idx + 1) % ContextMenuItem::ALL.len();
                    }
                    MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                        // Map click to menu item. Modal is centered:
                        // modal_height = items + 4 (border + title + blank + footer)
                        // Items start at row y+3 (border=1, title=1, blank=1)
                        let modal_width = 40.min(width.saturating_sub(4));
                        let modal_height =
                            (ContextMenuItem::ALL.len() as u16 + 4).min(height.saturating_sub(4));
                        let modal_x = (width - modal_width) / 2;
                        let modal_y = (height - modal_height) / 2;
                        let items_start_y = modal_y + 3; // border + title + blank

                        let cx = mouse.column;
                        let cy = mouse.row;

                        if cx >= modal_x
                            && cx < modal_x + modal_width
                            && cy >= items_start_y
                            && cy < items_start_y + ContextMenuItem::ALL.len() as u16
                        {
                            let idx = (cy - items_start_y) as usize;
                            self.context_menu_idx = idx;
                            self.execute_context_menu_action()?;
                        } else {
                            // Click outside modal closes it
                            self.mode = Mode::Normal;
                        }
                    }
                    _ => {}
                }
            }
            Mode::ConfirmDelete | Mode::ConfirmUnlock => {
                let modal_w = if self.mode == Mode::ConfirmDelete {
                    50
                } else {
                    55
                };
                // 8-tall modal: button row at offset 6 (border + 5 content lines)
                match modals::handle_confirm_click(&mouse, width, height, modal_w, 8, 6) {
                    modals::ConfirmClick::Yes => {
                        self.mode = if self.mode == Mode::ConfirmDelete {
                            Mode::ConfirmDeleteFinal
                        } else {
                            Mode::ConfirmUnlockFinal
                        };
                    }
                    modals::ConfirmClick::No => self.mode = Mode::Normal,
                    modals::ConfirmClick::Ignored => {}
                }
            }
            Mode::ConfirmDeleteFinal | Mode::ConfirmUnlockFinal => {
                let modal_w = if self.mode == Mode::ConfirmDeleteFinal {
                    50
                } else {
                    55
                };
                match modals::handle_confirm_click(&mouse, width, height, modal_w, 8, 6) {
                    modals::ConfirmClick::Yes => {
                        if self.mode == Mode::ConfirmDeleteFinal {
                            self.delete_session()?;
                        } else {
                            self.remove_selected_lock();
                        }
                        self.mode = Mode::Normal;
                    }
                    modals::ConfirmClick::No => self.mode = Mode::Normal,
                    modals::ConfirmClick::Ignored => {}
                }
            }
            // Other modal modes: click outside dismisses
            _ => {
                if let MouseEventKind::Down(crossterm::event::MouseButton::Left) = mouse.kind {
                    self.mode = Mode::Normal;
                    self.optimize_result = None;
                }
            }
        }

        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        // Try shared key handling first (navigation, search, agent filter, help)
        if let Some(shared_mode) = self.mode.to_shared() {
            match handle_shared_key(&shared_mode, key, &mut self.shared) {
                KeyResult::Consumed => return Ok(()),
                KeyResult::EnterMode(m) => {
                    self.mode = Mode::from_shared(m);
                    return Ok(());
                }
                KeyResult::NotConsumed => {}
            }
        }

        // Handle app-specific modes
        match self.mode {
            Mode::Normal => self.handle_normal_key(key)?,
            Mode::ConfirmDelete => self.handle_confirm_delete_key(key)?,
            Mode::ConfirmDeleteFinal => self.handle_confirm_delete_final_key(key)?,
            Mode::ContextMenu => self.handle_context_menu_key(key)?,
            Mode::OptimizeResult => self.handle_optimize_result_key(key)?,
            Mode::ConfirmUnlock => self.handle_confirm_unlock_key(key)?,
            Mode::ConfirmUnlockFinal => self.handle_confirm_unlock_final_key(key)?,
            Mode::RenameInput => self.handle_rename_input_key(key)?,
            _ => {}
        }
        Ok(())
    }

    fn draw(&mut self) -> Result<()> {
        // Get terminal size for page calculations
        let (_, height) = self.app.size()?;
        self.shared
            .explorer
            .set_page_size((height.saturating_sub(6)) as usize);

        // Poll cache for completed loads and request prefetch
        self.shared.preview_cache.poll();
        prefetch_adjacent_previews(&self.shared.explorer, &mut self.shared.preview_cache);

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
                render_explorer_list_with_rename(frame, chunks[0], explorer, preview, false, backup_exists, Some((rename_input, rename_cursor, rename_selected_all)));
            } else {
                render_explorer_list(frame, chunks[0], explorer, preview, false, backup_exists);
            }

            // Render status line — input modes highlight only the input
            // value and always take priority over status_message.
            match mode {
                Mode::Search => {
                    let value = format!("{}_", search_input);
                    render_input_line(frame, chunks[1], "Search: ", &value);
                }
                Mode::AgentFilter => {
                    let agent = &available_agents[agent_filter_idx];
                    render_input_line(
                        frame,
                        chunks[1],
                        "Filter by agent: ",
                        &format!("{} (←/→ to change, Enter to apply)", agent),
                    );
                }
                Mode::RenameInput => {
                    let theme = current_theme();
                    let hl = theme.highlight_style();
                    let blink = hl.add_modifier(Modifier::SLOW_BLINK);
                    let mut spans = vec![
                        Span::styled("Rename: ", Style::default().fg(theme.text_secondary)),
                    ];
                    if rename_selected_all {
                        spans.push(Span::styled("[", blink));
                        spans.push(Span::styled(rename_input.as_str(), blink));
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
                    frame.render_widget(Paragraph::new(Line::from(spans)), chunks[1]);
                }
                Mode::ConfirmDelete => {
                    render_input_line(frame, chunks[1], "🗑  Delete? ", &format!("(y/n) — {}", selected_name));
                }
                Mode::ConfirmUnlock => {
                    render_input_line(frame, chunks[1], "🔓 Force unlock? ", &format!("(y/n) — {}", selected_name));
                }
                Mode::ConfirmDeleteFinal => {
                    render_input_line(frame, chunks[1], "🗑  Are you sure? ", &format!("(y/n) — {}", selected_name));
                }
                Mode::ConfirmUnlockFinal => {
                    render_input_line(frame, chunks[1], "🔓 Are you sure? ", &format!("(y/n) — {}", selected_name));
                }
                Mode::ContextMenu | Mode::OptimizeResult => {
                    render_status_line(frame, chunks[1], &selected_name);
                }
                _ => {
                    let text = if let Some(msg) = &status {
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
                    render_status_line(frame, chunks[1], &text);
                }
            }

            // Render footer with keybindings
            let footer_text = match mode {
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
                Mode::Normal => {
                    "↑↓: navigate | Enter: menu | p: play | c: copy | r: rename | t: optimize | a: analyze | d: delete | ?: help | q: quit"
                }
            };
            render_footer_text(frame, chunks[2], footer_text);

            // Render modal overlays
            match mode {
                Mode::Help => Self::render_help_modal(frame, area),
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
                    Self::render_context_menu_modal(frame, area, context_menu_idx, backup_exists);
                }
                Mode::OptimizeResult => {
                    if let Some(ref result_state) = optimize_result {
                        Self::render_optimize_result_modal(frame, area, result_state);
                    }
                }
                Mode::ConfirmUnlock | Mode::ConfirmUnlockFinal => {
                    if let Some(item) = explorer.selected_item() {
                        let lock_msg = if let Some(ref info) = item.lock_info {
                            format!("PID {} since {}", info.pid, &info.started[..19])
                        } else {
                            "Unknown lock".to_string()
                        };
                        let final_confirm = mode == Mode::ConfirmUnlockFinal;
                        modals::render_confirm_unlock_modal(frame, area, &lock_msg, final_confirm);
                    }
                }
                _ => {}
            }
        })?;

        Ok(())
    }
}

/// Format a duration in seconds as human-readable string.
///
/// Examples:
/// - 65.5 -> "1m 5s"
/// - 3661.0 -> "1h 1m 1s"
/// - 30.0 -> "30s"
fn format_duration(seconds: f64) -> String {
    let total_secs = seconds.round() as u64;
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
}
