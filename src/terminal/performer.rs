//! VTE performer implementation.
//!
//! Contains the `TerminalPerformer` struct that implements the vte::Perform trait.
//! Handles escape sequence dispatch to handler modules.

use unicode_width::UnicodeWidthChar;
use vte::Perform;

use super::handlers::{log_unhandled_csi, log_unhandled_esc};
use super::types::Cell;

/// Performer that handles VTE callbacks and updates the buffer.
pub(crate) struct TerminalPerformer<'a> {
    pub buffer: &'a mut Vec<Vec<Cell>>,
    pub width: usize,
    pub height: usize,
    pub cursor_col: &'a mut usize,
    pub cursor_row: &'a mut usize,
    pub current_style: &'a mut super::types::CellStyle,
    pub saved_cursor: &'a mut Option<(usize, usize)>,
    /// Top margin of scroll region (0-indexed, inclusive)
    pub scroll_top: usize,
    /// Bottom margin of scroll region (0-indexed, inclusive)
    pub scroll_bottom: usize,
    /// Optional callback for lines that are scrolled off the screen
    pub scroll_callback: Option<&'a mut dyn FnMut(Vec<Cell>)>,
}

impl<'a> TerminalPerformer<'a> {
    /// Move cursor down one line, scrolling if necessary.
    /// Respects the scroll region (DECSTBM).
    /// Note: This does NOT move to column 0 (that's carriage return).
    ///
    /// Behavior:
    /// - Cursor above scroll_top: moves down normally (will enter region)
    /// - Cursor within region (not at bottom): moves down normally
    /// - Cursor at scroll_bottom: scrolls region up, cursor stays
    /// - Cursor below scroll_bottom: moves down if room, else stays
    fn line_feed(&mut self) {
        if *self.cursor_row < self.scroll_bottom {
            // Above or within scroll region but not at bottom - just move down
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
    pub(crate) fn scroll_up_region(&mut self, n: usize) {
        for _ in 0..n {
            if self.scroll_top < self.height && self.scroll_bottom < self.height {
                // Remove the line at scroll_top
                let line = self.buffer.remove(self.scroll_top);

                // If a callback is registered, pass the scrolled-off line to it
                if let Some(ref mut cb) = self.scroll_callback {
                    cb(line);
                }

                // Insert a new blank line at scroll_bottom
                self.buffer
                    .insert(self.scroll_bottom, vec![Cell::default(); self.width]);
            }
        }
    }

    /// Scroll the scroll region down by n lines.
    /// Removes lines from scroll_bottom and adds empty lines at scroll_top.
    pub(crate) fn scroll_down_region(&mut self, n: usize) {
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
    pub(crate) fn erase_to_eol(&mut self) {
        if *self.cursor_row < self.height {
            for col in *self.cursor_col..self.width {
                self.buffer[*self.cursor_row][col] = Cell::default();
            }
        }
    }

    /// Erase entire line.
    pub(crate) fn erase_entire_line(&mut self) {
        if *self.cursor_row < self.height {
            for col in 0..self.width {
                self.buffer[*self.cursor_row][col] = Cell::default();
            }
        }
    }

    /// Erase from start of line to cursor (inclusive).
    pub(crate) fn erase_from_sol(&mut self) {
        if self.width == 0 {
            return;
        }
        if *self.cursor_row < self.height {
            let end_col = (*self.cursor_col).min(self.width - 1);
            for col in 0..=end_col {
                self.buffer[*self.cursor_row][col] = Cell::default();
            }
        }
    }

    /// Erase from start of screen to cursor.
    pub(crate) fn erase_from_sos(&mut self) {
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
    pub(crate) fn delete_chars(&mut self, n: usize) {
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
    pub(crate) fn insert_chars(&mut self, n: usize) {
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
    pub(crate) fn delete_lines(&mut self, n: usize) {
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
    pub(crate) fn insert_lines(&mut self, n: usize) {
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
    pub(crate) fn erase_to_eos(&mut self) {
        self.erase_to_eol();
        for row in (*self.cursor_row + 1)..self.height {
            for col in 0..self.width {
                self.buffer[row][col] = Cell::default();
            }
        }
    }

    /// Clear entire screen.
    pub(crate) fn clear_screen(&mut self) {
        for row in 0..self.height {
            for col in 0..self.width {
                self.buffer[row][col] = Cell::default();
            }
        }
        *self.cursor_row = 0;
        *self.cursor_col = 0;
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
            // Cursor movement (handlers/cursor.rs)
            'A' => {
                let n = params.first().copied().filter(|&x| x != 0).unwrap_or(1) as usize;
                self.handle_cursor_up(n);
            }
            'B' => {
                let n = params.first().copied().filter(|&x| x != 0).unwrap_or(1) as usize;
                self.handle_cursor_down(n);
            }
            'C' => {
                let n = params.first().copied().filter(|&x| x != 0).unwrap_or(1) as usize;
                self.handle_cursor_forward(n);
            }
            'D' => {
                let n = params.first().copied().filter(|&x| x != 0).unwrap_or(1) as usize;
                self.handle_cursor_back(n);
            }
            'H' | 'f' => {
                let row = params.first().copied().unwrap_or(1) as usize;
                let col = params.get(1).copied().unwrap_or(1) as usize;
                self.handle_cursor_position(row, col);
            }
            'G' => {
                let col = params.first().copied().unwrap_or(1) as usize;
                self.handle_cursor_horizontal_absolute(col);
            }
            'd' => {
                let row = params.first().copied().unwrap_or(1) as usize;
                self.handle_cursor_vertical_absolute(row);
            }
            's' => self.handle_save_cursor(),
            'u' => self.handle_restore_cursor(),

            // Editing (handlers/editing.rs)
            'J' => {
                let mode = params.first().copied().unwrap_or(0);
                self.handle_erase_display(mode);
            }
            'K' => {
                let mode = params.first().copied().unwrap_or(0);
                self.handle_erase_line(mode);
            }
            'L' => {
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.handle_insert_lines(n);
            }
            'M' => {
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.handle_delete_lines(n);
            }
            'P' => {
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.handle_delete_chars(n);
            }
            '@' => {
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.handle_insert_chars(n);
            }
            'X' => {
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.handle_erase_chars(n);
            }

            // Style (handlers/style.rs)
            'm' => self.handle_sgr(&params),

            // Scroll region (handlers/scroll.rs)
            'r' => {
                let top = params.first().copied().unwrap_or(1) as usize;
                let bottom = params.get(1).copied().unwrap_or(self.height as u16) as usize;
                self.handle_set_scroll_region(top, bottom);
            }
            'S' => {
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.handle_scroll_up(n);
            }
            'T' => {
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.handle_scroll_down(n);
            }

            _ => log_unhandled_csi(action, &params, intermediates),
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'7' => self.handle_dec_save_cursor(),
            b'8' => self.handle_dec_restore_cursor(),
            b'M' => self.handle_reverse_index(),
            _ => log_unhandled_esc(byte, intermediates),
        }
    }
}
