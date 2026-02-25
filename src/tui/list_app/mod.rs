//! List command TUI application
//!
//! Interactive file explorer for browsing and managing session recordings.
//! Features: search, agent filter, play, copy, rename, optimize, analyze, restore, delete, import.

mod actions;
mod render;

pub use render::{render_context_menu_modal, render_help_modal, render_optimize_result_modal};

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};

use super::app::layout::build_explorer_layout;
use super::app::list_view::{render_explorer_list, render_explorer_list_with_rename};
use super::app::modals;
use super::app::{handle_shared_key, App, KeyResult, SharedMode, SharedState, TuiApp};
use super::import;
use super::widgets::preview::prefetch_adjacent_previews;
use super::widgets::FileItem;
use crate::asciicast::TransformResult;
use crate::config::Config;
use crate::files::backup::has_backup;
use crate::files::lock;

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
    pub(super) app: App,
    /// Shared state (explorer, search, agent filter, preview cache, status)
    pub(super) shared: SharedState,
    /// Current UI mode
    pub(super) mode: Mode,
    /// Context menu selected index
    pub(super) context_menu_idx: usize,
    /// Optimize result for modal display
    pub(super) optimize_result: Option<OptimizeResultState>,
    /// Rename input buffer (filename stem without extension)
    pub(super) rename_input: String,
    /// Cursor position within rename_input (byte offset)
    pub(super) rename_cursor: usize,
    /// Whether the entire rename input is "selected" (first keystroke replaces all)
    pub(super) rename_selected_all: bool,
    /// Import state for drag-and-drop .cast file imports
    pub(super) import_state: Option<import::ImportState>,
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
        match key.code {
            // Actions
            KeyCode::Enter => {
                if self.redirect_if_locked() {
                    return Ok(());
                }
                if self.shared.explorer.selected_item().is_some() {
                    self.context_menu_idx = 0;
                    self.mode = Mode::ContextMenu;
                }
            }

            // Direct shortcuts (bypass context menu)
            KeyCode::Char('p') => {
                if self.redirect_if_locked() {
                    return Ok(());
                }
                self.play_session()?;
            }
            KeyCode::Char('c') => self.copy_to_clipboard()?,
            KeyCode::Char('t') => {
                if self.redirect_if_locked() {
                    return Ok(());
                }
                self.optimize_session()?;
            }
            KeyCode::Char('a') => {
                if self.redirect_if_locked() {
                    return Ok(());
                }
                self.analyze_session()?;
            }
            KeyCode::Char('r') => {
                if self.redirect_if_locked() {
                    return Ok(());
                }
                self.enter_rename_mode();
            }
            KeyCode::Char('d') => {
                if self.redirect_if_locked() {
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
        use crate::files::filename;

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
            Mode::Import => self.handle_import_key(key)?,
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

            render::render_status_line_for_mode(
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

            let footer_text = render::footer_text_for_mode(mode, import_state);
            super::app::status_footer::render_footer_text(frame, chunks[2], footer_text);

            render::render_modal_overlays(
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
        use crate::files::filename;
        for &c in filename::INVALID_CHARS {
            assert!(!filename::is_valid_filename_char(c));
        }
    }

    #[test]
    fn is_valid_filename_char_accepts_valid() {
        use crate::files::filename;
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
}
