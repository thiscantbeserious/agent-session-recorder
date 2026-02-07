//! Shared modal utilities for TUI explorer applications
//!
//! Provides `center_modal()` for creating centered modal areas and
//! shared modal rendering functions used by both apps.

use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::theme::current_theme;

/// Calculate a centered modal area within the given parent area.
///
/// Constrains the modal to the given `width` and `height`, centered
/// both horizontally and vertically. Clamps to fit within the parent
/// area with at least 2 cells of margin on each side.
pub fn center_modal(area: Rect, width: u16, height: u16) -> Rect {
    let modal_width = width.min(area.width.saturating_sub(4));
    let modal_height = height.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    Rect::new(x, y, modal_width, modal_height)
}

/// Render a confirm-delete modal for a single file.
///
/// Shows the filename and y/n confirmation prompt. Extracted from
/// `list_app.rs` `render_confirm_delete_modal`. The cleanup app uses
/// a different bulk-delete modal, so it keeps its own version.
pub fn render_confirm_delete_modal(frame: &mut Frame, area: Rect, filename: &str) {
    let theme = current_theme();
    let modal_area = center_modal(area, 50, 7);

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
        Line::from(format!("File: {}", filename)),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_modal_is_centered() {
        let area = Rect::new(0, 0, 80, 24);
        let modal = center_modal(area, 40, 10);
        assert_eq!(modal.width, 40);
        assert_eq!(modal.height, 10);
        assert_eq!(modal.x, 20); // (80 - 40) / 2
        assert_eq!(modal.y, 7); // (24 - 10) / 2
    }

    #[test]
    fn center_modal_clamps_to_area() {
        let area = Rect::new(0, 0, 30, 10);
        let modal = center_modal(area, 80, 40);
        // Should clamp: width = min(80, 30-4) = 26, height = min(40, 10-4) = 6
        assert_eq!(modal.width, 26);
        assert_eq!(modal.height, 6);
    }

    #[test]
    fn center_modal_respects_area_offset() {
        let area = Rect::new(10, 5, 80, 24);
        let modal = center_modal(area, 40, 10);
        assert_eq!(modal.x, 30); // 10 + (80 - 40) / 2
        assert_eq!(modal.y, 12); // 5 + (24 - 10) / 2
    }
}
