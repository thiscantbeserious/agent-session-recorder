//! Cleanup command TUI application
//!
//! Interactive file explorer for selecting and deleting session recordings.
//! Features: multi-select, search, agent filter, glob select, storage preview.

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::{layout::Rect, text::Line, Frame};

use super::app::layout::build_explorer_layout;
use super::app::list_view::render_explorer_list;
use super::app::modals::{
    self, help_close_hint, help_section_header, help_shortcut_line, help_title_line,
    render_help_paragraph,
};
use super::app::status_footer::{render_footer_text, render_input_line, render_status_line};
use super::app::{classify_confirm_key, App, ConfirmAction, SharedMode, SharedState, TuiApp};
use super::widgets::FileItem;
use crate::config::Config;
use crate::theme::current_theme;

/// UI mode for the cleanup application
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Normal browsing mode
    #[default]
    Normal,
    /// Search mode - typing filters by filename
    Search,
    /// Agent filter mode - selecting agent to filter by
    AgentFilter,
    /// Glob select mode - enter pattern to select matching files
    GlobSelect,
    /// Help mode - showing keyboard shortcuts
    Help,
    /// Confirm delete mode
    ConfirmDelete,
    /// Second confirmation for delete - "Are you sure?"
    ConfirmDeleteFinal,
}

impl Mode {
    fn to_shared(self) -> Option<SharedMode> {
        match self {
            Mode::Normal => Some(SharedMode::Normal),
            Mode::Search => Some(SharedMode::Search),
            Mode::AgentFilter => Some(SharedMode::AgentFilter),
            Mode::Help => Some(SharedMode::Help),
            Mode::ConfirmDelete => Some(SharedMode::ConfirmDelete),
            Mode::GlobSelect | Mode::ConfirmDeleteFinal => None,
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

/// Cleanup application state
pub struct CleanupApp {
    /// Base app for terminal handling
    app: App,
    /// Shared state (explorer, search, agent filter, preview cache, status)
    shared: SharedState,
    /// Current UI mode
    mode: Mode,
    /// Glob pattern input buffer
    glob_input: String,
    /// Whether files were deleted (for success message)
    files_deleted: bool,
}

impl CleanupApp {
    /// Create a new cleanup application with the given sessions.
    pub fn new(items: Vec<FileItem>, config: Config) -> Result<Self> {
        let app = App::new(Duration::from_millis(250))?;
        let shared = SharedState::new(items, Some(config));

        Ok(Self {
            app,
            shared,
            mode: Mode::Normal,
            glob_input: String::new(),
            files_deleted: false,
        })
    }

    /// Check if any files were deleted during this session
    pub fn files_were_deleted(&self) -> bool {
        self.files_deleted
    }

    /// Handle keys in normal mode (app-specific only).
    ///
    /// Navigation (up/down/pgup/pgdn/home/end) and mode transitions
    /// (`/`, `f`, `?`) are handled by `handle_shared_key`. This only
    /// handles app-specific keys: Space, a, g, Enter, Esc, q.
    fn handle_normal_key(&mut self, key: KeyEvent) -> Result<()> {
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
    fn handle_glob_key(&mut self, key: KeyEvent) -> Result<()> {
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
            KeyCode::Char(c) => {
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                    self.glob_input.push(c);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Select items matching a glob-like pattern.
    /// Supports: * (any chars), ? (single char), agent/pattern syntax
    fn select_by_glob(&mut self, pattern: &str) -> usize {
        // Parse agent/pattern syntax (e.g., "claude/*.cast" or "*2024*")
        let (agent_filter, file_pattern) = if let Some(slash_pos) = pattern.find('/') {
            let agent = &pattern[..slash_pos];
            let pat = &pattern[slash_pos + 1..];
            (Some(agent), pat)
        } else {
            (None, pattern)
        };

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
            let matches = if let Some(agent_pat) = agent_filter {
                glob_match(&agent, agent_pat) && glob_match(&name, file_pattern)
            } else {
                glob_match(&name, file_pattern)
            };
            if matches && !is_selected {
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
                self.delete_selected()?;
                self.mode = Mode::Normal;
            }
            ConfirmAction::Cancelled => self.mode = Mode::Normal,
            ConfirmAction::Ignored => {}
        }
        Ok(())
    }

    /// Delete all selected sessions.
    fn delete_selected(&mut self) -> Result<()> {
        let selected_items = self.shared.explorer.selected_items();
        if selected_items.is_empty() {
            return Ok(());
        }

        // Collect paths to delete
        let paths: Vec<String> = selected_items.iter().map(|i| i.path.clone()).collect();
        let count = paths.len();

        // Delete files
        let mut deleted = 0;
        let mut total_freed: u64 = 0;
        for path in &paths {
            if let Ok(metadata) = std::fs::metadata(path) {
                total_freed += metadata.len();
            }
            if std::fs::remove_file(path).is_ok() {
                deleted += 1;
            }
        }

        // Remove from explorer
        for path in &paths {
            self.shared.explorer.remove_item(path);
        }

        // Update status
        if deleted == count {
            self.shared.status_message = Some(format!(
                "Deleted {} sessions (freed {})",
                deleted,
                format_size(total_freed)
            ));
            self.files_deleted = true;
        } else {
            self.shared.status_message = Some(format!(
                "Deleted {}/{} sessions (some files could not be removed)",
                deleted, count
            ));
            if deleted > 0 {
                self.files_deleted = true;
            }
        }

        Ok(())
    }

    /// Render the help modal overlay.
    fn render_help_modal(frame: &mut Frame, area: Rect) {
        let theme = current_theme();
        let accent = theme.accent;
        let help_text = vec![
            help_title_line("Cleanup Keyboard Shortcuts", accent),
            Line::from(""),
            help_section_header("Navigation"),
            help_shortcut_line("  up/down, j/k", "   Move cursor", accent),
            help_shortcut_line("  PgUp/PgDn", "      Page up/down", accent),
            help_shortcut_line("  Home/End", "       Go to first/last", accent),
            Line::from(""),
            help_section_header("Selection"),
            help_shortcut_line("  Space", "          Toggle select current item", accent),
            help_shortcut_line("  a", "              Select all / Deselect all", accent),
            help_shortcut_line(
                "  g",
                "              Glob select (e.g., *2024*, claude/*.cast)",
                accent,
            ),
            Line::from(""),
            help_section_header("Filtering"),
            help_shortcut_line("  /", "              Search by filename", accent),
            help_shortcut_line("  f", "              Filter by agent", accent),
            Line::from(""),
            help_shortcut_line(
                "  Enter",
                "          Delete selected (with confirmation)",
                theme.error,
            ),
            help_shortcut_line(
                "  Esc",
                "            Clear selection / Clear filters",
                accent,
            ),
            help_shortcut_line("  q", "              Quit without deleting", accent),
            Line::from(""),
            help_close_hint(),
        ];
        render_help_paragraph(frame, area, "Help", help_text, 65, 20);
    }

    /// Render the status line for the current mode.
    ///
    /// Input modes render `render_input_line`; Normal mode renders a status
    /// summary; other modes render nothing.
    #[allow(clippy::too_many_arguments)]
    fn render_status_for_mode(
        frame: &mut Frame,
        area: Rect,
        mode: Mode,
        search_input: &str,
        glob_input: &str,
        agent: &str,
        status: &Option<String>,
        selected_count: usize,
        selected_size: u64,
        explorer_len: usize,
        search_filter: Option<&str>,
        agent_filter: Option<&str>,
    ) {
        match mode {
            Mode::Search => {
                let value = format!("{}_", search_input);
                render_input_line(frame, area, "Search: ", &value);
            }
            Mode::GlobSelect => {
                let value = format!("{}_", glob_input);
                render_input_line(frame, area, "Glob pattern: ", &value);
            }
            Mode::AgentFilter => {
                render_input_line(
                    frame,
                    area,
                    "Filter by agent: ",
                    &format!("{} (left/right to change, Enter to apply)", agent),
                );
            }
            Mode::ConfirmDelete => {
                render_input_line(frame, area, "Delete selected sessions? ", "(y/n)");
            }
            Mode::ConfirmDeleteFinal => {
                render_input_line(frame, area, "Are you sure? ", "(y/n)");
            }
            _ => {
                let text = status_line_content(
                    mode,
                    status,
                    selected_count,
                    selected_size,
                    explorer_len,
                    search_filter,
                    agent_filter,
                );
                render_status_line(frame, area, &text);
            }
        }
    }
}

impl TuiApp for CleanupApp {
    fn app(&mut self) -> &mut App {
        &mut self.app
    }

    fn shared_state(&mut self) -> &mut SharedState {
        &mut self.shared
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        match self.mode {
            Mode::Normal => {
                let (_, height) = self.app.size()?;
                match mouse.kind {
                    MouseEventKind::ScrollUp => self.shared.explorer.up(),
                    MouseEventKind::ScrollDown => self.shared.explorer.down(),
                    MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                        handle_normal_mouse_click(&mut self.shared, height, mouse.row)?;
                    }
                    _ => {}
                }
            }
            Mode::ConfirmDelete => {
                let (width, height) = self.app.size()?;
                match modals::handle_confirm_click(&mouse, width, height, 50, 8, 6) {
                    modals::ConfirmClick::Yes => {
                        self.mode = Mode::ConfirmDeleteFinal;
                    }
                    modals::ConfirmClick::No => self.mode = Mode::Normal,
                    modals::ConfirmClick::Ignored => {}
                }
            }
            Mode::ConfirmDeleteFinal => {
                let (width, height) = self.app.size()?;
                match modals::handle_confirm_click(&mouse, width, height, 50, 8, 6) {
                    modals::ConfirmClick::Yes => {
                        self.delete_selected()?;
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
                }
            }
        }
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
            Mode::GlobSelect => self.handle_glob_key(key)?,
            Mode::ConfirmDelete => self.handle_confirm_delete_key(key)?,
            Mode::ConfirmDeleteFinal => self.handle_confirm_delete_final_key(key)?,
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
        let glob_input = &self.glob_input;
        let status = self.shared.status_message.clone();
        let agent_filter_idx = self.shared.agent_filter_idx;
        let available_agents = &self.shared.available_agents;

        // Calculate selected size for status bar
        let selected_size: u64 = explorer.selected_items().iter().map(|i| i.size).sum();
        let selected_count = explorer.selected_count();
        let explorer_len = explorer.len();
        let search_filter = explorer.search_filter().map(|s| s.to_string());
        let agent_filter = explorer.agent_filter().map(|s| s.to_string());

        // Get preview for current selection from cache
        let current_path = explorer.selected_item().map(|i| i.path.clone());
        let preview = current_path
            .as_ref()
            .and_then(|p| self.shared.preview_cache.get(p));

        let agent = available_agents[agent_filter_idx].clone();

        self.app.draw(|frame| {
            let area = frame.area();

            // Main layout: explorer + status + footer
            let chunks = build_explorer_layout(area);

            // Render file explorer with checkboxes (cleanup uses multi-select)
            render_explorer_list(frame, chunks[0], explorer, preview, true, false);

            CleanupApp::render_status_for_mode(
                frame,
                chunks[1],
                mode,
                search_input,
                glob_input,
                &agent,
                &status,
                selected_count,
                selected_size,
                explorer_len,
                search_filter.as_deref(),
                agent_filter.as_deref(),
            );

            // Render footer with keybindings
            let footer_text = footer_text_for_mode(mode, selected_count);
            render_footer_text(frame, chunks[2], footer_text);

            // Render modal overlays
            match mode {
                Mode::Help => CleanupApp::render_help_modal(frame, area),
                Mode::ConfirmDelete => {
                    modals::render_confirm_delete_modal(
                        frame,
                        area,
                        selected_count,
                        selected_size,
                        false,
                    );
                }
                Mode::ConfirmDeleteFinal => {
                    modals::render_confirm_delete_modal(
                        frame,
                        area,
                        selected_count,
                        selected_size,
                        true,
                    );
                }
                _ => {}
            }
        })?;

        Ok(())
    }
}

/// Handle a left-click in Normal mode: navigate to the clicked item and toggle its selection.
///
/// Skips locked items (currently being recorded) and shows a status message instead.
fn handle_normal_mouse_click(shared: &mut SharedState, height: u16, click_row: u16) -> Result<()> {
    let explorer_height = height.saturating_sub(2);
    if click_row >= 1 && click_row < explorer_height.saturating_sub(1) {
        let item_offset = (click_row - 1) as usize;
        let scroll_offset = shared.explorer.scroll_offset();
        let visible_idx = scroll_offset + item_offset;
        if shared.explorer.select_index(visible_idx) {
            // Block selection of locked items
            if let Some(item) = shared.explorer.selected_item() {
                if item.lock_info.is_some() {
                    shared.status_message = Some("Session is locked (being recorded)".to_string());
                    return Ok(());
                }
            }
            shared.explorer.toggle_select();
        }
    }
    Ok(())
}

/// Compute the status line text for the current mode.
///
/// Returns a status message if one is set; otherwise builds a summary based on
/// selection count and active filters. Returns an empty string for all modes
/// other than `Mode::Normal`.
pub fn status_line_content(
    mode: Mode,
    status: &Option<String>,
    selected_count: usize,
    selected_size: u64,
    explorer_len: usize,
    search_filter: Option<&str>,
    agent_filter: Option<&str>,
) -> String {
    if let Some(msg) = status {
        return msg.clone();
    }

    match mode {
        Mode::Normal => {
            if selected_count > 0 {
                format!(
                    "{} selected ({}) | {} total sessions",
                    selected_count,
                    format_size(selected_size),
                    explorer_len
                )
            } else {
                build_normal_status(explorer_len, search_filter, agent_filter)
            }
        }
        _ => String::new(),
    }
}

/// Build the status line for Normal mode with no selection.
///
/// Shows active filters as context, or a default hint when none are active.
fn build_normal_status(
    explorer_len: usize,
    search_filter: Option<&str>,
    agent_filter: Option<&str>,
) -> String {
    let mut parts = vec![];
    if let Some(search) = search_filter {
        parts.push(format!("search: \"{}\"", search));
    }
    if let Some(agent) = agent_filter {
        parts.push(format!("agent: {}", agent));
    }
    if parts.is_empty() {
        format!("{} sessions | Space to select", explorer_len)
    } else {
        format!(
            "{} sessions ({}) | Space to select",
            explorer_len,
            parts.join(", ")
        )
    }
}

/// Return the footer keybinding text for the given mode.
///
/// In Normal mode, the text changes based on whether items are selected.
pub fn footer_text_for_mode(mode: Mode, selected_count: usize) -> &'static str {
    match mode {
        Mode::Search => "Esc: cancel | Enter: apply | Backspace: delete",
        Mode::GlobSelect => "Esc: cancel | Enter: select matching | Backspace: delete",
        Mode::AgentFilter => "left/right: change | Enter: apply | Esc: cancel",
        Mode::ConfirmDelete => "y: confirm delete | n/Esc: cancel",
        Mode::ConfirmDeleteFinal => "y: confirm | n/Esc: cancel",
        Mode::Help => "Press any key to close",
        Mode::Normal => {
            if selected_count > 0 {
                "Space: toggle | a: toggle all | Enter: delete selected | Esc: clear | ?: help"
            } else {
                "Space: select | a: all | g: glob | /: search | f: filter | ?: help | q: quit"
            }
        }
    }
}

/// Simple glob pattern matching.
/// Supports * (match any) and ? (match single char).
fn glob_match(text: &str, pattern: &str) -> bool {
    let text = text.to_lowercase();
    let pattern = pattern.to_lowercase();

    glob_match_recursive(&text, &pattern)
}

fn glob_match_recursive(text: &str, pattern: &str) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }

    let mut pattern_chars = pattern.chars().peekable();
    let mut text_chars = text.chars().peekable();

    while let Some(p) = pattern_chars.next() {
        match p {
            '*' => {
                // Collect remaining pattern after *
                let rest_pattern: String = pattern_chars.collect();

                // If * is at the end, match everything
                if rest_pattern.is_empty() {
                    return true;
                }

                // Try matching rest of pattern at each position
                let rest_text: String = text_chars.collect();
                for i in 0..=rest_text.len() {
                    if glob_match_recursive(&rest_text[i..], &rest_pattern) {
                        return true;
                    }
                }
                return false;
            }
            '?' => {
                // Match any single character
                if text_chars.next().is_none() {
                    return false;
                }
            }
            c => {
                // Match literal character
                match text_chars.next() {
                    Some(t) if t == c => {}
                    _ => return false,
                }
            }
        }
    }

    // Pattern exhausted, text should be exhausted too
    text_chars.next().is_none()
}

/// Format a byte size as human-readable string.
fn format_size(bytes: u64) -> String {
    humansize::format_size(bytes, humansize::BINARY)
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
        assert_ne!(Mode::GlobSelect, Mode::Search);
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
    fn glob_mode_exists() {
        let mode = Mode::GlobSelect;
        let debug = format!("{:?}", mode);
        assert!(debug.contains("GlobSelect"));
    }

    // Glob matching tests

    #[test]
    fn glob_match_exact() {
        assert!(glob_match("test.cast", "test.cast"));
        assert!(!glob_match("test.cast", "other.cast"));
    }

    #[test]
    fn glob_match_star_any() {
        assert!(glob_match("test.cast", "*"));
        assert!(glob_match("test.cast", "*.cast"));
        assert!(glob_match("test.cast", "test.*"));
        assert!(glob_match("test.cast", "*test*"));
        assert!(glob_match("session_2024_01.cast", "*2024*"));
    }

    #[test]
    fn glob_match_question_single() {
        assert!(glob_match("test.cast", "tes?.cast"));
        assert!(glob_match("test.cast", "????.cast"));
        assert!(!glob_match("test.cast", "???.cast"));
    }

    #[test]
    fn glob_match_case_insensitive() {
        assert!(glob_match("TEST.CAST", "test.cast"));
        assert!(glob_match("Test.Cast", "TEST.CAST"));
        assert!(glob_match("MyFile.cast", "*myfile*"));
    }

    #[test]
    fn glob_match_complex_patterns() {
        assert!(glob_match(
            "session_2024_01_15.cast",
            "session_????_??_??.cast"
        ));
        assert!(glob_match("claude_session.cast", "*_session.cast"));
        assert!(!glob_match("test.txt", "*.cast"));
    }
}
