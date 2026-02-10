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

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use crate::theme::current_theme;

/// Result of clicking inside a confirm modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmClick {
    /// User clicked the "y" (left) button
    Yes,
    /// User clicked the "n" (right) button or outside the modal
    No,
    /// Click was inside modal but not on a button — ignore
    Ignored,
}

/// Handle a mouse click against a centered confirm modal.
///
/// `modal_width` and `modal_height` must match the values passed to
/// `center_modal()` when the modal was rendered.  `button_row_offset`
/// is the row index (from the top of the modal) where the y/n buttons
/// live.
///
/// Returns `ConfirmClick::Yes` when the left half of the button row is
/// clicked, `No` for right half or outside, and `Ignored` for clicks
/// inside the modal but not on the button row.
pub fn handle_confirm_click(
    mouse: &MouseEvent,
    area_width: u16,
    area_height: u16,
    modal_width: u16,
    modal_height: u16,
    button_row_offset: u16,
) -> ConfirmClick {
    if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        return ConfirmClick::Ignored;
    }

    let mw = modal_width.min(area_width.saturating_sub(4));
    let mh = modal_height.min(area_height.saturating_sub(4));
    let mx = (area_width - mw) / 2;
    let my = (area_height - mh) / 2;
    let cx = mouse.column;
    let cy = mouse.row;

    if cx < mx || cx >= mx + mw || cy < my || cy >= my + mh {
        return ConfirmClick::No; // outside modal
    }

    if cy == my + button_row_offset {
        if cx < mx + mw / 2 {
            ConfirmClick::Yes
        } else {
            ConfirmClick::No
        }
    } else {
        ConfirmClick::Ignored
    }
}

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
/// When `final_confirm` is true, shows "ARE YOU SURE - THIS WILL DELETE"
/// as the second confirmation step.
pub fn render_confirm_delete_modal(
    frame: &mut Frame,
    area: Rect,
    count: usize,
    size: u64,
    final_confirm: bool,
) {
    let theme = current_theme();
    let modal_area = center_modal(area, 50, 8);

    frame.render_widget(Clear, modal_area);

    let heading = if final_confirm {
        "ARE YOU SURE - THIS WILL DELETE"
    } else if count == 1 {
        "Delete Session?"
    } else {
        "Delete Sessions?"
    };

    let body_style = if final_confirm {
        Style::default().fg(theme.error)
    } else {
        Style::default()
    };

    let text = vec![
        Line::from(Span::styled(
            heading,
            Style::default()
                .fg(theme.error)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("Sessions to delete: {}", count),
            body_style,
        )),
        Line::from(Span::styled(
            format!(
                "Storage to free: {}",
                humansize::format_size(size, humansize::BINARY)
            ),
            body_style,
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("y", Style::default().fg(theme.error)),
            Span::styled(": Yes 🗑️   |  ", body_style),
            Span::styled("n", Style::default().fg(theme.accent)),
            Span::raw(": No ❌"),
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
/// When `final_confirm` is true, shows "ARE YOU SURE - THIS WILL UNLOCK"
/// as the second confirmation step.
pub fn render_confirm_unlock_modal(
    frame: &mut Frame,
    area: Rect,
    lock_msg: &str,
    final_confirm: bool,
) {
    let theme = current_theme();
    let modal_area = center_modal(area, 55, 8);

    frame.render_widget(Clear, modal_area);

    let heading = if final_confirm {
        "ARE YOU SURE - THIS WILL UNLOCK"
    } else {
        "Session Locked"
    };

    let body_style = if final_confirm {
        Style::default().fg(theme.error)
    } else {
        Style::default()
    };

    let text = vec![
        Line::from(Span::styled(
            heading,
            Style::default()
                .fg(theme.error)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled("This session is being recorded.", body_style)),
        Line::from(Span::styled(format!("Lock: {}", lock_msg), body_style)),
        Line::from(""),
        Line::from(vec![
            Span::styled("y", Style::default().fg(theme.error)),
            Span::styled(": Yes 🔓  |  ", body_style),
            Span::styled("n", Style::default().fg(theme.accent)),
            Span::raw(": No ❌"),
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
