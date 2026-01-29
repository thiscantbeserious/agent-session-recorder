//! Erase and delete operation handlers.
//!
//! Handles CSI sequences for editing:
//! - J: Erase in Display
//! - K: Erase in Line
//! - L: Insert Lines
//! - M: Delete Lines
//! - P: Delete Characters
//! - @: Insert Characters
//! - X: Erase Characters

use super::super::performer::TerminalPerformer;
use super::super::types::Cell;

impl TerminalPerformer<'_> {
    /// Handle Erase in Display (CSI J).
    /// Mode 0: Erase from cursor to end of screen
    /// Mode 1: Erase from start of screen to cursor
    /// Mode 2/3: Erase entire screen
    pub fn handle_erase_display(&mut self, mode: u16) {
        match mode {
            0 => self.erase_to_eos(),
            1 => self.erase_from_sos(),
            2 | 3 => self.clear_screen(),
            _ => {}
        }
    }

    /// Handle Erase in Line (CSI K).
    /// Mode 0: Erase from cursor to end of line
    /// Mode 1: Erase from start of line to cursor
    /// Mode 2: Erase entire line
    pub fn handle_erase_line(&mut self, mode: u16) {
        match mode {
            0 => self.erase_to_eol(),
            1 => self.erase_from_sol(),
            2 => self.erase_entire_line(),
            _ => {}
        }
    }

    /// Handle Delete Lines (CSI M).
    pub fn handle_delete_lines(&mut self, n: usize) {
        self.delete_lines(n);
    }

    /// Handle Insert Lines (CSI L).
    pub fn handle_insert_lines(&mut self, n: usize) {
        self.insert_lines(n);
    }

    /// Handle Delete Characters (CSI P).
    pub fn handle_delete_chars(&mut self, n: usize) {
        self.delete_chars(n);
    }

    /// Handle Insert Characters (CSI @).
    pub fn handle_insert_chars(&mut self, n: usize) {
        self.insert_chars(n);
    }

    /// Handle Erase Characters (CSI X).
    /// Replaces n characters with spaces starting at cursor, without moving cursor.
    pub fn handle_erase_chars(&mut self, n: usize) {
        if *self.cursor_row < self.height {
            for i in 0..n {
                let col = *self.cursor_col + i;
                if col < self.width {
                    self.buffer[*self.cursor_row][col] = Cell::default();
                }
            }
        }
    }
}
