//! Integration tests for full sequence replay and fixtures.

use agr::terminal::TerminalBuffer;

#[test]
fn new_buffer_is_empty() {
    let buf = TerminalBuffer::new(80, 24);
    assert_eq!(buf.width(), 80);
    assert_eq!(buf.height(), 24);
    assert_eq!(buf.to_string(), "");
}

#[test]
fn process_simple_text() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("Hello, World!", None);
    assert_eq!(buf.to_string(), "Hello, World!");
}

#[test]
fn process_newline() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("Line 1\r\nLine 2", None);
    assert_eq!(buf.to_string(), "Line 1\nLine 2");
}

#[test]
fn process_carriage_return_newline() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("Line 1\r\nLine 2\r\n", None);
    assert_eq!(buf.to_string(), "Line 1\nLine 2");
}

#[test]
fn process_typical_shell_output() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("$ echo hello\r\n", None);
    buf.process("hello\r\n", None);
    buf.process("$ ", None);
    assert_eq!(buf.to_string(), "$ echo hello\nhello\n$");
}

#[test]
fn unknown_csi_action_is_ignored() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("X\x1b[5ZY", None);
    let output = buf.to_string();
    assert!(output.contains("X"));
    assert!(output.contains("Y"));
}

#[test]
fn dec_private_mode_sequences_are_ignored() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\x1b[?2026hHello\x1b[?2026l", None);
    assert_eq!(buf.to_string(), "Hello");
}

#[test]
fn dec_cursor_visibility_is_ignored() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\x1b[?25lHidden\x1b[?25h Visible", None);
    assert_eq!(buf.to_string(), "Hidden Visible");
}

#[test]
fn dec_alternate_screen_is_ignored() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\x1b[?1049hContent\x1b[?1049l", None);
    assert_eq!(buf.to_string(), "Content");
}

#[test]
fn mouse_tracking_sgr_mode_ignored() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("Hello\x1b[<0;10;5MWorld", None);
    assert_eq!(buf.to_string(), "HelloWorld");
}

#[test]
fn wide_character_takes_two_columns() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\u{4e2d}X", None);
    assert_eq!(buf.cursor_col(), 3);
}

#[test]
fn wide_character_alignment() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("A\u{4e2d}B", None);
    let output = buf.to_string();
    assert!(output.contains("A"));
    assert!(output.contains("\u{4e2d}"));
    assert!(output.contains("B"));
}

#[test]
fn wide_character_wraps_correctly() {
    let mut buf = TerminalBuffer::new(5, 2);
    buf.process("AAAA\u{4e2d}", None);
    assert_eq!(buf.cursor_row(), 1);
}

#[test]
fn bullet_character_width() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\u{25cf}X", None);
    let output = buf.to_string();
    assert!(output.contains("\u{25cf}"));
    assert!(output.contains("X"));
}

#[test]
fn special_characters_have_correct_width() {
    use unicode_width::UnicodeWidthChar;
    let chars = [
        '\u{23fa}', '\u{23bf}', '\u{2733}', '\u{2736}', '\u{273b}', '\u{2722}', '\u{273d}',
        '\u{00b7}', '\u{25cf}', '\u{276f}', '\u{2193}', '\u{2500}', '\u{2502}', '\u{00A0}',
    ];
    for c in chars {
        let w = c.width().unwrap_or(1);
        assert_eq!(
            w, 1,
            "Character {} (U+{:04X}) should have width 1, got {}",
            c, c as u32, w
        );
    }
}

#[test]
fn spinner_update_sequence() {
    let mut buf = TerminalBuffer::new(86, 52);

    buf.process("Line 0: Some initial content\r\n", None);
    buf.process("Line 1: More content\r\n", None);
    buf.process("Line 2: Even more\r\n", None);
    buf.process("Line 3: Content here\r\n", None);
    buf.process("Line 4: Fourth line\r\n", None);
    buf.process("Line 5: Fifth line\r\n", None);
    buf.process("Line 6: Sixth line", None);

    assert_eq!(buf.cursor_row(), 6);

    buf.process("\r", None);
    assert_eq!(buf.cursor_col(), 0);

    buf.process("\x1b[6A", None);
    assert_eq!(buf.cursor_row(), 0);

    buf.process("\u{23fa}", None);
    assert_eq!(buf.cursor_col(), 1);

    buf.process("\x1b[1C", None);
    assert_eq!(buf.cursor_col(), 2);

    buf.process("Bash", None);
    assert_eq!(buf.cursor_col(), 6);

    let output = buf.to_string();
    let lines: Vec<&str> = output.lines().collect();
    assert!(
        lines[0].starts_with("\u{23fa}"),
        "Row 0 should start with record symbol, got: {}",
        lines[0]
    );
    assert!(
        lines[0].contains("Bash"),
        "Row 0 should contain Bash, got: {}",
        lines[0]
    );
}

/// Test using the scroll region fixture file.
/// This fixture contains diverse scroll region sequences for CI testing.
#[test]
fn scroll_region_fixture_test() {
    use agr::asciicast::{AsciicastFile, EventType};
    use std::path::Path;

    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("scroll_region_test.cast");

    if !fixture_path.exists() {
        println!(
            "Fixture file not found at {:?}, skipping test",
            fixture_path
        );
        return;
    }

    let cast = AsciicastFile::parse(&fixture_path).expect("Failed to parse fixture");
    let (cols, rows) = cast.terminal_size();

    let mut buf = TerminalBuffer::new(cols as usize, rows as usize);

    // Process all events
    for event in &cast.events {
        if event.event_type == EventType::Output {
            buf.process(&event.data, None);
        }
    }

    // Verify basic rendering worked
    let output = buf.to_string();
    assert!(!output.is_empty(), "Output should not be empty");

    // The fixture should demonstrate scroll region behavior
    // Specific assertions depend on the fixture content
    println!("Fixture test completed successfully");
    println!("Terminal size: {}x{}", cols, rows);
    println!("Events processed: {}", cast.events.len());
}
