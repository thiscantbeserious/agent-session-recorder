//! Shared status line and footer rendering for TUI explorer applications
//!
//! Provides rendering functions for the status bar (filter info, mode prompts)
//! and the footer bar (keybinding hints).

use ratatui::{
    layout::{Alignment, Rect},
    style::Style,
    widgets::Paragraph,
    Frame,
};

use crate::theme::current_theme;

/// Render a status line with the given text.
///
/// Displays the text in the secondary text color of the current theme.
/// Each app computes its own mode-aware status text and passes it here.
pub fn render_status_line(frame: &mut Frame, area: Rect, text: &str) {
    let theme = current_theme();
    let status = Paragraph::new(text.to_string()).style(Style::default().fg(theme.text_secondary));
    frame.render_widget(status, area);
}

/// Render a centered footer from a pre-formatted text string.
///
/// Displays the text centered in the secondary text color of the current
/// theme. Each app composes its own mode-specific footer text.
pub fn render_footer_text(frame: &mut Frame, area: Rect, text: &str) {
    let theme = current_theme();
    let footer = Paragraph::new(text.to_string())
        .style(Style::default().fg(theme.text_secondary))
        .alignment(Alignment::Center);
    frame.render_widget(footer, area);
}
