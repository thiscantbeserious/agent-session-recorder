//! List command TUI application
//!
//! Interactive file explorer for browsing and managing session recordings.
//! Features: search, agent filter, play, delete, add marker.

use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use super::app::App;
use super::event::Event;
use super::preview_cache::PreviewCache;
use super::theme::current_theme;
use super::widgets::{FileExplorer, FileExplorerWidget, FileItem};

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
}

/// List application state
pub struct ListApp {
    /// Base app for terminal handling
    app: App,
    /// File explorer widget
    explorer: FileExplorer,
    /// Current UI mode
    mode: Mode,
    /// Search input buffer
    search_input: String,
    /// Selected agent filter index (for cycling through agents)
    agent_filter_idx: usize,
    /// Available agents (including "All")
    available_agents: Vec<String>,
    /// Status message to display
    status_message: Option<String>,
    /// Preview cache with async loading
    preview_cache: PreviewCache,
}

impl ListApp {
    /// Create a new list application with the given sessions.
    pub fn new(items: Vec<FileItem>) -> Result<Self> {
        let app = App::new(Duration::from_millis(250))?;

        // Collect unique agents and add "All" option
        let mut available_agents: Vec<String> = vec!["All".to_string()];
        let mut agents: Vec<String> = items.iter().map(|i| i.agent.clone()).collect();
        agents.sort();
        agents.dedup();
        available_agents.extend(agents);

        let explorer = FileExplorer::new(items);

        Ok(Self {
            app,
            explorer,
            mode: Mode::Normal,
            search_input: String::new(),
            agent_filter_idx: 0,
            available_agents,
            status_message: None,
            preview_cache: PreviewCache::default(),
        })
    }

    /// Set initial agent filter (for CLI argument support)
    pub fn set_agent_filter(&mut self, agent: &str) {
        // Find the agent in available_agents and set the index
        if let Some(idx) = self.available_agents.iter().position(|a| a == agent) {
            self.agent_filter_idx = idx;
            self.apply_agent_filter();
        }
    }

    /// Run the list application event loop.
    pub fn run(&mut self) -> Result<()> {
        loop {
            // Draw the UI
            self.draw()?;

            // Handle events
            match self.app.next_event()? {
                Event::Key(key) => self.handle_key(key)?,
                Event::Resize(_, _) => {
                    // Resize handled automatically by ratatui
                }
                Event::Tick => {
                    // Clear status message after some time (could add timer)
                }
                Event::Quit => break,
            }

            if self.app.should_quit() {
                break;
            }
        }

        Ok(())
    }

    /// Draw the UI.
    fn draw(&mut self) -> Result<()> {
        // Get terminal size for page calculations
        let (_, height) = self.app.size()?;
        self.explorer
            .set_page_size((height.saturating_sub(6)) as usize);

        // Poll cache for completed loads and request prefetch
        self.preview_cache.poll();
        self.prefetch_adjacent_previews();

        let explorer = &mut self.explorer;
        let mode = self.mode;
        let search_input = &self.search_input;
        let status = self.status_message.clone();
        let agent_filter_idx = self.agent_filter_idx;
        let available_agents = &self.available_agents;

        // Get preview for current selection from cache
        let current_path = explorer.selected_item().map(|i| i.path.clone());
        let preview = current_path
            .as_ref()
            .and_then(|p| self.preview_cache.get(p));

        self.app.draw(|frame| {
            let area = frame.area();

            // Main layout: explorer + footer
            let chunks = Layout::vertical([
                Constraint::Min(1),    // Explorer
                Constraint::Length(1), // Status line
                Constraint::Length(1), // Footer
            ])
            .split(area);

            // Render file explorer (no checkboxes in list view - it's single-select)
            let widget = FileExplorerWidget::new(explorer)
                .show_checkboxes(false)
                .session_preview(preview);
            frame.render_widget(widget, chunks[0]);

            // Render status line
            let theme = current_theme();
            let status_text = if let Some(msg) = &status {
                msg.clone()
            } else {
                match mode {
                    Mode::Search => format!("Search: {}_", search_input),
                    Mode::AgentFilter => {
                        let agent = &available_agents[agent_filter_idx];
                        format!("Filter by agent: {} (←/→ to change, Enter to apply)", agent)
                    }
                    Mode::ConfirmDelete => "Delete this session? (y/n)".to_string(),
                    Mode::Help => String::new(),
                    Mode::Normal => {
                        // Show current filters if any
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
                }
            };
            let status_line =
                Paragraph::new(status_text).style(Style::default().fg(theme.text_secondary));
            frame.render_widget(status_line, chunks[1]);

            // Render footer with keybindings
            let footer_text = match mode {
                Mode::Search => "Esc: cancel | Enter: apply search | Backspace: delete char",
                Mode::AgentFilter => "←/→: change agent | Enter: apply | Esc: cancel",
                Mode::ConfirmDelete => "y: confirm delete | n/Esc: cancel",
                Mode::Help => "Press any key to close help",
                Mode::Normal => {
                    "↑↓: navigate | Enter: play | /: search | f: filter | d: delete | m: marker | ?: help | q: quit"
                }
            };
            let footer = Paragraph::new(footer_text)
                .style(Style::default().fg(theme.text_secondary))
                .alignment(Alignment::Center);
            frame.render_widget(footer, chunks[2]);

            // Render modal overlays
            match mode {
                Mode::Help => Self::render_help_modal(frame, area),
                Mode::ConfirmDelete => {
                    if let Some(item) = explorer.selected_item() {
                        Self::render_confirm_delete_modal(frame, area, item);
                    }
                }
                _ => {}
            }
        })?;

        Ok(())
    }

    /// Handle keyboard input based on current mode.
    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        match self.mode {
            Mode::Normal => self.handle_normal_key(key)?,
            Mode::Search => self.handle_search_key(key)?,
            Mode::AgentFilter => self.handle_agent_filter_key(key)?,
            Mode::Help => self.handle_help_key(key)?,
            Mode::ConfirmDelete => self.handle_confirm_delete_key(key)?,
        }
        Ok(())
    }

    /// Handle keys in normal mode.
    fn handle_normal_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            // Navigation
            KeyCode::Up | KeyCode::Char('k') => self.explorer.up(),
            KeyCode::Down | KeyCode::Char('j') => self.explorer.down(),
            KeyCode::PageUp => self.explorer.page_up(),
            KeyCode::PageDown => self.explorer.page_down(),
            KeyCode::Home => self.explorer.home(),
            KeyCode::End => self.explorer.end(),

            // Actions
            KeyCode::Enter => self.play_session()?,
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.search_input.clear();
                self.status_message = None;
            }
            KeyCode::Char('f') => {
                self.mode = Mode::AgentFilter;
                // Set agent_filter_idx based on current filter
                if let Some(current) = self.explorer.agent_filter() {
                    self.agent_filter_idx = self
                        .available_agents
                        .iter()
                        .position(|a| a == current)
                        .unwrap_or(0);
                } else {
                    self.agent_filter_idx = 0; // "All"
                }
            }
            KeyCode::Char('d') => {
                if self.explorer.selected_item().is_some() {
                    self.mode = Mode::ConfirmDelete;
                }
            }
            KeyCode::Char('m') => self.add_marker()?,
            KeyCode::Char('?') => self.mode = Mode::Help,

            // Clear filters
            KeyCode::Esc => {
                self.explorer.clear_filters();
                self.search_input.clear();
                self.agent_filter_idx = 0;
            }

            // Quit is handled by EventHandler
            _ => {}
        }
        Ok(())
    }

    /// Handle keys in search mode.
    fn handle_search_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                // Keep the search filter if any was applied
            }
            KeyCode::Enter => {
                // Apply search filter
                if self.search_input.is_empty() {
                    self.explorer.set_search_filter(None);
                } else {
                    self.explorer
                        .set_search_filter(Some(self.search_input.clone()));
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.search_input.pop();
                // Live filter as user types
                if self.search_input.is_empty() {
                    self.explorer.set_search_filter(None);
                } else {
                    self.explorer
                        .set_search_filter(Some(self.search_input.clone()));
                }
            }
            KeyCode::Char(c) => {
                // Ignore ctrl+c etc in search mode
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                    self.search_input.push(c);
                    // Live filter as user types
                    self.explorer
                        .set_search_filter(Some(self.search_input.clone()));
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Handle keys in agent filter mode.
    fn handle_agent_filter_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.mode = Mode::Normal;
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if self.agent_filter_idx > 0 {
                    self.agent_filter_idx -= 1;
                } else {
                    self.agent_filter_idx = self.available_agents.len() - 1;
                }
                self.apply_agent_filter();
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.agent_filter_idx = (self.agent_filter_idx + 1) % self.available_agents.len();
                self.apply_agent_filter();
            }
            _ => {}
        }
        Ok(())
    }

    /// Apply the currently selected agent filter
    fn apply_agent_filter(&mut self) {
        let selected = &self.available_agents[self.agent_filter_idx];
        if selected == "All" {
            self.explorer.set_agent_filter(None);
        } else {
            self.explorer.set_agent_filter(Some(selected.clone()));
        }
    }

    /// Handle keys in help mode.
    fn handle_help_key(&mut self, _key: KeyEvent) -> Result<()> {
        // Any key closes help
        self.mode = Mode::Normal;
        Ok(())
    }

    /// Handle keys in confirm delete mode.
    fn handle_confirm_delete_key(&mut self, key: KeyEvent) -> Result<()> {
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

    /// Play the selected session with asciinema.
    fn play_session(&mut self) -> Result<()> {
        use crate::player;

        if let Some(item) = self.explorer.selected_item() {
            let path = Path::new(&item.path);

            // Suspend TUI - restores normal terminal mode
            self.app.suspend()?;

            // Play the session
            let result = player::play_session(path)?;

            // Resume TUI - re-enters alternate screen and raw mode
            self.app.resume()?;
            self.status_message = Some(result.message());
        }
        Ok(())
    }

    /// Delete the selected session.
    fn delete_session(&mut self) -> Result<()> {
        if let Some(item) = self.explorer.selected_item() {
            let path = item.path.clone();
            let name = item.name.clone();

            // Delete the file
            if let Err(e) = std::fs::remove_file(&path) {
                self.status_message = Some(format!("Failed to delete: {}", e));
            } else {
                // Remove from explorer to keep UI in sync
                self.explorer.remove_item(&path);
                self.status_message = Some(format!("Deleted: {}", name));
            }
        }
        Ok(())
    }

    /// Prefetch previews for current, previous, and next items.
    fn prefetch_adjacent_previews(&mut self) {
        let selected = self.explorer.selected();
        let len = self.explorer.len();
        if len == 0 {
            return;
        }

        // Collect paths to prefetch (current, prev, next)
        let mut paths_to_prefetch = Vec::with_capacity(3);

        // Current selection
        if let Some(item) = self.explorer.selected_item() {
            paths_to_prefetch.push(item.path.clone());
        }

        // Previous item (with wrap)
        let prev_idx = if selected > 0 { selected - 1 } else { len - 1 };
        if let Some((_, item, _)) = self.explorer.visible_items().nth(prev_idx) {
            paths_to_prefetch.push(item.path.clone());
        }

        // Next item (with wrap)
        let next_idx = if selected < len - 1 { selected + 1 } else { 0 };
        if let Some((_, item, _)) = self.explorer.visible_items().nth(next_idx) {
            paths_to_prefetch.push(item.path.clone());
        }

        // Request prefetch for all
        self.preview_cache.prefetch(&paths_to_prefetch);
    }

    /// Add a marker to the selected session (placeholder).
    fn add_marker(&mut self) -> Result<()> {
        self.status_message = Some("Marker feature coming soon!".to_string());
        Ok(())
    }

    /// Render the help modal overlay.
    fn render_help_modal(frame: &mut Frame, area: Rect) {
        let theme = current_theme();

        // Center the modal
        let modal_width = 60.min(area.width.saturating_sub(4));
        let modal_height = 16.min(area.height.saturating_sub(4));
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
            Line::from(vec![
                Span::styled("↑/↓ or j/k", Style::default().fg(theme.accent)),
                Span::raw("  Navigate up/down"),
            ]),
            Line::from(vec![
                Span::styled("PgUp/PgDn", Style::default().fg(theme.accent)),
                Span::raw("   Page up/down"),
            ]),
            Line::from(vec![
                Span::styled("Home/End", Style::default().fg(theme.accent)),
                Span::raw("     Go to first/last"),
            ]),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme.accent)),
                Span::raw("        Play session with asciinema"),
            ]),
            Line::from(vec![
                Span::styled("/", Style::default().fg(theme.accent)),
                Span::raw("            Search by filename"),
            ]),
            Line::from(vec![
                Span::styled("f", Style::default().fg(theme.accent)),
                Span::raw("            Filter by agent"),
            ]),
            Line::from(vec![
                Span::styled("d", Style::default().fg(theme.accent)),
                Span::raw("            Delete session"),
            ]),
            Line::from(vec![
                Span::styled("m", Style::default().fg(theme.accent)),
                Span::raw("            Add marker (coming soon)"),
            ]),
            Line::from(vec![
                Span::styled("Esc", Style::default().fg(theme.accent)),
                Span::raw("          Clear filters"),
            ]),
            Line::from(vec![
                Span::styled("q", Style::default().fg(theme.accent)),
                Span::raw("            Quit"),
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

    /// Render the confirm delete modal overlay.
    fn render_confirm_delete_modal(frame: &mut Frame, area: Rect, item: &FileItem) {
        let theme = current_theme();

        // Center the modal
        let modal_width = 50.min(area.width.saturating_sub(4));
        let modal_height = 7.min(area.height.saturating_sub(4));
        let x = (area.width - modal_width) / 2;
        let y = (area.height - modal_height) / 2;
        let modal_area = Rect::new(x, y, modal_width, modal_height);

        // Clear the area behind the modal
        frame.render_widget(Clear, modal_area);

        let text = vec![
            Line::from(Span::styled(
                "Delete Session?",
                Style::default()
                    .fg(theme.error)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(format!("File: {}", item.name)),
            Line::from(""),
            Line::from(vec![
                Span::styled("y", Style::default().fg(theme.error)),
                Span::raw(": Yes, delete  |  "),
                Span::styled("n", Style::default().fg(theme.accent)),
                Span::raw(": No, cancel"),
            ]),
        ];

        let confirm = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.error))
                    .title(" Confirm Delete "),
            )
            .alignment(Alignment::Center);

        frame.render_widget(confirm, modal_area);
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
}
