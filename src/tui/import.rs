//! Import state and path parsing for drag-and-drop .cast file imports
//!
//! Handles state management for the import flow including:
//! - Path parsing from pasted text (tilde expansion, quote trimming, relative resolution)
//! - Agent name autocomplete filtering
//! - Import phase tracking (AgentInput, Importing, Done)
//! - Results tracking per file

use std::path::PathBuf;

use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::theme::current_theme;
use crate::tui::app::modals::center_modal;

/// Parse pasted text into file paths
///
/// - Splits on newlines
/// - Trims whitespace and quotes (single/double)
/// - Expands tilde (~) to home directory
/// - Resolves relative paths against current working directory
pub fn parse_paste_paths(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|line| {
            // Trim surrounding quotes
            let trimmed = line
                .trim_start_matches('\'')
                .trim_start_matches('"')
                .trim_end_matches('\'')
                .trim_end_matches('"');

            // Expand tilde
            let expanded = if let Some(rest) = trimmed.strip_prefix('~') {
                if let Some(home) = dirs::home_dir() {
                    home.join(rest.trim_start_matches('/'))
                } else {
                    PathBuf::from(trimmed)
                }
            } else {
                PathBuf::from(trimmed)
            };

            // Resolve relative paths
            if expanded.is_relative() {
                std::env::current_dir()
                    .map(|cwd| cwd.join(&expanded))
                    .unwrap_or(expanded)
            } else {
                expanded
            }
        })
        .collect()
}

/// Import workflow phase
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportPhase {
    /// User is typing agent name
    AgentInput,
    /// Validation and copy in progress
    Importing,
    /// Results ready for display
    Done,
}

/// Result of importing a single file
#[derive(Debug, Clone)]
pub struct ImportResult {
    /// Original filename
    pub filename: String,
    /// Destination path on success, error message on failure
    pub outcome: Result<PathBuf, String>,
}

/// State for the import modal workflow
pub struct ImportState {
    /// Current phase
    pub phase: ImportPhase,
    /// Parsed file paths to import
    pub paths: Vec<PathBuf>,
    /// Current agent name input
    pub agent_input: String,
    /// Cursor byte offset in agent_input
    pub agent_cursor: usize,
    /// Agents matching current input prefix
    pub filtered_agents: Vec<String>,
    /// Selected suggestion index (if any)
    pub autocomplete_idx: Option<usize>,
    /// Import results per file
    pub results: Vec<ImportResult>,
}

impl ImportState {
    /// Create new import state from pasted text
    ///
    /// Parses paths and initializes agent filter with all available agents.
    /// Starts in AgentInput phase.
    pub fn new(paste_text: &str, available_agents: &[String]) -> Self {
        let paths = parse_paste_paths(paste_text);
        let filtered_agents: Vec<String> = available_agents
            .iter()
            .filter(|a| *a != "All")
            .cloned()
            .collect();

        let autocomplete_idx = if filtered_agents.is_empty() {
            None
        } else {
            Some(0)
        };

        Self {
            phase: ImportPhase::AgentInput,
            paths,
            agent_input: String::new(),
            agent_cursor: 0,
            filtered_agents,
            autocomplete_idx,
            results: Vec::new(),
        }
    }

    /// Update agent filter based on current input
    ///
    /// Filters available agents by prefix match, resets autocomplete selection.
    pub fn update_agent_filter(&mut self, available_agents: &[String]) {
        let input_lower = self.agent_input.to_lowercase();
        self.filtered_agents = available_agents
            .iter()
            .filter(|a| *a != "All" && a.to_lowercase().starts_with(&input_lower))
            .cloned()
            .collect();

        self.autocomplete_idx = if self.filtered_agents.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    /// Cycle autocomplete selection up
    pub fn autocomplete_up(&mut self) {
        if let Some(idx) = self.autocomplete_idx {
            if !self.filtered_agents.is_empty() {
                self.autocomplete_idx = Some(if idx == 0 {
                    self.filtered_agents.len() - 1
                } else {
                    idx - 1
                });
            }
        }
    }

    /// Cycle autocomplete selection down
    pub fn autocomplete_down(&mut self) {
        if let Some(idx) = self.autocomplete_idx {
            if !self.filtered_agents.is_empty() {
                self.autocomplete_idx = Some((idx + 1) % self.filtered_agents.len());
            }
        }
    }

    /// Fill agent_input from selected autocomplete suggestion
    pub fn accept_autocomplete(&mut self) {
        if let Some(idx) = self.autocomplete_idx {
            if let Some(agent) = self.filtered_agents.get(idx) {
                self.agent_input = agent.clone();
                self.agent_cursor = self.agent_input.len();
            }
        }
    }

    /// Get the selected agent name (trimmed)
    pub fn selected_agent(&self) -> &str {
        self.agent_input.trim()
    }

    /// Get number of files to import
    pub fn file_count(&self) -> usize {
        self.paths.len()
    }

    /// Check if there are paths to import
    pub fn has_paths(&self) -> bool {
        !self.paths.is_empty()
    }

    /// Insert character at cursor position
    pub fn agent_input_char(&mut self, c: char) {
        self.agent_input.insert(self.agent_cursor, c);
        self.agent_cursor += c.len_utf8();
    }

    /// Delete character before cursor
    pub fn agent_input_backspace(&mut self) {
        if self.agent_cursor > 0 {
            let before = &self.agent_input[..self.agent_cursor];
            if let Some((last_char_start, _)) = before.char_indices().last() {
                self.agent_input.remove(last_char_start);
                self.agent_cursor = last_char_start;
            }
        }
    }

    /// Delete character at cursor
    pub fn agent_input_delete(&mut self) {
        if self.agent_cursor < self.agent_input.len() {
            self.agent_input.remove(self.agent_cursor);
        }
    }

    /// Move cursor left
    pub fn agent_input_left(&mut self) {
        if self.agent_cursor > 0 {
            let before = &self.agent_input[..self.agent_cursor];
            if let Some((last_char_start, _)) = before.char_indices().last() {
                self.agent_cursor = last_char_start;
            }
        }
    }

    /// Move cursor right
    pub fn agent_input_right(&mut self) {
        if self.agent_cursor < self.agent_input.len() {
            let after = &self.agent_input[self.agent_cursor..];
            if let Some((_, ch)) = after.char_indices().next() {
                self.agent_cursor += ch.len_utf8();
            }
        }
    }
}

/// Render the import modal overlay (public entry point)
pub fn render(state: &ImportState, frame: &mut Frame, area: Rect) {
    match state.phase {
        ImportPhase::AgentInput => render_agent_input(state, frame, area),
        ImportPhase::Importing => render_agent_input(state, frame, area), // Show same UI while importing
        ImportPhase::Done => render_result(state, frame, area),
    }
}

/// Render agent input phase modal
fn render_agent_input(state: &ImportState, frame: &mut Frame, area: Rect) {
    let theme = current_theme();

    // Calculate modal size based on autocomplete list
    let autocomplete_lines = state.filtered_agents.len().min(5);
    let modal_height = 8 + autocomplete_lines as u16; // title + input + list + footer + padding
    let modal_area = center_modal(area, 50, modal_height);

    frame.render_widget(Clear, modal_area);

    let mut lines = vec![
        Line::from(Span::styled(
            format!("Import {} file(s)", state.file_count()),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    // Agent input line with cursor
    let input_label = "Agent: ";
    let before = &state.agent_input[..state.agent_cursor];
    let after = &state.agent_input[state.agent_cursor..];

    let mut input_spans = vec![
        Span::styled(input_label, Style::default().fg(theme.text_secondary)),
        Span::styled(before, Style::default().fg(theme.text_primary)),
        Span::styled(
            "│",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ];
    if !after.is_empty() {
        input_spans.push(Span::styled(after, Style::default().fg(theme.text_primary)));
    }
    lines.push(Line::from(input_spans));
    lines.push(Line::from(""));

    // Autocomplete dropdown
    if !state.filtered_agents.is_empty() {
        lines.push(Line::from(Span::styled(
            "Suggestions:",
            Style::default().fg(theme.text_secondary),
        )));

        for (idx, agent) in state.filtered_agents.iter().take(5).enumerate() {
            let is_selected = state.autocomplete_idx == Some(idx);
            let style = if is_selected {
                theme.highlight_style()
            } else {
                Style::default().fg(theme.text_primary)
            };
            let prefix = if is_selected { "> " } else { "  " };
            lines.push(Line::from(Span::styled(
                format!("{}{}", prefix, agent),
                style,
            )));
        }
    } else if !state.agent_input.is_empty() {
        lines.push(Line::from(Span::styled(
            "No matching agents",
            Style::default().fg(theme.text_secondary),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Enter: confirm | Esc: cancel | Tab: complete",
        Style::default().fg(theme.text_secondary),
    )));

    let modal = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent))
                .title(" Import "),
        )
        .alignment(Alignment::Left);

    frame.render_widget(modal, modal_area);
}

/// Render import result phase modal
fn render_result(state: &ImportState, frame: &mut Frame, area: Rect) {
    let theme = current_theme();

    // Calculate modal size based on results
    // Failed imports render 2 lines (filename + error detail), successful imports render 1 line
    let result_lines: usize = state
        .results
        .iter()
        .map(|r| if r.outcome.is_err() { 2 } else { 1 })
        .sum();
    let modal_height = (6 + result_lines as u16).min(20); // title + results + footer + padding
    let modal_area = center_modal(area, 60, modal_height);

    frame.render_widget(Clear, modal_area);

    let mut lines = vec![];

    // Count successes
    let success_count = state.results.iter().filter(|r| r.outcome.is_ok()).count();
    let total_count = state.results.len();

    let title = if success_count == total_count {
        "Import Complete"
    } else {
        "Import Results"
    };

    lines.push(Line::from(Span::styled(
        title,
        Style::default()
            .fg(if success_count == total_count {
                theme.success
            } else {
                theme.accent
            })
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    // Per-file status
    for result in &state.results {
        match &result.outcome {
            Ok(_path) => {
                lines.push(Line::from(vec![
                    Span::styled("✓ ", Style::default().fg(theme.success)),
                    Span::styled(&result.filename, Style::default().fg(theme.text_primary)),
                ]));
            }
            Err(error) => {
                lines.push(Line::from(vec![
                    Span::styled("✗ ", Style::default().fg(theme.error)),
                    Span::styled(&result.filename, Style::default().fg(theme.text_primary)),
                ]));
                lines.push(Line::from(Span::styled(
                    format!("  {}", error),
                    Style::default().fg(theme.error),
                )));
            }
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("Imported {}/{} files", success_count, total_count),
        Style::default().fg(theme.text_secondary),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Enter/Esc: dismiss",
        Style::default().fg(theme.text_secondary),
    )));

    let modal = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if success_count == total_count {
                    theme.success
                } else {
                    theme.accent
                }))
                .title(" Import "),
        )
        .alignment(Alignment::Left);

    frame.render_widget(modal, modal_area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_paste_single_absolute_path() {
        let text = "/Users/test/session.cast";
        let paths = parse_paste_paths(text);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], PathBuf::from("/Users/test/session.cast"));
    }

    #[test]
    fn parse_paste_multiple_paths_newline_separated() {
        let text = "/path/one.cast\n/path/two.cast\n/path/three.cast";
        let paths = parse_paste_paths(text);
        assert_eq!(paths.len(), 3);
        assert_eq!(paths[0], PathBuf::from("/path/one.cast"));
        assert_eq!(paths[1], PathBuf::from("/path/two.cast"));
        assert_eq!(paths[2], PathBuf::from("/path/three.cast"));
    }

    #[test]
    fn parse_paste_tilde_expansion() {
        let text = "~/session.cast";
        let paths = parse_paste_paths(text);
        assert_eq!(paths.len(), 1);
        // Should expand to home directory
        let home = dirs::home_dir().expect("HOME must be set for this test");
        assert_eq!(paths[0], home.join("session.cast"));
    }

    #[test]
    fn parse_paste_quoted_paths() {
        let text = r#""/path/with spaces/file.cast"
'/another/path/file.cast'"#;
        let paths = parse_paste_paths(text);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], PathBuf::from("/path/with spaces/file.cast"));
        assert_eq!(paths[1], PathBuf::from("/another/path/file.cast"));
    }

    #[test]
    fn parse_paste_empty_lines_skipped() {
        let text = "/path/one.cast\n\n  \n/path/two.cast\n\n";
        let paths = parse_paste_paths(text);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], PathBuf::from("/path/one.cast"));
        assert_eq!(paths[1], PathBuf::from("/path/two.cast"));
    }

    #[test]
    fn parse_paste_relative_path_resolved() {
        let text = "session.cast";
        let paths = parse_paste_paths(text);
        assert_eq!(paths.len(), 1);
        // Should resolve against current directory
        if let Ok(cwd) = std::env::current_dir() {
            assert_eq!(paths[0], cwd.join("session.cast"));
        }
    }

    #[test]
    fn import_state_new_parses_paths() {
        let agents = vec![
            "agent1".to_string(),
            "agent2".to_string(),
            "All".to_string(),
        ];
        let state = ImportState::new("/path/one.cast\n/path/two.cast", &agents);

        assert_eq!(state.file_count(), 2);
        assert!(state.has_paths());
        assert_eq!(state.phase, ImportPhase::AgentInput);
        assert_eq!(state.agent_input, "");
        // Should filter out "All"
        assert_eq!(state.filtered_agents.len(), 2);
        assert!(!state.filtered_agents.contains(&"All".to_string()));
    }

    #[test]
    fn import_state_agent_filter_prefix_match() {
        let agents = vec![
            "claude".to_string(),
            "cursor".to_string(),
            "copilot".to_string(),
            "gemini".to_string(),
            "All".to_string(),
        ];
        let mut state = ImportState::new("/path/file.cast", &agents);

        // Filter by "c"
        state.agent_input = "c".to_string();
        state.agent_cursor = 1;
        state.update_agent_filter(&agents);

        assert_eq!(state.filtered_agents.len(), 3);
        assert!(state.filtered_agents.contains(&"claude".to_string()));
        assert!(state.filtered_agents.contains(&"cursor".to_string()));
        assert!(state.filtered_agents.contains(&"copilot".to_string()));
        assert!(!state.filtered_agents.contains(&"gemini".to_string()));
        assert!(!state.filtered_agents.contains(&"All".to_string()));

        // Filter by "cu"
        state.agent_input = "cu".to_string();
        state.agent_cursor = 2;
        state.update_agent_filter(&agents);

        assert_eq!(state.filtered_agents.len(), 1);
        assert!(state.filtered_agents.contains(&"cursor".to_string()));
    }

    #[test]
    fn import_state_autocomplete_cycle() {
        let agents = vec![
            "agent1".to_string(),
            "agent2".to_string(),
            "agent3".to_string(),
        ];
        let mut state = ImportState::new("/path/file.cast", &agents);

        // Start at 0
        assert_eq!(state.autocomplete_idx, Some(0));

        // Down to 1
        state.autocomplete_down();
        assert_eq!(state.autocomplete_idx, Some(1));

        // Down to 2
        state.autocomplete_down();
        assert_eq!(state.autocomplete_idx, Some(2));

        // Down wraps to 0
        state.autocomplete_down();
        assert_eq!(state.autocomplete_idx, Some(0));

        // Up wraps to 2
        state.autocomplete_up();
        assert_eq!(state.autocomplete_idx, Some(2));

        // Up to 1
        state.autocomplete_up();
        assert_eq!(state.autocomplete_idx, Some(1));
    }

    #[test]
    fn import_state_accept_autocomplete_fills_input() {
        let agents = vec!["claude".to_string(), "cursor".to_string()];
        let mut state = ImportState::new("/path/file.cast", &agents);

        // Select first agent (claude) and accept
        state.autocomplete_idx = Some(0);
        state.accept_autocomplete();

        assert_eq!(state.agent_input, "claude");
        assert_eq!(state.agent_cursor, 6);

        // Reset and select second agent
        state.agent_input.clear();
        state.agent_cursor = 0;
        state.autocomplete_idx = Some(1);
        state.accept_autocomplete();

        assert_eq!(state.agent_input, "cursor");
        assert_eq!(state.agent_cursor, 6);
    }

    #[test]
    fn import_state_agent_input_char_insert() {
        let agents = vec!["test".to_string()];
        let mut state = ImportState::new("/path/file.cast", &agents);

        state.agent_input_char('a');
        assert_eq!(state.agent_input, "a");
        assert_eq!(state.agent_cursor, 1);

        state.agent_input_char('b');
        assert_eq!(state.agent_input, "ab");
        assert_eq!(state.agent_cursor, 2);

        // Insert in middle
        state.agent_cursor = 1;
        state.agent_input_char('x');
        assert_eq!(state.agent_input, "axb");
        assert_eq!(state.agent_cursor, 2);
    }

    #[test]
    fn import_state_agent_input_backspace() {
        let agents = vec!["test".to_string()];
        let mut state = ImportState::new("/path/file.cast", &agents);

        state.agent_input = "abc".to_string();
        state.agent_cursor = 3;

        state.agent_input_backspace();
        assert_eq!(state.agent_input, "ab");
        assert_eq!(state.agent_cursor, 2);

        state.agent_input_backspace();
        assert_eq!(state.agent_input, "a");
        assert_eq!(state.agent_cursor, 1);

        // Backspace at beginning does nothing
        state.agent_cursor = 0;
        state.agent_input_backspace();
        assert_eq!(state.agent_input, "a");
        assert_eq!(state.agent_cursor, 0);
    }
}
