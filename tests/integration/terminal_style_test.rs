//! SGR (Select Graphic Rendition) tests for colors and attributes.

use agr::terminal::{Cell, CellStyle, Color, TerminalBuffer};

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
    buf.process("Line 1\r\nLine 2\r\nLine 3", None);
    let lines = buf.styled_lines();
    assert_eq!(lines.len(), 3);
}

#[test]
fn styled_lines_preserves_red_foreground() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\x1b[31mRed", None);
    let lines = buf.styled_lines();
    assert!(!lines.is_empty());
    let first_line = &lines[0];
    assert!(first_line.cells.iter().any(|c| c.style.fg == Color::Red));
}

#[test]
fn styled_lines_preserves_green_foreground() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\x1b[32mGreen", None);
    let lines = buf.styled_lines();
    let first_line = &lines[0];
    assert!(first_line.cells.iter().any(|c| c.style.fg == Color::Green));
}

#[test]
fn styled_lines_preserves_blue_background() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\x1b[44mBlue BG", None);
    let lines = buf.styled_lines();
    let first_line = &lines[0];
    assert!(first_line.cells.iter().any(|c| c.style.bg == Color::Blue));
}

#[test]
fn styled_lines_preserves_bold() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\x1b[1mBold", None);
    let lines = buf.styled_lines();
    let first_line = &lines[0];
    assert!(first_line.cells.iter().any(|c| c.style.bold));
}

#[test]
fn styled_lines_reset_clears_style() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\x1b[31mRed\x1b[0mNormal", None);
    let lines = buf.styled_lines();
    let first_line = &lines[0];
    assert!(first_line.cells.iter().any(|c| c.style.fg == Color::Red));
    assert!(first_line
        .cells
        .iter()
        .any(|c| c.style.fg == Color::Default && c.char == 'N'));
}

#[test]
fn styled_lines_bright_colors() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\x1b[91mBright Red", None);
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
    buf.process("\x1b[38;5;196mIndexed", None);
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
    buf.process("\x1b[38;2;255;128;64mRGB", None);
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
    buf.process("\x1b[1;4;31mBold Underline Red", None);
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
    buf.process("\x1b[1;32mBold Green\x1b[0m Normal", None);
    let lines = buf.styled_lines();
    let first_line = &lines[0];

    let bold_green_cells: Vec<_> = first_line
        .cells
        .iter()
        .filter(|c| c.style.bold && c.style.fg == Color::Green)
        .collect();
    assert!(!bold_green_cells.is_empty(), "Should have bold green cells");

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
    buf.process("\x1b[2mDim", None);
    let lines = buf.styled_lines();
    let first_line = &lines[0];
    assert!(first_line.cells.iter().any(|c| c.style.dim));
}

#[test]
fn styled_lines_italic_attribute() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\x1b[3mItalic", None);
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
        buf.process(&format!("{}X", seq), None);
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
        buf.process(&format!("{}X", seq), None);
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
    buf.process("\x1b[31mR\x1b[39mD", None);
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
    buf.process("\x1b[41mR\x1b[49mD", None);
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
    buf.process("\x1b[100mX", None);
    let lines = buf.styled_lines();
    assert!(lines[0]
        .cells
        .iter()
        .any(|c| c.style.bg == Color::BrightBlack));
}

#[test]
fn styled_lines_256_background_color() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\x1b[48;5;82mX", None);
    let lines = buf.styled_lines();
    assert!(lines[0]
        .cells
        .iter()
        .any(|c| c.style.bg == Color::Indexed(82)));
}

#[test]
fn styled_lines_rgb_background_color() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\x1b[48;2;100;150;200mX", None);
    let lines = buf.styled_lines();
    assert!(lines[0]
        .cells
        .iter()
        .any(|c| c.style.bg == Color::Rgb(100, 150, 200)));
}

#[test]
fn sgr_reset_bold_and_dim() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\x1b[1;2mX\x1b[22mY", None);
    let lines = buf.styled_lines();
    let first_line = &lines[0];
    assert!(first_line
        .cells
        .iter()
        .any(|c| c.char == 'X' && c.style.bold && c.style.dim));
    assert!(first_line
        .cells
        .iter()
        .any(|c| c.char == 'Y' && !c.style.bold && !c.style.dim));
}

#[test]
fn sgr_reset_italic() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\x1b[3mX\x1b[23mY", None);
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
    buf.process("\x1b[4mX\x1b[24mY", None);
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
        buf.process(&format!("{}X", seq), None);
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
        buf.process(&format!("{}X", seq), None);
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
    buf.process("\x1b[999mX", None);
    let lines = buf.styled_lines();
    assert!(lines[0].cells.iter().any(|c| c.char == 'X'));
}

#[test]
fn reverse_video_attribute() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\x1b[7mReversed\x1b[27mNormal", None);
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
fn reverse_video_reset_by_sgr0() {
    let mut buf = TerminalBuffer::new(80, 24);
    buf.process("\x1b[7mReversed\x1b[0mNormal", None);
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
    buf.process("\x1b[31;7mX", None);
    let lines = buf.styled_lines();
    let cell = lines[0].cells.iter().find(|c| c.char == 'X').unwrap();
    assert!(cell.style.reverse);
    assert_eq!(cell.style.fg, Color::Red);
}
