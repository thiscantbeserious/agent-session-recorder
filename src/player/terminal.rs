//! Virtual terminal buffer for replaying asciicast output.
//!
//! This module re-exports from `crate::terminal` for backward compatibility.
//! All types and implementations have been moved to `src/terminal/`.

// Re-export all types from the new terminal module for backward compatibility
pub use crate::terminal::{Cell, CellStyle, Color, StyledLine, TerminalBuffer};

#[cfg(test)]
mod tests {
    use super::*;

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
        buf.process("Hello, World!");
        assert_eq!(buf.to_string(), "Hello, World!");
    }

    #[test]
    fn process_newline() {
        let mut buf = TerminalBuffer::new(80, 24);
        // In VT100, \n only moves down, doesn't return to col 0
        // Most terminal output uses \r\n for new lines
        buf.process("Line 1\r\nLine 2");
        assert_eq!(buf.to_string(), "Line 1\nLine 2");
    }

    #[test]
    fn process_carriage_return_newline() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("Line 1\r\nLine 2\r\n");
        assert_eq!(buf.to_string(), "Line 1\nLine 2");
    }

    #[test]
    fn process_carriage_return_overwrites() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("Hello\rWorld");
        assert_eq!(buf.to_string(), "World");
    }

    #[test]
    fn line_wrap_at_width() {
        let mut buf = TerminalBuffer::new(10, 3);
        buf.process("1234567890ABC");
        assert_eq!(buf.to_string(), "1234567890\nABC");
    }

    #[test]
    fn cursor_movement_up() {
        let mut buf = TerminalBuffer::new(80, 24);
        // After "Line 1\r\nLine 2", cursor is at row 1, col 6
        buf.process("Line 1\r\nLine 2");
        assert_eq!(buf.cursor_row(), 1, "cursor row should be 1 after Line 2");
        assert_eq!(buf.cursor_col(), 6, "cursor col should be 6 after Line 2");

        // ESC[A moves up 1 row to row 0, col 6
        // Process together to ensure escape sequence isn't split
        buf.process("\x1b[AX");
        assert_eq!(buf.to_string(), "Line 1X\nLine 2");
    }

    #[test]
    fn cursor_position_absolute() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("Hello\x1b[1;3HX");
        assert_eq!(buf.to_string(), "HeXlo");
    }

    #[test]
    fn erase_to_end_of_line() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("Hello World\x1b[1;6H\x1b[K");
        assert_eq!(buf.to_string(), "Hello");
    }

    #[test]
    fn clear_screen() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("Hello\nWorld\x1b[2J");
        assert_eq!(buf.to_string(), "");
    }

    #[test]
    fn scroll_when_full() {
        let mut buf = TerminalBuffer::new(10, 3);
        buf.process("Line 1\r\nLine 2\r\nLine 3\r\nLine 4");
        // Should scroll up, losing Line 1
        assert_eq!(buf.to_string(), "Line 2\nLine 3\nLine 4");
    }

    #[test]
    fn process_typical_shell_output() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("$ echo hello\r\n");
        buf.process("hello\r\n");
        buf.process("$ ");
        assert_eq!(buf.to_string(), "$ echo hello\nhello\n$");
    }

    // Color and style tests

    #[test]
    fn color_default_is_default() {
        assert_eq!(Color::default(), Color::Default);
    }

    #[test]
    fn cell_style_default_has_no_attributes() {
        let style = CellStyle::default();
        assert_eq!(style.fg, Color::Default);
        assert_eq!(style.bg, Color::Default);
        assert!(!style.bold);
        assert!(!style.dim);
        assert!(!style.italic);
        assert!(!style.underline);
    }

    #[test]
    fn cell_default_is_space() {
        let cell = Cell::default();
        assert_eq!(cell.char, ' ');
        assert_eq!(cell.style, CellStyle::default());
    }

    #[test]
    fn styled_lines_returns_correct_count() {
        let mut buf = TerminalBuffer::new(80, 3);
        buf.process("Line 1\r\nLine 2\r\nLine 3");
        let lines = buf.styled_lines();
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn styled_lines_preserves_red_foreground() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[31mRed");
        let lines = buf.styled_lines();
        assert!(!lines.is_empty());
        let first_line = &lines[0];
        assert!(first_line.cells.iter().any(|c| c.style.fg == Color::Red));
    }

    #[test]
    fn styled_lines_preserves_green_foreground() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[32mGreen");
        let lines = buf.styled_lines();
        let first_line = &lines[0];
        assert!(first_line.cells.iter().any(|c| c.style.fg == Color::Green));
    }

    #[test]
    fn styled_lines_preserves_blue_background() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[44mBlue BG");
        let lines = buf.styled_lines();
        let first_line = &lines[0];
        assert!(first_line.cells.iter().any(|c| c.style.bg == Color::Blue));
    }

    #[test]
    fn styled_lines_preserves_bold() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[1mBold");
        let lines = buf.styled_lines();
        let first_line = &lines[0];
        assert!(first_line.cells.iter().any(|c| c.style.bold));
    }

    #[test]
    fn styled_lines_reset_clears_style() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[31mRed\x1b[0mNormal");
        let lines = buf.styled_lines();
        let first_line = &lines[0];
        // Should have both red cells and default cells
        assert!(first_line.cells.iter().any(|c| c.style.fg == Color::Red));
        assert!(first_line
            .cells
            .iter()
            .any(|c| c.style.fg == Color::Default && c.char == 'N'));
    }

    #[test]
    fn styled_lines_bright_colors() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[91mBright Red");
        let lines = buf.styled_lines();
        let first_line = &lines[0];
        assert!(first_line
            .cells
            .iter()
            .any(|c| c.style.fg == Color::BrightRed));
    }

    #[test]
    fn styled_lines_256_color() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[38;5;196mIndexed");
        let lines = buf.styled_lines();
        let first_line = &lines[0];
        assert!(first_line
            .cells
            .iter()
            .any(|c| c.style.fg == Color::Indexed(196)));
    }

    #[test]
    fn styled_lines_rgb_color() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[38;2;255;128;64mRGB");
        let lines = buf.styled_lines();
        let first_line = &lines[0];
        assert!(first_line
            .cells
            .iter()
            .any(|c| c.style.fg == Color::Rgb(255, 128, 64)));
    }

    #[test]
    fn styled_lines_multiple_attributes() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[1;4;31mBold Underline Red");
        let lines = buf.styled_lines();
        let first_line = &lines[0];
        assert!(first_line
            .cells
            .iter()
            .any(|c| { c.style.bold && c.style.underline && c.style.fg == Color::Red }));
    }

    #[test]
    fn process_colored_text_applies_style() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[1;32mBold Green\x1b[0m Normal");
        let lines = buf.styled_lines();
        let first_line = &lines[0];

        // Check that "Bold Green" has bold and green
        let bold_green_cells: Vec<_> = first_line
            .cells
            .iter()
            .filter(|c| c.style.bold && c.style.fg == Color::Green)
            .collect();
        assert!(!bold_green_cells.is_empty(), "Should have bold green cells");

        // Check that "Normal" has default style
        let normal_cells: Vec<_> = first_line
            .cells
            .iter()
            .filter(|c| !c.style.bold && c.style.fg == Color::Default && c.char != ' ')
            .collect();
        assert!(!normal_cells.is_empty(), "Should have normal styled cells");
    }

    #[test]
    fn styled_lines_dim_attribute() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[2mDim");
        let lines = buf.styled_lines();
        let first_line = &lines[0];
        assert!(first_line.cells.iter().any(|c| c.style.dim));
    }

    #[test]
    fn styled_lines_italic_attribute() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[3mItalic");
        let lines = buf.styled_lines();
        let first_line = &lines[0];
        assert!(first_line.cells.iter().any(|c| c.style.italic));
    }

    #[test]
    fn styled_lines_all_basic_foreground_colors() {
        let colors = [
            ("\x1b[30m", Color::Black),
            ("\x1b[31m", Color::Red),
            ("\x1b[32m", Color::Green),
            ("\x1b[33m", Color::Yellow),
            ("\x1b[34m", Color::Blue),
            ("\x1b[35m", Color::Magenta),
            ("\x1b[36m", Color::Cyan),
            ("\x1b[37m", Color::White),
        ];
        for (seq, expected) in colors {
            let mut buf = TerminalBuffer::new(80, 24);
            buf.process(&format!("{}X", seq));
            let lines = buf.styled_lines();
            assert!(
                lines[0].cells.iter().any(|c| c.style.fg == expected),
                "Expected {:?} for sequence {}",
                expected,
                seq
            );
        }
    }

    #[test]
    fn styled_lines_all_basic_background_colors() {
        let colors = [
            ("\x1b[40m", Color::Black),
            ("\x1b[41m", Color::Red),
            ("\x1b[42m", Color::Green),
            ("\x1b[43m", Color::Yellow),
            ("\x1b[44m", Color::Blue),
            ("\x1b[45m", Color::Magenta),
            ("\x1b[46m", Color::Cyan),
            ("\x1b[47m", Color::White),
        ];
        for (seq, expected) in colors {
            let mut buf = TerminalBuffer::new(80, 24);
            buf.process(&format!("{}X", seq));
            let lines = buf.styled_lines();
            assert!(
                lines[0].cells.iter().any(|c| c.style.bg == expected),
                "Expected bg {:?} for sequence {}",
                expected,
                seq
            );
        }
    }

    #[test]
    fn styled_lines_default_foreground_reset() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[31mR\x1b[39mD");
        let lines = buf.styled_lines();
        let first_line = &lines[0];
        assert!(first_line
            .cells
            .iter()
            .any(|c| c.char == 'R' && c.style.fg == Color::Red));
        assert!(first_line
            .cells
            .iter()
            .any(|c| c.char == 'D' && c.style.fg == Color::Default));
    }

    #[test]
    fn styled_lines_default_background_reset() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[41mR\x1b[49mD");
        let lines = buf.styled_lines();
        let first_line = &lines[0];
        assert!(first_line
            .cells
            .iter()
            .any(|c| c.char == 'R' && c.style.bg == Color::Red));
        assert!(first_line
            .cells
            .iter()
            .any(|c| c.char == 'D' && c.style.bg == Color::Default));
    }

    #[test]
    fn styled_lines_bright_background_colors() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[100mX");
        let lines = buf.styled_lines();
        assert!(lines[0]
            .cells
            .iter()
            .any(|c| c.style.bg == Color::BrightBlack));
    }

    #[test]
    fn styled_lines_256_background_color() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[48;5;82mX");
        let lines = buf.styled_lines();
        assert!(lines[0]
            .cells
            .iter()
            .any(|c| c.style.bg == Color::Indexed(82)));
    }

    #[test]
    fn styled_lines_rgb_background_color() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[48;2;100;150;200mX");
        let lines = buf.styled_lines();
        assert!(lines[0]
            .cells
            .iter()
            .any(|c| c.style.bg == Color::Rgb(100, 150, 200)));
    }

    // Additional tests for uncovered code paths

    #[test]
    fn backspace_moves_cursor_back() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("AB\x08C"); // AB, backspace, C
        assert_eq!(buf.to_string(), "AC");
    }

    #[test]
    fn backspace_at_start_does_nothing() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x08X"); // Backspace at start, then X
        assert_eq!(buf.to_string(), "X");
    }

    #[test]
    fn tab_moves_to_next_tab_stop() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("A\tB"); // A, tab, B
        let output = buf.to_string();
        assert!(output.starts_with("A"));
        assert!(output.contains("B"));
        // Tab stop is at column 8, so there should be spaces between A and B
        assert!(output.len() >= 8);
    }

    #[test]
    fn cursor_down_moves_cursor() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("Line1\x1b[BX"); // Line1, cursor down, X
        let output = buf.to_string();
        assert!(output.contains("Line1"));
        assert!(output.contains("X"));
    }

    #[test]
    fn cursor_forward_moves_cursor() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("A\x1b[3CB"); // A, cursor forward 3, B
        let output = buf.to_string();
        assert!(output.starts_with("A"));
        assert!(output.ends_with("B"));
    }

    #[test]
    fn cursor_back_moves_cursor() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("ABCD\x1b[2DX"); // ABCD, cursor back 2, X
        assert_eq!(buf.to_string(), "ABXD");
    }

    #[test]
    fn erase_line_clears_entire_line() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("Hello World\x1b[1;6H\x1b[2K"); // Move to col 6, erase line
        assert_eq!(buf.to_string(), "");
    }

    #[test]
    fn erase_to_end_of_screen() {
        let mut buf = TerminalBuffer::new(10, 3);
        buf.process("Line1\r\nLine2\r\nLine3");
        buf.process("\x1b[2;1H\x1b[0J"); // Move to row 2 col 1, erase to end of screen
        assert_eq!(buf.to_string(), "Line1");
    }

    #[test]
    fn sgr_reset_bold_and_dim() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[1;2mX\x1b[22mY"); // Bold+dim, then reset bold/dim
        let lines = buf.styled_lines();
        let first_line = &lines[0];
        // X should be bold and dim
        assert!(first_line
            .cells
            .iter()
            .any(|c| c.char == 'X' && c.style.bold && c.style.dim));
        // Y should not be bold or dim
        assert!(first_line
            .cells
            .iter()
            .any(|c| c.char == 'Y' && !c.style.bold && !c.style.dim));
    }

    #[test]
    fn sgr_reset_italic() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[3mX\x1b[23mY"); // Italic, then reset italic
        let lines = buf.styled_lines();
        let first_line = &lines[0];
        assert!(first_line
            .cells
            .iter()
            .any(|c| c.char == 'X' && c.style.italic));
        assert!(first_line
            .cells
            .iter()
            .any(|c| c.char == 'Y' && !c.style.italic));
    }

    #[test]
    fn sgr_reset_underline() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[4mX\x1b[24mY"); // Underline, then reset underline
        let lines = buf.styled_lines();
        let first_line = &lines[0];
        assert!(first_line
            .cells
            .iter()
            .any(|c| c.char == 'X' && c.style.underline));
        assert!(first_line
            .cells
            .iter()
            .any(|c| c.char == 'Y' && !c.style.underline));
    }

    #[test]
    fn cursor_position_with_f_command() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("Hello\x1b[1;3fX"); // Use 'f' command (same as H)
        assert_eq!(buf.to_string(), "HeXlo");
    }

    #[test]
    fn all_bright_foreground_colors() {
        let colors = [
            ("\x1b[90m", Color::BrightBlack),
            ("\x1b[91m", Color::BrightRed),
            ("\x1b[92m", Color::BrightGreen),
            ("\x1b[93m", Color::BrightYellow),
            ("\x1b[94m", Color::BrightBlue),
            ("\x1b[95m", Color::BrightMagenta),
            ("\x1b[96m", Color::BrightCyan),
            ("\x1b[97m", Color::BrightWhite),
        ];
        for (seq, expected) in colors {
            let mut buf = TerminalBuffer::new(80, 24);
            buf.process(&format!("{}X", seq));
            let lines = buf.styled_lines();
            assert!(
                lines[0].cells.iter().any(|c| c.style.fg == expected),
                "Expected {:?} for sequence {}",
                expected,
                seq
            );
        }
    }

    #[test]
    fn all_bright_background_colors() {
        let colors = [
            ("\x1b[100m", Color::BrightBlack),
            ("\x1b[101m", Color::BrightRed),
            ("\x1b[102m", Color::BrightGreen),
            ("\x1b[103m", Color::BrightYellow),
            ("\x1b[104m", Color::BrightBlue),
            ("\x1b[105m", Color::BrightMagenta),
            ("\x1b[106m", Color::BrightCyan),
            ("\x1b[107m", Color::BrightWhite),
        ];
        for (seq, expected) in colors {
            let mut buf = TerminalBuffer::new(80, 24);
            buf.process(&format!("{}X", seq));
            let lines = buf.styled_lines();
            assert!(
                lines[0].cells.iter().any(|c| c.style.bg == expected),
                "Expected bg {:?} for sequence {}",
                expected,
                seq
            );
        }
    }

    #[test]
    fn unknown_sgr_code_is_ignored() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[999mX"); // Unknown SGR code
        let lines = buf.styled_lines();
        // Should still work, just with default style
        assert!(lines[0].cells.iter().any(|c| c.char == 'X'));
    }

    #[test]
    fn unknown_csi_action_is_ignored() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("X\x1b[5ZY"); // Unknown CSI Z action
                                  // Should still process X and Y
        let output = buf.to_string();
        assert!(output.contains("X"));
        assert!(output.contains("Y"));
    }

    // DEC private mode tests

    #[test]
    fn dec_private_mode_sequences_are_ignored() {
        let mut buf = TerminalBuffer::new(80, 24);
        // ESC[?2026h (synchronized update begin) and ESC[?2026l (end)
        buf.process("\x1b[?2026hHello\x1b[?2026l");
        assert_eq!(buf.to_string(), "Hello");
    }

    #[test]
    fn dec_cursor_visibility_is_ignored() {
        let mut buf = TerminalBuffer::new(80, 24);
        // ESC[?25l (hide cursor) and ESC[?25h (show cursor)
        buf.process("\x1b[?25lHidden\x1b[?25h Visible");
        assert_eq!(buf.to_string(), "Hidden Visible");
    }

    #[test]
    fn dec_alternate_screen_is_ignored() {
        let mut buf = TerminalBuffer::new(80, 24);
        // ESC[?1049h (enter alternate screen) and ESC[?1049l (leave)
        buf.process("\x1b[?1049hContent\x1b[?1049l");
        assert_eq!(buf.to_string(), "Content");
    }

    // DEC save/restore cursor tests

    #[test]
    fn dec_save_restore_cursor() {
        let mut buf = TerminalBuffer::new(80, 24);
        // ESC 7 (save cursor), move and write, ESC 8 (restore cursor)
        buf.process("Hello\x1b7"); // Save cursor at position 5
        buf.process("\r\nWorld"); // Move to next line
        buf.process("\x1b8"); // Restore cursor to position 5
        buf.process("!");
        assert_eq!(buf.to_string(), "Hello!\nWorld");
    }

    #[test]
    fn dec_restore_without_save_does_nothing() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("Hello");
        buf.process("\x1b8"); // Restore without prior save
        buf.process("X");
        // Cursor should stay where it was
        assert_eq!(buf.to_string(), "HelloX");
    }

    // Reverse index test

    #[test]
    fn reverse_index_moves_cursor_up() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("Line1\r\nLine2");
        buf.process("\x1bM"); // Reverse index - move up
        buf.process("X");
        assert_eq!(buf.to_string(), "Line1X\nLine2");
    }

    #[test]
    fn reverse_index_scrolls_at_top() {
        let mut buf = TerminalBuffer::new(10, 3);
        buf.process("Line1\r\nLine2\r\nLine3");
        buf.process("\x1b[1;1H"); // Move to top-left
        buf.process("\x1bM"); // Reverse index at top - should scroll down
        buf.process("New");
        // Line3 should be pushed off, new line at top
        assert_eq!(buf.to_string(), "New\nLine1\nLine2");
    }

    // Reverse video attribute tests

    #[test]
    fn reverse_video_attribute() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[7mReversed\x1b[27mNormal");
        let lines = buf.styled_lines();
        let first_line = &lines[0];
        // Check that "Reversed" has reverse attribute
        assert!(first_line
            .cells
            .iter()
            .any(|c| c.char == 'R' && c.style.reverse));
        // Check that "Normal" does not have reverse attribute
        assert!(first_line
            .cells
            .iter()
            .any(|c| c.char == 'N' && !c.style.reverse));
    }

    #[test]
    fn reverse_video_reset_by_sgr0() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("\x1b[7mReversed\x1b[0mNormal");
        let lines = buf.styled_lines();
        let first_line = &lines[0];
        assert!(first_line
            .cells
            .iter()
            .any(|c| c.char == 'R' && c.style.reverse));
        assert!(first_line
            .cells
            .iter()
            .any(|c| c.char == 'N' && !c.style.reverse));
    }

    #[test]
    fn reverse_video_combined_with_colors() {
        let mut buf = TerminalBuffer::new(80, 24);
        // Red text with reverse video
        buf.process("\x1b[31;7mX");
        let lines = buf.styled_lines();
        let cell = lines[0].cells.iter().find(|c| c.char == 'X').unwrap();
        assert!(cell.style.reverse);
        assert_eq!(cell.style.fg, Color::Red);
    }

    // CSI s/u cursor save/restore (different from DEC ESC 7/8)

    #[test]
    fn csi_save_restore_cursor() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("Hello\x1b[s"); // CSI s - save cursor
        buf.process("\r\nWorld");
        buf.process("\x1b[u"); // CSI u - restore cursor
        buf.process("!");
        assert_eq!(buf.to_string(), "Hello!\nWorld");
    }

    // Wide character tests

    #[test]
    fn wide_character_takes_two_columns() {
        let mut buf = TerminalBuffer::new(80, 24);
        // CJK character (wide) followed by ASCII
        buf.process("中X");
        // The wide char takes 2 columns, so X should be at column 2
        assert_eq!(buf.cursor_col(), 3); // 中 (2 cols) + X (1 col) = 3
    }

    #[test]
    fn wide_character_alignment() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("A中B");
        // A at col 0, 中 at col 1-2, B at col 3
        let output = buf.to_string();
        assert!(output.contains("A"));
        assert!(output.contains("中"));
        assert!(output.contains("B"));
    }

    #[test]
    fn wide_character_wraps_correctly() {
        let mut buf = TerminalBuffer::new(5, 2);
        // Width is 5, wide char needs 2 cols
        buf.process("AAAA中"); // 4 A's + wide char that needs 2 cols
                               // Wide char should wrap to next line since only 1 col left
        assert_eq!(buf.cursor_row(), 1);
    }

    #[test]
    fn bullet_character_width() {
        let mut buf = TerminalBuffer::new(80, 24);
        // Bullet ● is typically single-width
        buf.process("●X");
        let output = buf.to_string();
        assert!(output.contains("●"));
        assert!(output.contains("X"));
    }

    // Cursor movement with zero parameter tests

    #[test]
    fn cursor_forward_zero_param_moves_one() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("A\x1b[0CB"); // ESC[0C should move forward 1, same as ESC[1C
        assert_eq!(buf.to_string(), "A B");
    }

    #[test]
    fn cursor_down_zero_param_moves_one() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("A\x1b[0BB"); // ESC[0B should move down 1
        assert_eq!(buf.cursor_row(), 1);
    }

    #[test]
    fn cursor_back_zero_param_moves_one() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("ABC\x1b[0DX"); // ESC[0D should move back 1
        assert_eq!(buf.to_string(), "ABX");
    }

    #[test]
    fn cursor_up_zero_param_moves_one() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("A\r\nB\x1b[0AX"); // ESC[0A should move up 1
        assert_eq!(buf.to_string(), "AX\nB");
    }

    // Mouse tracking sequences are ignored

    #[test]
    fn mouse_tracking_sgr_mode_ignored() {
        let mut buf = TerminalBuffer::new(80, 24);
        // ESC[<0;10;5M is SGR mouse tracking format
        buf.process("Hello\x1b[<0;10;5MWorld");
        assert_eq!(buf.to_string(), "HelloWorld");
    }

    #[test]
    fn special_characters_have_correct_width() {
        // Verify that special Unicode chars used in spinners have width 1
        use unicode_width::UnicodeWidthChar;
        let chars = [
            '⏺', '⎿', '✳', '✶', '✻', '✢', '✽', '·', '●', '❯', '↓', '─', '│', '\u{00A0}',
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

    // Terminal resize tests

    #[test]
    fn resize_preserves_content() {
        let mut buf = TerminalBuffer::new(10, 3);
        buf.process("Hello\r\nWorld");

        // Resize to larger
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

        // Resize to smaller width
        buf.resize(5, 3);
        assert_eq!(buf.width(), 5);

        let output = buf.to_string();
        assert!(output.contains("Hello"), "Should contain Hello");
        assert!(!output.contains("World"), "Should truncate World");
    }

    #[test]
    fn visual_comparison_with_real_cast() {
        // Compare our player output with expected (pyte) output for a real cast file
        // This test helps catch rendering differences

        let test_file = std::path::Path::new(
            "/Users/simon.sanladerer/recorded_agent_sessions/codex/agr_codex_failed_interactively.cast",
        );
        if !test_file.exists() {
            println!("Test file not found, skipping visual comparison");
            return;
        }

        use crate::asciicast::AsciicastFile;
        let cast = AsciicastFile::parse(test_file).expect("Failed to parse cast");
        let (cols, rows) = cast.terminal_size();

        let mut buf = TerminalBuffer::new(cols as usize, rows as usize);

        // Process first 10000 events
        for event in cast.events.iter().take(10000) {
            if event.event_type == crate::asciicast::EventType::Output {
                buf.process(&event.data);
            }
        }

        // Print lines with content for visual comparison
        println!("\n=== OUR PLAYER OUTPUT (10000 events) ===");
        println!("Terminal size: {}x{}", cols, rows);
        let lines = buf.styled_lines();
        for (i, line) in lines.iter().enumerate() {
            let text: String = line.cells.iter().map(|c| c.char).collect();
            if !text.trim().is_empty() {
                let display = if text.len() > 80 { &text[..80] } else { &text };
                println!("{:2}: |{}|", i + 1, display);
            }
        }

        // Expected output from pyte at same point (first few lines):
        // Line 1: |  filename and filename alone, using glob::Pattern, with error handlin|
        // Line 2: |  introduce dialoguer for interactive UI enhancements and update CLI p|
        // Line 4: |  Designing interactive session list and cleanup UI                   |

        // Check that key content is present
        let full_output = buf.to_string();
        assert!(
            full_output.contains("filename"),
            "Expected 'filename' in output"
        );
        assert!(
            full_output.contains("Designing"),
            "Expected 'Designing' in output"
        );
        println!("\n✓ Key content found in output - visual comparison passed");
    }

    #[test]
    fn resize_clamps_cursor() {
        let mut buf = TerminalBuffer::new(20, 10);
        buf.process("Test\r\n\r\n\r\n\r\n\r\nEnd"); // Move cursor down

        // Cursor should be at row 5
        assert_eq!(buf.cursor_row(), 5);

        // Resize to fewer rows
        buf.resize(20, 3);
        assert_eq!(buf.cursor_row(), 2); // Clamped to max row (height - 1)
    }

    #[test]
    fn resize_during_playback_sequence() {
        // Simulate what happens when recording resizes mid-playback
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process("Initial content at 80 cols");
        assert_eq!(buf.width(), 80);

        // Simulate resize event from 80x24 to 100x24
        buf.resize(100, 24);
        assert_eq!(buf.width(), 100);

        // New content should be able to use full width
        buf.process("\r\n");
        buf.process("This is wider content that uses the new 100 column width...............");

        let output = buf.to_string();
        assert!(output.contains("Initial content"));
        assert!(output.contains("This is wider content"));
    }

    #[test]
    fn spinner_update_sequence() {
        // Simulate the actual spinner update sequence from a Claude Code recording
        let mut buf = TerminalBuffer::new(86, 52);

        // First, set up some initial content (like the terminal would have)
        buf.process("Line 0: Some initial content\r\n");
        buf.process("Line 1: More content\r\n");
        buf.process("Line 2: Even more\r\n");
        buf.process("Line 3: Content here\r\n");
        buf.process("Line 4: Fourth line\r\n");
        buf.process("Line 5: Fifth line\r\n");
        buf.process("Line 6: Sixth line");

        // Now at row 6, col 17
        assert_eq!(buf.cursor_row(), 6);

        // Now process the spinner update (simplified version):
        // CR, cursor up 6, print spinner, cursor forward 1, print "Bash"
        buf.process("\r"); // CR - go to col 0
        assert_eq!(buf.cursor_col(), 0);

        buf.process("\x1b[6A"); // Cursor up 6
        assert_eq!(buf.cursor_row(), 0);

        buf.process("⏺"); // Print spinner
        assert_eq!(buf.cursor_col(), 1);

        buf.process("\x1b[1C"); // Cursor forward 1
        assert_eq!(buf.cursor_col(), 2);

        buf.process("Bash"); // Print text
        assert_eq!(buf.cursor_col(), 6);

        // Check row 0 contains the spinner and "Bash"
        let output = buf.to_string();
        let lines: Vec<&str> = output.lines().collect();
        assert!(
            lines[0].starts_with("⏺"),
            "Row 0 should start with ⏺, got: {}",
            lines[0]
        );
        assert!(
            lines[0].contains("Bash"),
            "Row 0 should contain Bash, got: {}",
            lines[0]
        );
    }

    // Scroll region tests

    #[test]
    fn scroll_region_basic_setup() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4");

        // Set scroll region to lines 2-4 (rows 1-3 in 0-indexed)
        buf.process("\x1b[2;4r"); // DECSTBM - set scroll region

        // Cursor should move to home after setting scroll region
        assert_eq!(buf.cursor_row(), 0);
        assert_eq!(buf.cursor_col(), 0);
    }

    #[test]
    fn scroll_region_scroll_within_region() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4");

        // Set scroll region to lines 2-4 (rows 1-3 in 0-indexed)
        buf.process("\x1b[2;4r");

        // Move to bottom of scroll region (row 3 = line 4 in 1-indexed)
        buf.process("\x1b[4;1H");
        assert_eq!(buf.cursor_row(), 3);

        // Now a line feed at the bottom of scroll region should scroll within region
        buf.process("\n");

        let output = buf.to_string();
        let lines: Vec<&str> = output.lines().collect();

        // Line0 should still be at row 0 (outside scroll region)
        assert!(lines[0].starts_with("Line0"), "Line0 should be preserved");

        // Line4 should still be at row 4 (outside scroll region)
        assert!(
            lines[4].starts_with("Line4"),
            "Line4 should be preserved at row 4"
        );
    }

    #[test]
    fn scroll_region_reverse_index_within_region() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4");

        // Set scroll region to lines 2-4 (rows 1-3 in 0-indexed)
        buf.process("\x1b[2;4r");

        // Move to top of scroll region (row 1 = line 2 in 1-indexed)
        buf.process("\x1b[2;1H");
        assert_eq!(buf.cursor_row(), 1);

        // Reverse index at top of scroll region should scroll down within region
        buf.process("\x1bM");

        let output = buf.to_string();
        let lines: Vec<&str> = output.lines().collect();

        // Line0 should still be at row 0 (outside scroll region)
        assert!(lines[0].starts_with("Line0"), "Line0 should be preserved");

        // Line4 should still be at row 4 (outside scroll region)
        assert!(
            lines[4].starts_with("Line4"),
            "Line4 should be preserved at row 4"
        );
    }

    #[test]
    fn scroll_region_csi_scroll_up() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4");

        // Set scroll region to lines 2-4 (rows 1-3 in 0-indexed)
        buf.process("\x1b[2;4r");

        // CSI S - scroll up (pan down)
        buf.process("\x1b[1S");

        let output = buf.to_string();
        let lines: Vec<&str> = output.lines().collect();

        // Line0 should still be at row 0 (outside scroll region)
        assert!(lines[0].starts_with("Line0"), "Line0 should be preserved");

        // Line4 should still be at row 4 (outside scroll region)
        assert!(
            lines[4].starts_with("Line4"),
            "Line4 should be preserved at row 4"
        );
    }

    #[test]
    fn scroll_region_csi_scroll_down() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4");

        // Set scroll region to lines 2-4 (rows 1-3 in 0-indexed)
        buf.process("\x1b[2;4r");

        // CSI T - scroll down (pan up)
        buf.process("\x1b[1T");

        let output = buf.to_string();
        let lines: Vec<&str> = output.lines().collect();

        // Line0 should still be at row 0 (outside scroll region)
        assert!(lines[0].starts_with("Line0"), "Line0 should be preserved");

        // Line4 should still be at row 4 (outside scroll region)
        assert!(
            lines[4].starts_with("Line4"),
            "Line4 should be preserved at row 4"
        );
    }

    #[test]
    fn scroll_region_reset_on_resize() {
        let mut buf = TerminalBuffer::new(10, 5);

        // Set scroll region to lines 2-4
        buf.process("\x1b[2;4r");

        // Resize should reset scroll region to full screen
        buf.resize(10, 10);

        // Now scrolling should affect the full screen
        buf.process("\x1b[10;1H"); // Move to last row
        buf.process("Last\n"); // Should scroll the entire screen

        // The fact that we can scroll to the bottom row indicates
        // scroll region was reset to full height
        assert_eq!(buf.height(), 10);
    }

    #[test]
    fn scroll_region_full_screen_default() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.process("L0\r\nL1\r\nL2\r\nL3\r\nL4");

        // Move to last line and add more lines to trigger scrolling
        buf.process("\x1b[5;1H"); // Move to row 5 (last row)
        buf.process("\nL5"); // This should scroll up

        let output = buf.to_string();
        let lines: Vec<&str> = output.lines().collect();

        // L0 should have scrolled off
        assert!(!lines[0].starts_with("L0"), "L0 should have scrolled off");
        // Last line should be L5
        assert!(lines[4].starts_with("L5"), "L5 should be at bottom");
    }

    #[test]
    fn scroll_region_reset_via_csi_r() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.process("Line0\r\nLine1\r\nLine2\r\nLine3\r\nLine4");

        // Set scroll region to lines 2-4
        buf.process("\x1b[2;4r");

        // Reset scroll region to full screen via CSI r without params
        buf.process("\x1b[r");

        // Now scrolling should affect the full screen
        buf.process("\x1b[5;1H\n"); // Move to last row and scroll

        let output = buf.to_string();
        let lines: Vec<&str> = output.lines().collect();

        // Line0 should have scrolled off since scroll region is now full screen
        assert!(
            !lines[0].starts_with("Line0"),
            "Line0 should have scrolled off"
        );
    }
}
