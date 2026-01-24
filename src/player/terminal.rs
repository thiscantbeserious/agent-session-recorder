//! Virtual terminal buffer for replaying asciicast output.
//!
//! Uses the VTE crate to parse ANSI escape sequences and maintain
//! a virtual terminal state. This allows extracting a snapshot of
//! what the terminal looks like at any point during playback.

use std::fmt;

use vte::{Parser, Perform};

/// ANSI color codes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    /// 256-color palette index
    Indexed(u8),
    /// RGB color
    Rgb(u8, u8, u8),
}

/// Style attributes for a terminal cell
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellStyle {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
}

/// A single cell in the terminal buffer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub char: char,
    pub style: CellStyle,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            char: ' ',
            style: CellStyle::default(),
        }
    }
}

/// A styled line for rendering
#[derive(Debug, Clone)]
pub struct StyledLine {
    pub cells: Vec<Cell>,
}

/// A virtual terminal buffer that processes ANSI escape sequences.
///
/// The buffer maintains a 2D grid of cells representing the terminal
/// screen state. It handles cursor movement, line wrapping, colors,
/// and basic ANSI escape sequences.
pub struct TerminalBuffer {
    /// Terminal width in columns
    width: usize,
    /// Terminal height in rows
    height: usize,
    /// The screen buffer - a 2D grid of cells
    buffer: Vec<Vec<Cell>>,
    /// Current cursor column (0-indexed)
    cursor_col: usize,
    /// Current cursor row (0-indexed)
    cursor_row: usize,
    /// Current style for new characters
    current_style: CellStyle,
    /// VTE parser for handling ANSI sequences
    parser: Parser,
}

impl TerminalBuffer {
    /// Create a new terminal buffer with the given dimensions.
    pub fn new(width: usize, height: usize) -> Self {
        let buffer = vec![vec![Cell::default(); width]; height];
        Self {
            width,
            height,
            buffer,
            cursor_col: 0,
            cursor_row: 0,
            current_style: CellStyle::default(),
            parser: Parser::new(),
        }
    }

    /// Process output data through the terminal emulator.
    ///
    /// This parses ANSI escape sequences and updates the buffer state.
    pub fn process(&mut self, data: &str) {
        let mut performer = TerminalPerformer {
            buffer: &mut self.buffer,
            width: self.width,
            height: self.height,
            cursor_col: &mut self.cursor_col,
            cursor_row: &mut self.cursor_row,
            current_style: &mut self.current_style,
        };
        self.parser.advance(&mut performer, data.as_bytes());
    }

    /// Get the terminal width.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Get the terminal height.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Get styled lines for rendering with color support.
    pub fn styled_lines(&self) -> Vec<StyledLine> {
        self.buffer
            .iter()
            .map(|row| {
                // Trim trailing default cells
                let mut cells: Vec<Cell> = row.clone();
                while cells
                    .last()
                    .map(|c| c.char == ' ' && c.style == CellStyle::default())
                    .unwrap_or(false)
                {
                    cells.pop();
                }
                StyledLine { cells }
            })
            .collect()
    }
}

impl fmt::Display for TerminalBuffer {
    /// Display the current screen content as a string (without colors).
    ///
    /// Returns the visible content with trailing whitespace trimmed from each line.
    /// Empty trailing lines are removed.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut lines: Vec<String> = self
            .buffer
            .iter()
            .map(|row| {
                row.iter()
                    .map(|c| c.char)
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect();

        // Remove empty trailing lines
        while lines.last().map(|s| s.is_empty()).unwrap_or(false) {
            lines.pop();
        }

        write!(f, "{}", lines.join("\n"))
    }
}

/// Performer that handles VTE callbacks and updates the buffer.
struct TerminalPerformer<'a> {
    buffer: &'a mut Vec<Vec<Cell>>,
    width: usize,
    height: usize,
    cursor_col: &'a mut usize,
    cursor_row: &'a mut usize,
    current_style: &'a mut CellStyle,
}

impl<'a> TerminalPerformer<'a> {
    /// Move cursor down one line, scrolling if necessary.
    /// Note: This does NOT move to column 0 (that's carriage return).
    fn line_feed(&mut self) {
        if *self.cursor_row + 1 < self.height {
            *self.cursor_row += 1;
        } else {
            // Scroll up - remove first row and add empty row at bottom
            self.buffer.remove(0);
            self.buffer.push(vec![Cell::default(); self.width]);
        }
    }

    /// Move cursor to start of current line.
    fn carriage_return(&mut self) {
        *self.cursor_col = 0;
    }

    /// Move cursor back one position.
    fn backspace(&mut self) {
        if *self.cursor_col > 0 {
            *self.cursor_col -= 1;
        }
    }

    /// Write a character at the current cursor position with current style.
    fn put_char(&mut self, c: char) {
        if *self.cursor_col >= self.width {
            // Line wrap - move to next line and column 0
            self.line_feed();
            self.carriage_return();
        }

        if *self.cursor_row < self.height && *self.cursor_col < self.width {
            self.buffer[*self.cursor_row][*self.cursor_col] = Cell {
                char: c,
                style: *self.current_style,
            };
            *self.cursor_col += 1;
        }
    }

    /// Erase from cursor to end of line.
    fn erase_to_eol(&mut self) {
        if *self.cursor_row < self.height {
            for col in *self.cursor_col..self.width {
                self.buffer[*self.cursor_row][col] = Cell::default();
            }
        }
    }

    /// Erase entire line.
    fn erase_line(&mut self) {
        if *self.cursor_row < self.height {
            for col in 0..self.width {
                self.buffer[*self.cursor_row][col] = Cell::default();
            }
        }
    }

    /// Erase from cursor to end of screen.
    fn erase_to_eos(&mut self) {
        self.erase_to_eol();
        for row in (*self.cursor_row + 1)..self.height {
            for col in 0..self.width {
                self.buffer[row][col] = Cell::default();
            }
        }
    }

    /// Clear entire screen.
    fn clear_screen(&mut self) {
        for row in 0..self.height {
            for col in 0..self.width {
                self.buffer[row][col] = Cell::default();
            }
        }
        *self.cursor_row = 0;
        *self.cursor_col = 0;
    }

    /// Parse SGR (Select Graphic Rendition) parameters and update current style.
    fn handle_sgr(&mut self, params: &[u16]) {
        let mut iter = params.iter().peekable();

        while let Some(&param) = iter.next() {
            match param {
                0 => *self.current_style = CellStyle::default(), // Reset
                1 => self.current_style.bold = true,
                2 => self.current_style.dim = true,
                3 => self.current_style.italic = true,
                4 => self.current_style.underline = true,
                22 => {
                    self.current_style.bold = false;
                    self.current_style.dim = false;
                }
                23 => self.current_style.italic = false,
                24 => self.current_style.underline = false,
                // Standard foreground colors (30-37)
                30 => self.current_style.fg = Color::Black,
                31 => self.current_style.fg = Color::Red,
                32 => self.current_style.fg = Color::Green,
                33 => self.current_style.fg = Color::Yellow,
                34 => self.current_style.fg = Color::Blue,
                35 => self.current_style.fg = Color::Magenta,
                36 => self.current_style.fg = Color::Cyan,
                37 => self.current_style.fg = Color::White,
                38 => {
                    // Extended foreground color
                    if let Some(&&mode) = iter.peek() {
                        iter.next();
                        match mode {
                            5 => {
                                // 256-color mode
                                if let Some(&&idx) = iter.peek() {
                                    iter.next();
                                    self.current_style.fg = Color::Indexed(idx as u8);
                                }
                            }
                            2 => {
                                // RGB mode
                                let r = iter.next().copied().unwrap_or(0) as u8;
                                let g = iter.next().copied().unwrap_or(0) as u8;
                                let b = iter.next().copied().unwrap_or(0) as u8;
                                self.current_style.fg = Color::Rgb(r, g, b);
                            }
                            _ => {}
                        }
                    }
                }
                39 => self.current_style.fg = Color::Default,
                // Standard background colors (40-47)
                40 => self.current_style.bg = Color::Black,
                41 => self.current_style.bg = Color::Red,
                42 => self.current_style.bg = Color::Green,
                43 => self.current_style.bg = Color::Yellow,
                44 => self.current_style.bg = Color::Blue,
                45 => self.current_style.bg = Color::Magenta,
                46 => self.current_style.bg = Color::Cyan,
                47 => self.current_style.bg = Color::White,
                48 => {
                    // Extended background color
                    if let Some(&&mode) = iter.peek() {
                        iter.next();
                        match mode {
                            5 => {
                                // 256-color mode
                                if let Some(&&idx) = iter.peek() {
                                    iter.next();
                                    self.current_style.bg = Color::Indexed(idx as u8);
                                }
                            }
                            2 => {
                                // RGB mode
                                let r = iter.next().copied().unwrap_or(0) as u8;
                                let g = iter.next().copied().unwrap_or(0) as u8;
                                let b = iter.next().copied().unwrap_or(0) as u8;
                                self.current_style.bg = Color::Rgb(r, g, b);
                            }
                            _ => {}
                        }
                    }
                }
                49 => self.current_style.bg = Color::Default,
                // Bright foreground colors (90-97)
                90 => self.current_style.fg = Color::BrightBlack,
                91 => self.current_style.fg = Color::BrightRed,
                92 => self.current_style.fg = Color::BrightGreen,
                93 => self.current_style.fg = Color::BrightYellow,
                94 => self.current_style.fg = Color::BrightBlue,
                95 => self.current_style.fg = Color::BrightMagenta,
                96 => self.current_style.fg = Color::BrightCyan,
                97 => self.current_style.fg = Color::BrightWhite,
                // Bright background colors (100-107)
                100 => self.current_style.bg = Color::BrightBlack,
                101 => self.current_style.bg = Color::BrightRed,
                102 => self.current_style.bg = Color::BrightGreen,
                103 => self.current_style.bg = Color::BrightYellow,
                104 => self.current_style.bg = Color::BrightBlue,
                105 => self.current_style.bg = Color::BrightMagenta,
                106 => self.current_style.bg = Color::BrightCyan,
                107 => self.current_style.bg = Color::BrightWhite,
                _ => {}
            }
        }
    }
}

impl Perform for TerminalPerformer<'_> {
    fn print(&mut self, c: char) {
        self.put_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => self.line_feed(),
            b'\r' => self.carriage_return(),
            b'\x08' => self.backspace(), // Backspace
            b'\t' => {
                // Tab - move to next tab stop (every 8 columns)
                let next_tab = (*self.cursor_col / 8 + 1) * 8;
                *self.cursor_col = next_tab.min(self.width - 1);
            }
            _ => {}
        }
    }

    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _action: char) {
    }

    fn put(&mut self, _byte: u8) {}

    fn unhook(&mut self) {}

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let params: Vec<u16> = params
            .iter()
            .map(|p| p.first().copied().unwrap_or(0))
            .collect();

        match action {
            // Cursor movement
            'A' => {
                // Cursor up - default to 1 if no param or param is 0
                let n = params.first().copied().filter(|&x| x != 0).unwrap_or(1) as usize;
                *self.cursor_row = self.cursor_row.saturating_sub(n);
            }
            'B' => {
                // Cursor down
                let n = params.first().copied().unwrap_or(1) as usize;
                *self.cursor_row = (*self.cursor_row + n).min(self.height - 1);
            }
            'C' => {
                // Cursor forward
                let n = params.first().copied().unwrap_or(1) as usize;
                *self.cursor_col = (*self.cursor_col + n).min(self.width - 1);
            }
            'D' => {
                // Cursor back
                let n = params.first().copied().unwrap_or(1) as usize;
                *self.cursor_col = self.cursor_col.saturating_sub(n);
            }
            'H' | 'f' => {
                // Cursor position (row;col)
                let row = params.first().copied().unwrap_or(1) as usize;
                let col = params.get(1).copied().unwrap_or(1) as usize;
                *self.cursor_row = row.saturating_sub(1).min(self.height - 1);
                *self.cursor_col = col.saturating_sub(1).min(self.width - 1);
            }
            'J' => {
                // Erase in display
                let mode = params.first().copied().unwrap_or(0);
                match mode {
                    0 => self.erase_to_eos(),
                    2 => self.clear_screen(),
                    _ => {}
                }
            }
            'K' => {
                // Erase in line
                let mode = params.first().copied().unwrap_or(0);
                match mode {
                    0 => self.erase_to_eol(),
                    2 => self.erase_line(),
                    _ => {}
                }
            }
            'm' => {
                // SGR (Select Graphic Rendition) - handle colors and styles
                self.handle_sgr(&params);
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}

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
        assert_eq!(buf.cursor_row, 1, "cursor row should be 1 after Line 2");
        assert_eq!(buf.cursor_col, 6, "cursor col should be 6 after Line 2");

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
}
