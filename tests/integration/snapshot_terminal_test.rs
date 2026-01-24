//! Snapshot tests for terminal buffer (native player core)
//!
//! Tests ANSI escape sequence handling and terminal rendering.

use agr::player::TerminalBuffer;

/// Helper to create a terminal buffer and process input
fn process(input: &str) -> String {
    let mut buf = TerminalBuffer::new(40, 10);
    buf.process(input);
    buf.to_string()
}

/// Helper with custom dimensions
fn process_sized(input: &str, cols: usize, rows: usize) -> String {
    let mut buf = TerminalBuffer::new(cols, rows);
    buf.process(input);
    buf.to_string()
}

// ============================================================================
// Basic text output
// ============================================================================

#[test]
fn terminal_simple_text() {
    insta::assert_snapshot!(process("Hello, World!"));
}

#[test]
fn terminal_multiline_text() {
    insta::assert_snapshot!(process("Line 1\nLine 2\nLine 3"));
}

#[test]
fn terminal_text_with_carriage_return() {
    insta::assert_snapshot!(process("XXXX\rHello"));
}

#[test]
fn terminal_text_wrap_at_boundary() {
    // 40 columns, text should wrap
    insta::assert_snapshot!(process(
        "This is a line that is longer than forty characters wide"
    ));
}

// ============================================================================
// Cursor movement
// ============================================================================

#[test]
fn terminal_cursor_up() {
    insta::assert_snapshot!(process("Line1\nLine2\nLine3\x1b[2AX"));
}

#[test]
fn terminal_cursor_down() {
    insta::assert_snapshot!(process("Line1\x1b[2BX"));
}

#[test]
fn terminal_cursor_forward() {
    insta::assert_snapshot!(process("Hello\x1b[5CWorld"));
}

#[test]
fn terminal_cursor_back() {
    insta::assert_snapshot!(process("Hello World\x1b[5DX"));
}

#[test]
fn terminal_cursor_position() {
    insta::assert_snapshot!(process("\x1b[3;10HX\x1b[5;20HY\x1b[1;1HZ"));
}

#[test]
fn terminal_cursor_home() {
    insta::assert_snapshot!(process("Hello\x1b[HX"));
}

#[test]
fn terminal_cursor_column() {
    insta::assert_snapshot!(process("0123456789\x1b[5GX"));
}

// ============================================================================
// Erase operations
// ============================================================================

#[test]
fn terminal_erase_to_end_of_line() {
    insta::assert_snapshot!(process("Hello World\x1b[6G\x1b[K"));
}

#[test]
fn terminal_erase_to_start_of_line() {
    insta::assert_snapshot!(process("Hello World\x1b[6G\x1b[1K"));
}

#[test]
fn terminal_erase_entire_line() {
    insta::assert_snapshot!(process("Hello World\x1b[2K"));
}

#[test]
fn terminal_erase_to_end_of_screen() {
    insta::assert_snapshot!(process("Line1\nLine2\nLine3\n\x1b[2;1H\x1b[J"));
}

#[test]
fn terminal_erase_to_start_of_screen() {
    insta::assert_snapshot!(process("Line1\nLine2\nLine3\n\x1b[2;3H\x1b[1J"));
}

#[test]
fn terminal_erase_entire_screen() {
    insta::assert_snapshot!(process("Line1\nLine2\nLine3\x1b[2J"));
}

// ============================================================================
// Colors and styles (SGR)
// ============================================================================

#[test]
fn terminal_bold_text() {
    // Bold is SGR 1, we test the buffer contains the text
    insta::assert_snapshot!(process("\x1b[1mBold\x1b[0m Normal"));
}

#[test]
fn terminal_colored_text() {
    // Red foreground (31), green (32), blue (34)
    insta::assert_snapshot!(process("\x1b[31mRed\x1b[32mGreen\x1b[34mBlue\x1b[0m"));
}

#[test]
fn terminal_background_color() {
    insta::assert_snapshot!(process("\x1b[41mRedBG\x1b[42mGreenBG\x1b[0m"));
}

#[test]
fn terminal_256_color() {
    insta::assert_snapshot!(process("\x1b[38;5;196mColor196\x1b[0m"));
}

#[test]
fn terminal_rgb_color() {
    insta::assert_snapshot!(process("\x1b[38;2;255;128;0mOrange\x1b[0m"));
}

#[test]
fn terminal_reverse_video() {
    insta::assert_snapshot!(process("\x1b[7mReversed\x1b[27mNormal"));
}

// ============================================================================
// Scrolling
// ============================================================================

#[test]
fn terminal_scroll_up() {
    insta::assert_snapshot!(process_sized("L1\nL2\nL3\nL4\nL5\nL6", 20, 5));
}

#[test]
fn terminal_newline_scroll() {
    // Fill screen then add more lines
    let mut input = String::new();
    for i in 1..=12 {
        input.push_str(&format!("Line{}\n", i));
    }
    insta::assert_snapshot!(process_sized(&input, 20, 10));
}

// ============================================================================
// Resize handling
// ============================================================================

#[test]
fn terminal_resize_wider() {
    let mut buf = TerminalBuffer::new(20, 5);
    buf.process("Hello\nWorld");
    buf.resize(40, 5);
    buf.process(" - extended");
    insta::assert_snapshot!(buf.to_string());
}

#[test]
fn terminal_resize_narrower() {
    let mut buf = TerminalBuffer::new(40, 5);
    buf.process("This is a long line of text");
    buf.resize(20, 5);
    insta::assert_snapshot!(buf.to_string());
}

#[test]
fn terminal_resize_taller() {
    let mut buf = TerminalBuffer::new(20, 3);
    buf.process("Line1\nLine2\nLine3");
    buf.resize(20, 6);
    buf.process("\nLine4\nLine5");
    insta::assert_snapshot!(buf.to_string());
}

#[test]
fn terminal_resize_shorter() {
    let mut buf = TerminalBuffer::new(20, 6);
    buf.process("Line1\nLine2\nLine3\nLine4\nLine5\nLine6");
    buf.resize(20, 3);
    insta::assert_snapshot!(buf.to_string());
}

// ============================================================================
// Unicode and wide characters
// ============================================================================

#[test]
fn terminal_unicode_emoji() {
    insta::assert_snapshot!(process("Hello 🎉 World"));
}

#[test]
fn terminal_unicode_cjk() {
    insta::assert_snapshot!(process("Hello 中文 World"));
}

#[test]
fn terminal_unicode_symbols() {
    insta::assert_snapshot!(process("Progress: ▓▓▓▓░░░░ 50%"));
}

// ============================================================================
// DEC private modes and special sequences
// ============================================================================

#[test]
fn terminal_dec_save_restore_cursor() {
    // ESC 7 saves, ESC 8 restores
    insta::assert_snapshot!(process("Hello\x1b7 World\x1b8X"));
}

#[test]
fn terminal_csi_save_restore_cursor() {
    // CSI s saves, CSI u restores
    insta::assert_snapshot!(process("Hello\x1b[s World\x1b[uX"));
}

#[test]
fn terminal_reverse_index() {
    // ESC M moves cursor up, scrolling if at top
    insta::assert_snapshot!(process("Line1\nLine2\x1b[1;1H\x1bMNew"));
}

// ============================================================================
// Complex sequences (real-world patterns)
// ============================================================================

#[test]
fn terminal_prompt_pattern() {
    // Typical shell prompt with colors
    insta::assert_snapshot!(process(
        "\x1b[32muser\x1b[0m@\x1b[34mhost\x1b[0m:\x1b[33m~/dir\x1b[0m$ "
    ));
}

#[test]
fn terminal_progress_bar_update() {
    // Progress bar that updates in place
    let mut buf = TerminalBuffer::new(40, 3);
    buf.process("Downloading...\n[          ] 0%");
    buf.process("\r[##        ] 20%");
    buf.process("\r[#####     ] 50%");
    buf.process("\r[##########] 100%\nDone!");
    insta::assert_snapshot!(buf.to_string());
}

#[test]
fn terminal_cursor_movement_zero_params() {
    // Zero params should be treated as 1
    insta::assert_snapshot!(process("ABCDE\x1b[0DXYZ"));
}

#[test]
fn terminal_synchronized_update_mode() {
    // DEC private mode 2026 (synchronized update) should be ignored
    insta::assert_snapshot!(process("\x1b[?2026hHello\x1b[?2026l World"));
}
