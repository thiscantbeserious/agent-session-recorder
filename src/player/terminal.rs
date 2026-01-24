//! Virtual terminal buffer for replaying asciicast output.
//!
//! Uses the VTE crate to parse ANSI escape sequences and maintain
//! a virtual terminal state. This allows extracting a snapshot of
//! what the terminal looks like at any point during playback.

use std::fmt;

use unicode_width::UnicodeWidthChar;
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
    pub reverse: bool,
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
    /// Saved cursor position (for CSI s/u)
    saved_cursor: Option<(usize, usize)>,
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
            saved_cursor: None,
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
            saved_cursor: &mut self.saved_cursor,
        };
        self.parser.advance(&mut performer, data.as_bytes());
    }

    /// Resize the terminal buffer to new dimensions.
    ///
    /// Preserves existing content where possible, truncating or extending
    /// rows/columns as needed. Cursor position is clamped to the new bounds.
    pub fn resize(&mut self, new_width: usize, new_height: usize) {
        // Create new buffer with new dimensions
        let mut new_buffer = vec![vec![Cell::default(); new_width]; new_height];

        // Copy existing content, preserving as much as possible
        for (row_idx, row) in self.buffer.iter().enumerate() {
            if row_idx >= new_height {
                break;
            }
            for (col_idx, cell) in row.iter().enumerate() {
                if col_idx >= new_width {
                    break;
                }
                new_buffer[row_idx][col_idx] = *cell;
            }
        }

        self.buffer = new_buffer;
        self.width = new_width;
        self.height = new_height;

        // Clamp cursor to new bounds
        self.cursor_col = self.cursor_col.min(new_width.saturating_sub(1));
        self.cursor_row = self.cursor_row.min(new_height.saturating_sub(1));

        // Invalidate saved cursor if it's now out of bounds
        if let Some((row, col)) = self.saved_cursor {
            if row >= new_height || col >= new_width {
                self.saved_cursor = None;
            }
        }
    }

    /// Get the terminal width.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Get the terminal height.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Get the current cursor row (0-indexed).
    pub fn cursor_row(&self) -> usize {
        self.cursor_row
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

    /// Get a reference to a specific row's cells (no cloning).
    pub fn row(&self, row_idx: usize) -> Option<&[Cell]> {
        self.buffer.get(row_idx).map(|r| r.as_slice())
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
    saved_cursor: &'a mut Option<(usize, usize)>,
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
        // Get the display width of the character (0, 1, or 2)
        let char_width = c.width().unwrap_or(1);

        // Skip zero-width characters (combining marks, etc.)
        if char_width == 0 {
            return;
        }

        // Check if we need to wrap
        if *self.cursor_col + char_width > self.width {
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

            // For wide characters, fill the next cell with a placeholder space
            if char_width == 2 && *self.cursor_col < self.width {
                self.buffer[*self.cursor_row][*self.cursor_col] = Cell {
                    char: ' ', // Placeholder for second half of wide char
                    style: *self.current_style,
                };
                *self.cursor_col += 1;
            }
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

    /// Erase from start of line to cursor (inclusive).
    fn erase_from_sol(&mut self) {
        if *self.cursor_row < self.height {
            let end_col = (*self.cursor_col).min(self.width - 1);
            for col in 0..=end_col {
                self.buffer[*self.cursor_row][col] = Cell::default();
            }
        }
    }

    /// Erase from start of screen to cursor.
    fn erase_from_sos(&mut self) {
        // Erase all rows before current
        for row in 0..*self.cursor_row {
            for col in 0..self.width {
                self.buffer[row][col] = Cell::default();
            }
        }
        // Erase current row up to and including cursor
        self.erase_from_sol();
    }

    /// Delete n characters at cursor, shifting remaining left.
    fn delete_chars(&mut self, n: usize) {
        if *self.cursor_row < self.height {
            let row = &mut self.buffer[*self.cursor_row];
            for i in *self.cursor_col..self.width {
                if i + n < self.width {
                    row[i] = row[i + n];
                } else {
                    row[i] = Cell::default();
                }
            }
        }
    }

    /// Insert n blank characters at cursor, shifting existing right.
    fn insert_chars(&mut self, n: usize) {
        if *self.cursor_row < self.height {
            let row = &mut self.buffer[*self.cursor_row];
            for i in ((*self.cursor_col + n)..self.width).rev() {
                row[i] = row[i - n];
            }
            let end = (*self.cursor_col + n).min(self.width);
            for cell in row.iter_mut().take(end).skip(*self.cursor_col) {
                *cell = Cell::default();
            }
        }
    }

    /// Delete n lines at cursor, scrolling up.
    fn delete_lines(&mut self, n: usize) {
        for _ in 0..n {
            if *self.cursor_row < self.height {
                self.buffer.remove(*self.cursor_row);
                self.buffer.push(vec![Cell::default(); self.width]);
            }
        }
    }

    /// Insert n blank lines at cursor, scrolling down.
    fn insert_lines(&mut self, n: usize) {
        for _ in 0..n {
            if *self.cursor_row < self.height {
                self.buffer.pop();
                self.buffer
                    .insert(*self.cursor_row, vec![Cell::default(); self.width]);
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
                7 => self.current_style.reverse = true,
                22 => {
                    self.current_style.bold = false;
                    self.current_style.dim = false;
                }
                23 => self.current_style.italic = false,
                24 => self.current_style.underline = false,
                27 => self.current_style.reverse = false,
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
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let params: Vec<u16> = params
            .iter()
            .map(|p| p.first().copied().unwrap_or(0))
            .collect();

        // Handle DEC private mode sequences (ESC[?...h/l) and mouse tracking (ESC[<...)
        // These are safe to ignore for text rendering purposes
        if intermediates.contains(&b'?') || intermediates.contains(&b'<') {
            // DEC private modes - we don't need to implement them for text rendering
            // Common ones: ?25h/l (cursor visibility), ?2026h/l (synchronized update),
            // ?1049h/l (alternate screen buffer), <... (mouse tracking SGR mode), etc.
            return;
        }

        match action {
            // Cursor movement
            'A' => {
                // Cursor up - default to 1 if no param or param is 0
                let n = params.first().copied().filter(|&x| x != 0).unwrap_or(1) as usize;
                *self.cursor_row = self.cursor_row.saturating_sub(n);
            }
            'B' => {
                // Cursor down - default to 1 if no param or param is 0
                let n = params.first().copied().filter(|&x| x != 0).unwrap_or(1) as usize;
                *self.cursor_row = (*self.cursor_row + n).min(self.height - 1);
            }
            'C' => {
                // Cursor forward - default to 1 if no param or param is 0
                let n = params.first().copied().filter(|&x| x != 0).unwrap_or(1) as usize;
                *self.cursor_col = (*self.cursor_col + n).min(self.width - 1);
            }
            'D' => {
                // Cursor back - default to 1 if no param or param is 0
                let n = params.first().copied().filter(|&x| x != 0).unwrap_or(1) as usize;
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
                    1 => self.erase_from_sos(),
                    2 | 3 => self.clear_screen(),
                    _ => {}
                }
            }
            'K' => {
                // Erase in line
                let mode = params.first().copied().unwrap_or(0);
                match mode {
                    0 => self.erase_to_eol(),
                    1 => self.erase_from_sol(),
                    2 => self.erase_line(),
                    _ => {}
                }
            }
            'L' => {
                // Insert lines
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.insert_lines(n);
            }
            'M' => {
                // Delete lines
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.delete_lines(n);
            }
            'P' => {
                // Delete characters
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.delete_chars(n);
            }
            '@' => {
                // Insert blank characters
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.insert_chars(n);
            }
            'X' => {
                // Erase characters (replace with spaces, don't move cursor)
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                if *self.cursor_row < self.height {
                    for i in 0..n {
                        let col = *self.cursor_col + i;
                        if col < self.width {
                            self.buffer[*self.cursor_row][col] = Cell::default();
                        }
                    }
                }
            }
            's' => {
                // Save cursor position
                *self.saved_cursor = Some((*self.cursor_row, *self.cursor_col));
            }
            'u' => {
                // Restore cursor position
                if let Some((row, col)) = *self.saved_cursor {
                    *self.cursor_row = row.min(self.height - 1);
                    *self.cursor_col = col.min(self.width - 1);
                }
            }
            'G' => {
                // Cursor horizontal absolute
                let col = params.first().copied().unwrap_or(1) as usize;
                *self.cursor_col = col.saturating_sub(1).min(self.width - 1);
            }
            'd' => {
                // Cursor vertical absolute
                let row = params.first().copied().unwrap_or(1) as usize;
                *self.cursor_row = row.saturating_sub(1).min(self.height - 1);
            }
            'm' => {
                // SGR (Select Graphic Rendition) - handle colors and styles
                self.handle_sgr(&params);
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'7' => {
                // DECSC - DEC save cursor
                *self.saved_cursor = Some((*self.cursor_row, *self.cursor_col));
            }
            b'8' => {
                // DECRC - DEC restore cursor
                if let Some((row, col)) = *self.saved_cursor {
                    *self.cursor_row = row.min(self.height - 1);
                    *self.cursor_col = col.min(self.width - 1);
                }
            }
            b'M' => {
                // RI - Reverse Index (move cursor up, scroll if at top)
                if *self.cursor_row > 0 {
                    *self.cursor_row -= 1;
                } else {
                    // Scroll down - add empty row at top, remove bottom
                    self.buffer.pop();
                    self.buffer.insert(0, vec![Cell::default(); self.width]);
                }
            }
            _ => {}
        }
    }
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
        assert_eq!(buf.cursor_col, 3); // 中 (2 cols) + X (1 col) = 3
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
        assert_eq!(buf.cursor_row, 1);
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
        assert_eq!(buf.cursor_row, 1);
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
    fn resize_clamps_cursor() {
        let mut buf = TerminalBuffer::new(20, 10);
        buf.process("Test\r\n\r\n\r\n\r\n\r\nEnd"); // Move cursor down

        // Cursor should be at row 5
        assert_eq!(buf.cursor_row, 5);

        // Resize to fewer rows
        buf.resize(20, 3);
        assert_eq!(buf.cursor_row, 2); // Clamped to max row (height - 1)
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
        assert_eq!(buf.cursor_row, 6);

        // Now process the spinner update (simplified version):
        // CR, cursor up 6, print spinner, cursor forward 1, print "Bash"
        buf.process("\r"); // CR - go to col 0
        assert_eq!(buf.cursor_col, 0);

        buf.process("\x1b[6A"); // Cursor up 6
        assert_eq!(buf.cursor_row, 0);

        buf.process("⏺"); // Print spinner
        assert_eq!(buf.cursor_col, 1);

        buf.process("\x1b[1C"); // Cursor forward 1
        assert_eq!(buf.cursor_col, 2);

        buf.process("Bash"); // Print text
        assert_eq!(buf.cursor_col, 6);

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
}
