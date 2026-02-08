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

/// Render a confirm-delete modal showing count and storage impact.
///
/// Used by both list (single delete) and cleanup (bulk delete) apps.
pub fn render_confirm_delete_modal(frame: &mut Frame, area: Rect, count: usize, size: u64) {
    let theme = current_theme();
    let modal_area = center_modal(area, 50, 8);

    // Clear the area behind the modal
    frame.render_widget(Clear, modal_area);

    let title = if count == 1 {
        "Delete Session?"
    } else {
        "Delete Sessions?"
    };

    let text = vec![
        Line::from(Span::styled(
            title,
            Style::default()
                .fg(theme.error)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("Sessions to delete: {}", count)),
        Line::from(format!(
            "Storage to free: {}",
            humansize::format_size(size, humansize::BINARY)
        )),
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

/// Render a confirm-unlock modal showing lock details.
///
/// Prompts the user to force-unlock a session that is currently being recorded.
pub fn render_confirm_unlock_modal(frame: &mut Frame, area: Rect, lock_msg: &str) {
    let theme = current_theme();
    let modal_area = center_modal(area, 55, 8);

    // Clear the area behind the modal
    frame.render_widget(Clear, modal_area);

    let text = vec![
        Line::from(Span::styled(
            "Session Locked",
            Style::default()
                .fg(theme.error)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("This session is being recorded.".to_string()),
        Line::from(format!("Lock: {}", lock_msg)),
        Line::from(""),
        Line::from(vec![
            Span::styled("y", Style::default().fg(theme.error)),
            Span::raw(": Force unlock  |  "),
            Span::styled("n", Style::default().fg(theme.accent)),
            Span::raw(": Cancel"),
        ]),
    ];

    let confirm = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.error))
                .title(" Confirm Unlock "),
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
