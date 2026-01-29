//! VTE performer implementation.
//!
//! Contains the `TerminalPerformer` struct that implements the vte::Perform trait.
//! Handles escape sequence dispatch to handler modules.

use unicode_width::UnicodeWidthChar;
use vte::Perform;

use super::types::{Cell, CellStyle, Color};

/// Performer that handles VTE callbacks and updates the buffer.
pub(crate) struct TerminalPerformer<'a> {
    pub buffer: &'a mut Vec<Vec<Cell>>,
    pub width: usize,
    pub height: usize,
    pub cursor_col: &'a mut usize,
    pub cursor_row: &'a mut usize,
    pub current_style: &'a mut CellStyle,
    pub saved_cursor: &'a mut Option<(usize, usize)>,
    /// Top margin of scroll region (0-indexed, inclusive)
    pub scroll_top: usize,
    /// Bottom margin of scroll region (0-indexed, inclusive)
    pub scroll_bottom: usize,
}

impl<'a> TerminalPerformer<'a> {
    /// Move cursor down one line, scrolling if necessary.
    /// Respects the scroll region (DECSTBM).
    /// Note: This does NOT move to column 0 (that's carriage return).
    fn line_feed(&mut self) {
        if *self.cursor_row < self.scroll_bottom {
            // Within scroll region and not at bottom - just move down
            *self.cursor_row += 1;
        } else if *self.cursor_row == self.scroll_bottom {
            // At bottom of scroll region - scroll the region up
            self.scroll_up_region(1);
        } else {
            // Below scroll region - just move down if possible
            if *self.cursor_row + 1 < self.height {
                *self.cursor_row += 1;
            }
        }
    }

    /// Scroll the scroll region up by n lines.
    /// Removes lines from scroll_top and adds empty lines at scroll_bottom.
    fn scroll_up_region(&mut self, n: usize) {
        for _ in 0..n {
            if self.scroll_top < self.height && self.scroll_bottom < self.height {
                // Remove the line at scroll_top
                self.buffer.remove(self.scroll_top);
                // Insert a new blank line at scroll_bottom
                self.buffer
                    .insert(self.scroll_bottom, vec![Cell::default(); self.width]);
            }
        }
    }

    /// Scroll the scroll region down by n lines.
    /// Removes lines from scroll_bottom and adds empty lines at scroll_top.
    fn scroll_down_region(&mut self, n: usize) {
        for _ in 0..n {
            if self.scroll_top < self.height && self.scroll_bottom < self.height {
                // Remove the line at scroll_bottom
                self.buffer.remove(self.scroll_bottom);
                // Insert a new blank line at scroll_top
                self.buffer
                    .insert(self.scroll_top, vec![Cell::default(); self.width]);
            }
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

    /// Delete n lines at cursor, scrolling up within scroll region.
    fn delete_lines(&mut self, n: usize) {
        // Only operates if cursor is within scroll region
        if *self.cursor_row >= self.scroll_top && *self.cursor_row <= self.scroll_bottom {
            for _ in 0..n {
                if *self.cursor_row <= self.scroll_bottom {
                    // Remove the line at cursor position
                    self.buffer.remove(*self.cursor_row);
                    // Insert a new blank line at scroll_bottom
                    self.buffer
                        .insert(self.scroll_bottom, vec![Cell::default(); self.width]);
                }
            }
        }
    }

    /// Insert n blank lines at cursor, scrolling down within scroll region.
    fn insert_lines(&mut self, n: usize) {
        // Only operates if cursor is within scroll region
        if *self.cursor_row >= self.scroll_top && *self.cursor_row <= self.scroll_bottom {
            for _ in 0..n {
                if *self.cursor_row <= self.scroll_bottom {
                    // Remove the line at scroll_bottom
                    self.buffer.remove(self.scroll_bottom);
                    // Insert a new blank line at cursor position
                    self.buffer
                        .insert(*self.cursor_row, vec![Cell::default(); self.width]);
                }
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
            'r' => {
                // DECSTBM - Set Top and Bottom Margins (scroll region)
                // CSI Pt ; Pb r - Set scrolling region from line Pt to Pb
                // Default is full screen
                let top = params.first().copied().unwrap_or(1) as usize;
                let bottom = params.get(1).copied().unwrap_or(self.height as u16) as usize;

                // Convert from 1-indexed to 0-indexed
                let new_top = top.saturating_sub(1);
                let new_bottom = bottom.saturating_sub(1).min(self.height.saturating_sub(1));

                // Validate: top must be less than bottom, both must be in bounds
                if new_top < new_bottom && new_bottom < self.height {
                    self.scroll_top = new_top;
                    self.scroll_bottom = new_bottom;
                    // Move cursor to home position after setting scroll region
                    *self.cursor_row = 0;
                    *self.cursor_col = 0;
                }
            }
            'S' => {
                // SU - Scroll Up (pan down)
                // CSI n S - Scroll up n lines (default 1)
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.scroll_up_region(n);
            }
            'T' => {
                // SD - Scroll Down (pan up)
                // CSI n T - Scroll down n lines (default 1)
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.scroll_down_region(n);
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
                // RI - Reverse Index (move cursor up, scroll if at top of scroll region)
                if *self.cursor_row > self.scroll_top {
                    *self.cursor_row -= 1;
                } else if *self.cursor_row == self.scroll_top {
                    // At top of scroll region - scroll the region down
                    self.scroll_down_region(1);
                } else {
                    // Above scroll region - just move up if possible
                    if *self.cursor_row > 0 {
                        *self.cursor_row -= 1;
                    }
                }
            }
            _ => {}
        }
    }
}
