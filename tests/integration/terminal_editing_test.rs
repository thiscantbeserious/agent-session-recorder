//! Erase and delete operation tests.

use agr::terminal::TerminalBuffer;

#[test]
fn erase_to_end_of_line() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("Hello World\x1b[1;6H\x1b[K");
    assert_eq!(buf.to_string(), "Hello");
}

#[test]
fn erase_line_clears_entire_line() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("Hello World\x1b[1;6H\x1b[2K");
    assert_eq!(buf.to_string(), "");
}

#[test]
fn erase_to_end_of_screen() {
    let mut buf = TerminalBuffer::new(10, 3);
    buf.process("Line1\r\nLine2\r\nLine3");
    buf.process("\x1b[2;1H\x1b[0J");
    assert_eq!(buf.to_string(), "Line1");
}

#[test]
fn clear_screen() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("Hello\nWorld\x1b[2J");
    assert_eq!(buf.to_string(), "");
}

#[test]
fn line_wrap_at_width() {
    let mut buf = TerminalBuffer::new(10, 3);
    buf.process("1234567890ABC");
    assert_eq!(buf.to_string(), "1234567890\nABC");
}

#[test]
fn process_carriage_return_overwrites() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("Hello\rWorld");
    assert_eq!(buf.to_string(), "World");
}

#[test]
fn resize_preserves_content() {
    let mut buf = TerminalBuffer::new(10, 3);
    buf.process("Hello\r\nWorld");

    buf.resize(20, 5);
    assert_eq!(buf.width(), 20);
    assert_eq!(buf.height(), 5);

    let output = buf.to_string();
    assert!(output.contains("Hello"), "Should preserve Hello");
    assert!(output.contains("World"), "Should preserve World");
}

#[test]
fn resize_truncates_content() {
    let mut buf = TerminalBuffer::new(10, 3);
    buf.process("Hello World");

    buf.resize(5, 3);
    assert_eq!(buf.width(), 5);

    let output = buf.to_string();
    assert!(output.contains("Hello"), "Should contain Hello");
    assert!(!output.contains("World"), "Should truncate World");
}

#[test]
fn resize_clamps_cursor() {
    let mut buf = TerminalBuffer::new(20, 10);
    buf.process("Test\r\n\r\n\r\n\r\n\r\nEnd");

    assert_eq!(buf.cursor_row(), 5);

    buf.resize(20, 3);
    assert_eq!(buf.cursor_row(), 2);
}

#[test]
fn resize_during_playback_sequence() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("Initial content at 80 cols");
    assert_eq!(buf.width(), 80);

    buf.resize(100, 24);
    assert_eq!(buf.width(), 100);

    buf.process("\r\n");
    buf.process("This is wider content that uses the new 100 column width...............");

    let output = buf.to_string();
    assert!(output.contains("Initial content"));
    assert!(output.contains("This is wider content"));
}
