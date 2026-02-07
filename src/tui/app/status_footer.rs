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
///
/// Stub for now -- will be populated in Stage 5 with the mode-aware
/// status text logic extracted from list_app and cleanup_app draw methods.
#[allow(dead_code)]
pub fn render_status_line(frame: &mut Frame, area: Rect, text: &str) {
    let theme = current_theme();
    let status = Paragraph::new(text.to_string()).style(Style::default().fg(theme.text_secondary));
    frame.render_widget(status, area);
}

/// Render a centered footer with keybinding hints.
///
/// Displays the text centered in the secondary text color.
///
/// Stub for now -- will be populated in Stage 5 with the mode-aware
/// footer text logic extracted from list_app and cleanup_app draw methods.
#[allow(dead_code)]
pub fn render_footer(frame: &mut Frame, area: Rect, text: &str) {
    let theme = current_theme();
    let footer = Paragraph::new(text.to_string())
        .style(Style::default().fg(theme.text_secondary))
        .alignment(Alignment::Center);
    frame.render_widget(footer, area);
}
