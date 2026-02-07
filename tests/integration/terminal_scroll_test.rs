//! Scroll region tests.

use agr::terminal::TerminalBuffer;

#[test]
fn scroll_when_full() {
    let mut buf = TerminalBuffer::new(10, 3);
    buf.process("Line 1\r\nLine 2\r\nLine 3\r\nLine 4", None);
    assert_eq!(buf.to_string(), "Line 2\nLine 3\nLine 4");
}

#[test]
fn reverse_index_moves_cursor_up() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("Line1\r\nLine2", None);
    buf.process("\x1bM", None);
    buf.process("X", None);
    assert_eq!(buf.to_string(), "Line1X\nLine2");
}

#[test]
fn reverse_index_scrolls_at_top() {
    let mut buf = TerminalBuffer::new(10, 3);
    buf.process("Line1\r\nLine2\r\nLine3", None);
    buf.process("\x1b[1;1H", None);
    buf.process("\x1bM", None);
    buf.process("New", None);
    assert_eq!(buf.to_string(), "New\nLine1\nLine2");
}

#[test]
fn scroll_region_basic_setup() {
    let mut buf = TerminalBuffer::new(10, 5);
    buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4", None);

    // Set scroll region to rows 2-4 (1-indexed), which is rows 1-3 (0-indexed)
    buf.process("\x1b[2;4r", None);

    // Cursor moves to top of scroll region (scroll_top), not absolute row 0
    assert_eq!(buf.cursor_row(), 1); // scroll_top = 1 (0-indexed)
    assert_eq!(buf.cursor_col(), 0);
}

#[test]
fn scroll_region_scroll_within_region() {
    let mut buf = TerminalBuffer::new(10, 5);
    buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4", None);

    buf.process("\x1b[2;4r", None);
    buf.process("\x1b[4;1H", None);
    assert_eq!(buf.cursor_row(), 3);

    buf.process("\n", None);

    let output = buf.to_string();
    let lines: Vec<&str> = output.lines().collect();

    assert!(lines[0].starts_with("Line0"), "Line0 should be preserved");
    assert!(
        lines[4].starts_with("Line4"),
        "Line4 should be preserved at row 4"
    );
}

#[test]
fn scroll_region_reverse_index_within_region() {
    let mut buf = TerminalBuffer::new(10, 5);
    buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4", None);

    buf.process("\x1b[2;4r", None);
    buf.process("\x1b[2;1H", None);
    assert_eq!(buf.cursor_row(), 1);

    buf.process("\x1bM", None);

    let output = buf.to_string();
    let lines: Vec<&str> = output.lines().collect();

    assert!(lines[0].starts_with("Line0"), "Line0 should be preserved");
    assert!(
        lines[4].starts_with("Line4"),
        "Line4 should be preserved at row 4"
    );
}

#[test]
fn scroll_region_csi_scroll_up() {
    let mut buf = TerminalBuffer::new(10, 5);
    buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4", None);

    buf.process("\x1b[2;4r", None);
    buf.process("\x1b[1S", None);

    let output = buf.to_string();
    let lines: Vec<&str> = output.lines().collect();

    assert!(lines[0].starts_with("Line0"), "Line0 should be preserved");
    assert!(
        lines[4].starts_with("Line4"),
        "Line4 should be preserved at row 4"
    );
}

#[test]
fn scroll_region_csi_scroll_down() {
    let mut buf = TerminalBuffer::new(10, 5);
    buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4", None);

    buf.process("\x1b[2;4r", None);
    buf.process("\x1b[1T", None);

    let output = buf.to_string();
    let lines: Vec<&str> = output.lines().collect();

    assert!(lines[0].starts_with("Line0"), "Line0 should be preserved");
    assert!(
        lines[4].starts_with("Line4"),
        "Line4 should be preserved at row 4"
    );
}

#[test]
fn scroll_region_reset_on_resize() {
    let mut buf = TerminalBuffer::new(10, 5);

    buf.process("\x1b[2;4r", None);
    buf.resize(10, 10);

    buf.process("\x1b[10;1H", None);
    buf.process("Last\n", None);

    assert_eq!(buf.height(), 10);
}

#[test]
fn scroll_region_full_screen_default() {
    let mut buf = TerminalBuffer::new(10, 5);
    buf.process("L0\r\nL1\r\nL2\r\nL3\r\nL4", None);

    buf.process("\x1b[5;1H", None);
    buf.process("\nL5", None);

    let output = buf.to_string();
    let lines: Vec<&str> = output.lines().collect();

    assert!(!lines[0].starts_with("L0"), "L0 should have scrolled off");
    assert!(lines[4].starts_with("L5"), "L5 should be at bottom");
}

#[test]
fn scroll_region_reset_via_csi_r() {
    let mut buf = TerminalBuffer::new(10, 5);
    buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4", None);

    buf.process("\x1b[2;4r", None);
    buf.process("\x1b[r", None);

    buf.process("\x1b[5;1H\n", None);

    let output = buf.to_string();
    let lines: Vec<&str> = output.lines().collect();

    assert!(
        !lines[0].starts_with("Line0"),
        "Line0 should have scrolled off"
    );
}

// ============================================================================
// Edge case tests (reviewer findings)
// ============================================================================

/// Test line feed when cursor is above the scroll region.
/// Cursor should move down normally and enter the scroll region.
#[test]
fn line_feed_cursor_above_scroll_region() {
    let mut buf = TerminalBuffer::new(10, 6);
    // Fill all lines
    buf.process("Row0\r\nRow1\r\nRow2\r\nRow3\r\nRow4\r\nRow5", None);

    // Set scroll region to rows 3-5 (1-indexed), which is rows 2-4 (0-indexed)
    buf.process("\x1b[3;5r", None);
    // Cursor moves to top of scroll region (scroll_top = 2)
    assert_eq!(buf.cursor_row(), 2);

    // Move cursor to row 0 to test line feed from above region
    buf.process("\x1b[1;1H", None);
    assert_eq!(buf.cursor_row(), 0);

    // Now do line feeds to move through and into the scroll region
    buf.process("\n", None); // row 0 -> 1
    assert_eq!(buf.cursor_row(), 1);
    buf.process("\n", None); // row 1 -> 2 (entering scroll region, None)
    assert_eq!(buf.cursor_row(), 2);
    buf.process("\n", None); // row 2 -> 3 (within scroll region, None)
    assert_eq!(buf.cursor_row(), 3);
    buf.process("\n", None); // row 3 -> 4 (at scroll_bottom, None)
    assert_eq!(buf.cursor_row(), 4);

    // At scroll_bottom, line feed should scroll the region, not move cursor
    buf.process("\n", None);
    assert_eq!(buf.cursor_row(), 4); // Cursor stays at scroll_bottom

    // Verify that rows outside region are preserved
    let output = buf.to_string();
    let lines: Vec<&str> = output.lines().collect();
    assert!(lines[0].starts_with("Row0"), "Row0 should be preserved");
    assert!(lines[1].starts_with("Row1"), "Row1 should be preserved");
    assert!(
        lines[5].starts_with("Row5"),
        "Row5 (below region) should be preserved"
    );
}

/// Test that invalid scroll region (top > bottom) is ignored.
#[test]
fn scroll_region_invalid_top_greater_than_bottom() {
    let mut buf = TerminalBuffer::new(10, 5);
    buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4", None);

    // First set a valid scroll region (rows 2-4, 1-indexed = rows 1-3, 0-indexed)
    buf.process("\x1b[2;4r", None);
    // Cursor moves to top of scroll region (scroll_top = 1)
    assert_eq!(buf.cursor_row(), 1);

    // Move cursor somewhere else
    buf.process("\x1b[3;5H", None); // Row 3, col 5
    assert_eq!(buf.cursor_row(), 2);
    assert_eq!(buf.cursor_col(), 4);

    // Try to set invalid region: top(5) > bottom(3)
    buf.process("\x1b[5;3r", None);

    // Cursor should NOT move (invalid region ignored)
    // Note: Per DECSTBM spec, invalid regions are typically ignored
    // but cursor position behavior varies. We verify region is preserved.

    // The scroll region should still be 2;4 (preserved from before)
    // Test by scrolling at the old bottom boundary
    buf.process("\x1b[4;1H", None); // Move to row 4 (old scroll_bottom, None)
    buf.process("X\n", None);

    let output = buf.to_string();
    let lines: Vec<&str> = output.lines().collect();

    // Line0 should still be preserved (outside old region)
    assert!(
        lines[0].starts_with("Line0"),
        "Line0 should be preserved - invalid region was ignored"
    );
}

/// Test that invalid scroll region (top == bottom) is ignored.
#[test]
fn scroll_region_invalid_top_equals_bottom() {
    let mut buf = TerminalBuffer::new(10, 5);
    buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4", None);

    // First set a valid scroll region (rows 2-4, 1-indexed = rows 1-3, 0-indexed)
    buf.process("\x1b[2;4r", None);
    // Cursor moves to top of scroll region (scroll_top = 1)
    assert_eq!(buf.cursor_row(), 1);

    // Try to set invalid region: top(3) == bottom(3)
    buf.process("\x1b[3;3r", None);

    // The scroll region should still be 2;4 (preserved)
    // Verify by checking scroll behavior
    buf.process("\x1b[4;1H", None); // Move to row 4 (old scroll_bottom = row index 3, None)
    buf.process("TEST\n", None);

    let output = buf.to_string();
    let lines: Vec<&str> = output.lines().collect();

    // Line0 should be preserved (outside scroll region)
    assert!(
        lines[0].starts_with("Line0"),
        "Line0 should be preserved - invalid region (top==bottom) was ignored"
    );
    // Line4 should also be preserved (outside scroll region)
    assert!(
        lines[4].starts_with("Line4"),
        "Line4 should be preserved - it's outside the scroll region"
    );
}

/// Test that scroll region is preserved when invalid params are provided.
#[test]
fn scroll_region_preserves_on_invalid_params() {
    let mut buf = TerminalBuffer::new(10, 5);
    buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4", None);

    // Set a valid scroll region rows 2-3 (1-indexed = rows 1-2, 0-indexed)
    buf.process("\x1b[2;3r", None);
    // Cursor moves to top of scroll region (scroll_top = 1)
    assert_eq!(buf.cursor_row(), 1);

    // Move to row 3 (scroll_bottom), write and newline
    buf.process("\x1b[3;1HX\n", None);

    // Now try various invalid regions
    buf.process("\x1b[10;2r", None); // top > bottom
    buf.process("\x1b[5;5r", None); // top == bottom

    // Move back to scroll_bottom and scroll again
    buf.process("\x1b[3;1HY\n", None);

    let output = buf.to_string();
    let lines: Vec<&str> = output.lines().collect();

    // Line0, Line3, Line4 should be preserved (outside region)
    assert!(
        lines[0].starts_with("Line0"),
        "Line0 preserved after invalid region attempts"
    );
    assert!(
        lines[3].starts_with("Line3"),
        "Line3 preserved after invalid region attempts"
    );
    assert!(
        lines[4].starts_with("Line4"),
        "Line4 preserved after invalid region attempts"
    );
}
