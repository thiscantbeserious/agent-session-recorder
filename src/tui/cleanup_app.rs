//! Cleanup command TUI application
//!
//! Interactive file explorer for selecting and deleting session recordings.
//! Features: multi-select, search, agent filter, glob select, storage preview.

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use super::app::layout::build_explorer_layout;
use super::app::list_view::render_explorer_list;
use super::app::modals;
use super::app::status_footer::{render_footer_text, render_status_line};
use super::app::{handle_shared_key, App, KeyResult, SharedMode, SharedState, TuiApp};
use super::widgets::preview::prefetch_adjacent_previews;
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
}

impl Mode {
    fn to_shared(self) -> Option<SharedMode> {
        match self {
            Mode::Normal => Some(SharedMode::Normal),
            Mode::Search => Some(SharedMode::Search),
            Mode::AgentFilter => Some(SharedMode::AgentFilter),
            Mode::Help => Some(SharedMode::Help),
            Mode::ConfirmDelete => Some(SharedMode::ConfirmDelete),
            Mode::GlobSelect => None, // app-specific
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
            // Selection
            KeyCode::Char(' ') => {
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
        let items_to_select: Vec<(usize, String, String, bool)> = self
            .shared
            .explorer
            .visible_items()
            .map(|(vis_idx, item, is_selected)| {
                (vis_idx, item.agent.clone(), item.name.clone(), is_selected)
            })
            .collect();

        // Track original position
        let original_selected = self.shared.explorer.selected();
        let mut actual_count = 0;

        // Select matching items
        for (vis_idx, agent, name, is_selected) in items_to_select {
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

    /// Handle keys in confirm delete mode.
    fn handle_confirm_delete_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.delete_selected()?;
                self.mode = Mode::Normal;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            _ => {}
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

        // Center the modal
        let modal_width = 65.min(area.width.saturating_sub(4));
        let modal_height = 20.min(area.height.saturating_sub(4));
        let x = (area.width - modal_width) / 2;
        let y = (area.height - modal_height) / 2;
        let modal_area = Rect::new(x, y, modal_width, modal_height);

        // Clear the area behind the modal
        frame.render_widget(Clear, modal_area);

        let help_text = vec![
            Line::from(Span::styled(
                "Cleanup Keyboard Shortcuts",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Navigation",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![
                Span::styled("  up/down, j/k", Style::default().fg(theme.accent)),
                Span::raw("   Move cursor"),
            ]),
            Line::from(vec![
                Span::styled("  PgUp/PgDn", Style::default().fg(theme.accent)),
                Span::raw("      Page up/down"),
            ]),
            Line::from(vec![
                Span::styled("  Home/End", Style::default().fg(theme.accent)),
                Span::raw("       Go to first/last"),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Selection",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![
                Span::styled("  Space", Style::default().fg(theme.accent)),
                Span::raw("          Toggle select current item"),
            ]),
            Line::from(vec![
                Span::styled("  a", Style::default().fg(theme.accent)),
                Span::raw("              Select all / Deselect all"),
            ]),
            Line::from(vec![
                Span::styled("  g", Style::default().fg(theme.accent)),
                Span::raw("              Glob select (e.g., *2024*, claude/*.cast)"),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Filtering",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![
                Span::styled("  /", Style::default().fg(theme.accent)),
                Span::raw("              Search by filename"),
            ]),
            Line::from(vec![
                Span::styled("  f", Style::default().fg(theme.accent)),
                Span::raw("              Filter by agent"),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Enter", Style::default().fg(theme.error)),
                Span::raw("          Delete selected (with confirmation)"),
            ]),
            Line::from(vec![
                Span::styled("  Esc", Style::default().fg(theme.accent)),
                Span::raw("            Clear selection / Clear filters"),
            ]),
            Line::from(vec![
                Span::styled("  q", Style::default().fg(theme.accent)),
                Span::raw("              Quit without deleting"),
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
}

impl TuiApp for CleanupApp {
    fn app(&mut self) -> &mut App {
        &mut self.app
    }

    fn shared_state(&mut self) -> &mut SharedState {
        &mut self.shared
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
            Mode::GlobSelect => self.handle_glob_key(key)?,
            Mode::ConfirmDelete => self.handle_confirm_delete_key(key)?,
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
        let glob_input = &self.glob_input;
        let status = self.shared.status_message.clone();
        let agent_filter_idx = self.shared.agent_filter_idx;
        let available_agents = &self.shared.available_agents;

        // Calculate selected size for status bar
        let selected_size: u64 = explorer.selected_items().iter().map(|i| i.size).sum();
        let selected_count = explorer.selected_count();

        // Get preview for current selection from cache
        let current_path = explorer.selected_item().map(|i| i.path.clone());
        let preview = current_path
            .as_ref()
            .and_then(|p| self.shared.preview_cache.get(p));

        self.app.draw(|frame| {
            let area = frame.area();

            // Main layout: explorer + status + footer
            let chunks = build_explorer_layout(area);

            // Render file explorer with checkboxes (cleanup uses multi-select)
            render_explorer_list(frame, chunks[0], explorer, preview, true, false);

            // Render status line
            let status_text = if let Some(msg) = &status {
                msg.clone()
            } else {
                match mode {
                    Mode::Search => format!("Search: {}_", search_input),
                    Mode::GlobSelect => format!("Glob pattern: {}_", glob_input),
                    Mode::AgentFilter => {
                        let agent = &available_agents[agent_filter_idx];
                        format!(
                            "Filter by agent: {} (left/right to change, Enter to apply)",
                            agent
                        )
                    }
                    Mode::ConfirmDelete => String::new(), // Modal shows this
                    Mode::Help => String::new(),
                    Mode::Normal => {
                        // Show selection info
                        if selected_count > 0 {
                            format!(
                                "{} selected ({}) | {} total sessions",
                                selected_count,
                                format_size(selected_size),
                                explorer.len()
                            )
                        } else {
                            let mut parts = vec![];
                            if let Some(search) = explorer.search_filter() {
                                parts.push(format!("search: \"{}\"", search));
                            }
                            if let Some(agent) = explorer.agent_filter() {
                                parts.push(format!("agent: {}", agent));
                            }
                            if parts.is_empty() {
                                format!("{} sessions | Space to select", explorer.len())
                            } else {
                                format!(
                                    "{} sessions ({}) | Space to select",
                                    explorer.len(),
                                    parts.join(", ")
                                )
                            }
                        }
                    }
                }
            };
            render_status_line(frame, chunks[1], &status_text);

            // Render footer with keybindings
            let footer_text = match mode {
                Mode::Search => "Esc: cancel | Enter: apply | Backspace: delete",
                Mode::GlobSelect => "Esc: cancel | Enter: select matching | Backspace: delete",
                Mode::AgentFilter => "left/right: change | Enter: apply | Esc: cancel",
                Mode::ConfirmDelete => "y: confirm | n/Esc: cancel",
                Mode::Help => "Press any key to close",
                Mode::Normal => {
                    if selected_count > 0 {
                        "Space: toggle | a: toggle all | Enter: delete selected | Esc: clear | ?: help"
                    } else {
                        "Space: select | a: all | g: glob | /: search | f: filter | ?: help | q: quit"
                    }
                }
            };
            render_footer_text(frame, chunks[2], footer_text);

            // Render modal overlays
            match mode {
                Mode::Help => Self::render_help_modal(frame, area),
                Mode::ConfirmDelete => {
                    modals::render_confirm_delete_modal(
                        frame,
                        area,
                        selected_count,
                        selected_size,
                    );
                }
                _ => {}
            }
        })?;

        Ok(())
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
