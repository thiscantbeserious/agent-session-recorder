//! Scroll region tests.

use crate::terminal::TerminalBuffer;

#[test]
fn scroll_when_full() {
    let mut buf = TerminalBuffer::new(10, 3);
    buf.process("Line 1\r\nLine 2\r\nLine 3\r\nLine 4");
    assert_eq!(buf.to_string(), "Line 2\nLine 3\nLine 4");
}

#[test]
fn reverse_index_moves_cursor_up() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("Line1\r\nLine2");
    buf.process("\x1bM");
    buf.process("X");
    assert_eq!(buf.to_string(), "Line1X\nLine2");
}

#[test]
fn reverse_index_scrolls_at_top() {
    let mut buf = TerminalBuffer::new(10, 3);
    buf.process("Line1\r\nLine2\r\nLine3");
    buf.process("\x1b[1;1H");
    buf.process("\x1bM");
    buf.process("New");
    assert_eq!(buf.to_string(), "New\nLine1\nLine2");
}

#[test]
fn scroll_region_basic_setup() {
    let mut buf = TerminalBuffer::new(10, 5);
    buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4");

    buf.process("\x1b[2;4r");

    assert_eq!(buf.cursor_row(), 0);
    assert_eq!(buf.cursor_col(), 0);
}

#[test]
fn scroll_region_scroll_within_region() {
    let mut buf = TerminalBuffer::new(10, 5);
    buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4");

    buf.process("\x1b[2;4r");
    buf.process("\x1b[4;1H");
    assert_eq!(buf.cursor_row(), 3);

    buf.process("\n");

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
    buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4");

    buf.process("\x1b[2;4r");
    buf.process("\x1b[2;1H");
    assert_eq!(buf.cursor_row(), 1);

    buf.process("\x1bM");

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
    buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4");

    buf.process("\x1b[2;4r");
    buf.process("\x1b[1S");

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
    buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4");

    buf.process("\x1b[2;4r");
    buf.process("\x1b[1T");

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

    buf.process("\x1b[2;4r");
    buf.resize(10, 10);

    buf.process("\x1b[10;1H");
    buf.process("Last\n");

    assert_eq!(buf.height(), 10);
}

#[test]
fn scroll_region_full_screen_default() {
    let mut buf = TerminalBuffer::new(10, 5);
    buf.process("L0\r\nL1\r\nL2\r\nL3\r\nL4");

    buf.process("\x1b[5;1H");
    buf.process("\nL5");

    let output = buf.to_string();
    let lines: Vec<&str> = output.lines().collect();

    assert!(!lines[0].starts_with("L0"), "L0 should have scrolled off");
    assert!(lines[4].starts_with("L5"), "L5 should be at bottom");
}

#[test]
fn scroll_region_reset_via_csi_r() {
    let mut buf = TerminalBuffer::new(10, 5);
    buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4");

    buf.process("\x1b[2;4r");
    buf.process("\x1b[r");

    buf.process("\x1b[5;1H\n");

    let output = buf.to_string();
    let lines: Vec<&str> = output.lines().collect();

    assert!(
        !lines[0].starts_with("Line0"),
        "Line0 should have scrolled off"
    );
}
