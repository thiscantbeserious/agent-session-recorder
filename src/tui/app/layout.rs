//! Shared layout helpers for TUI explorer applications
//!
//! Provides the standard 3-chunk vertical layout used by all explorer apps:
//! explorer area (flexible), status line (1 row), footer (1 row).

use ratatui::layout::{Constraint, Layout, Rect};

/// Build the standard explorer layout: explorer / status line / footer.
///
/// Returns a Vec of 3 `Rect` chunks:
/// - `[0]` explorer area (`Min(1)` -- takes remaining space)
/// - `[1]` status line (`Length(1)`)
/// - `[2]` footer (`Length(1)`)
///
pub fn build_explorer_layout(area: Rect) -> Vec<Rect> {
    Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area)
    .to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_explorer_layout_returns_three_chunks() {
        let area = Rect::new(0, 0, 80, 24);
        let chunks = build_explorer_layout(area);
        assert_eq!(chunks.len(), 3);
    }

    #[test]
    fn build_explorer_layout_footer_is_one_row() {
        let area = Rect::new(0, 0, 80, 24);
        let chunks = build_explorer_layout(area);
        assert_eq!(chunks[1].height, 1);
        assert_eq!(chunks[2].height, 1);
    }
}
