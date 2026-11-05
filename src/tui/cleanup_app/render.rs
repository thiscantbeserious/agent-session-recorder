//! Rendering helpers for the cleanup app (help modal and status line).

use ratatui::{layout::Rect, text::Line, Frame};

use super::status::status_line_content;
use super::{CleanupApp, Mode};
use crate::theme::current_theme;
use crate::tui::app::modals::{
    help_close_hint, help_section_header, help_shortcut_line, help_title_line,
    render_help_paragraph,
};
use crate::tui::app::status_footer::{render_input_line, render_status_line};

impl CleanupApp {
    /// Render the help modal overlay.
    pub(super) fn render_help_modal(frame: &mut Frame, area: Rect) {
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
                "              Glob select (e.g., *2024*, claude/*.cast*)",
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
    pub(super) fn render_status_for_mode(
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
