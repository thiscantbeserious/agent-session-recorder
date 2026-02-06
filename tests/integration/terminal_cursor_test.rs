//! Cursor movement and positioning tests.

use agr::terminal::TerminalBuffer;

#[test]
fn cursor_movement_up() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("Line 1\r\nLine 2", None);
    assert_eq!(buf.cursor_row(), 1, "cursor row should be 1 after Line 2");
    assert_eq!(buf.cursor_col(), 6, "cursor col should be 6 after Line 2");

    buf.process("\x1b[AX", None);
    assert_eq!(buf.to_string(), "Line 1X\nLine 2");
}

#[test]
fn cursor_position_absolute() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("Hello\x1b[1;3HX", None);
    assert_eq!(buf.to_string(), "HeXlo");
}

#[test]
fn cursor_down_moves_cursor() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("Line1\x1b[BX", None);
    let output = buf.to_string();
    assert!(output.contains("Line1"));
    assert!(output.contains("X"));
}

#[test]
fn cursor_forward_moves_cursor() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("A\x1b[3CB", None);
    let output = buf.to_string();
    assert!(output.starts_with("A"));
    assert!(output.ends_with("B"));
}

#[test]
fn cursor_back_moves_cursor() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("ABCD\x1b[2DX", None);
    assert_eq!(buf.to_string(), "ABXD");
}

#[test]
fn cursor_position_with_f_command() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("Hello\x1b[1;3fX", None);
    assert_eq!(buf.to_string(), "HeXlo");
}

#[test]
fn cursor_forward_zero_param_moves_one() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("A\x1b[0CB", None);
    assert_eq!(buf.to_string(), "A B");
}

#[test]
fn cursor_down_zero_param_moves_one() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("A\x1b[0BB", None);
    assert_eq!(buf.cursor_row(), 1);
}

#[test]
fn cursor_back_zero_param_moves_one() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("ABC\x1b[0DX", None);
    assert_eq!(buf.to_string(), "ABX");
}

#[test]
fn cursor_up_zero_param_moves_one() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("A\r\nB\x1b[0AX", None);
    assert_eq!(buf.to_string(), "AX\nB");
}

#[test]
fn dec_save_restore_cursor() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("Hello\x1b7", None);
    buf.process("\r\nWorld", None);
    buf.process("\x1b8", None);
    buf.process("!", None);
    assert_eq!(buf.to_string(), "Hello!\nWorld");
}

#[test]
fn dec_restore_without_save_does_nothing() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("Hello", None);
    buf.process("\x1b8", None);
    buf.process("X", None);
    assert_eq!(buf.to_string(), "HelloX");
}

#[test]
fn csi_save_restore_cursor() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("Hello\x1b[s", None);
    buf.process("\r\nWorld", None);
    buf.process("\x1b[u", None);
    buf.process("!", None);
    assert_eq!(buf.to_string(), "Hello!\nWorld");
}

#[test]
fn backspace_moves_cursor_back() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("AB\x08C", None);
    assert_eq!(buf.to_string(), "AC");
}

#[test]
fn backspace_at_start_does_nothing() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\x08X", None);
    assert_eq!(buf.to_string(), "X");
}

#[test]
fn tab_moves_to_next_tab_stop() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("A\tB", None);
    let output = buf.to_string();
    assert!(output.starts_with("A"));
    assert!(output.contains("B"));
    assert!(output.len() >= 8);
}
