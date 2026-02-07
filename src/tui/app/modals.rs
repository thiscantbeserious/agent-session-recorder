//! Shared modal utilities for TUI explorer applications
//!
//! Provides `center_modal()` for creating centered modal areas and
//! shared modal rendering functions used by both apps.

use ratatui::layout::Rect;

/// Calculate a centered modal area within the given parent area.
///
/// Constrains the modal to the given `width` and `height`, centered
/// both horizontally and vertically. Clamps to fit within the parent
/// area with at least 2 cells of margin on each side.
#[allow(dead_code)]
pub fn center_modal(area: Rect, width: u16, height: u16) -> Rect {
    let modal_width = width.min(area.width.saturating_sub(4));
    let modal_height = height.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    Rect::new(x, y, modal_width, modal_height)
}

/// Render a confirm-delete modal overlay.
///
/// Stub for now -- will be populated in Stage 5 with the shared
/// confirm-delete modal rendering extracted from list_app and cleanup_app.
#[allow(dead_code)]
pub fn render_confirm_delete_modal(_frame: &mut ratatui::Frame, _area: Rect, _message: &str) {
    // Stub -- will be populated in Stage 5
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
