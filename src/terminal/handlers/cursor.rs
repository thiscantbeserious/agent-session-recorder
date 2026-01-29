//! Cursor movement and positioning handlers.
//!
//! Handles CSI sequences for cursor control:
//! - A: Cursor Up
//! - B: Cursor Down
//! - C: Cursor Forward
//! - D: Cursor Back
//! - H/f: Cursor Position
//! - G: Cursor Horizontal Absolute
//! - d: Cursor Vertical Absolute
//! - s: Save Cursor Position
//! - u: Restore Cursor Position
//! - ESC 7: DEC Save Cursor
//! - ESC 8: DEC Restore Cursor

use super::super::performer::TerminalPerformer;

impl TerminalPerformer<'_> {
    /// Move cursor up by n rows (CSI A).
    pub fn handle_cursor_up(&mut self, n: usize) {
        *self.cursor_row = self.cursor_row.saturating_sub(n);
    }

    /// Move cursor down by n rows (CSI B).
    pub fn handle_cursor_down(&mut self, n: usize) {
        *self.cursor_row = (*self.cursor_row + n).min(self.height.saturating_sub(1));
    }

    /// Move cursor forward by n columns (CSI C).
    pub fn handle_cursor_forward(&mut self, n: usize) {
        *self.cursor_col = (*self.cursor_col + n).min(self.width.saturating_sub(1));
    }

    /// Move cursor back by n columns (CSI D).
    pub fn handle_cursor_back(&mut self, n: usize) {
        *self.cursor_col = self.cursor_col.saturating_sub(n);
    }

    /// Set cursor position to row, col (CSI H / CSI f).
    /// Parameters are 1-indexed, converted to 0-indexed internally.
    pub fn handle_cursor_position(&mut self, row: usize, col: usize) {
        *self.cursor_row = row.saturating_sub(1).min(self.height.saturating_sub(1));
        *self.cursor_col = col.saturating_sub(1).min(self.width.saturating_sub(1));
    }

    /// Set cursor column (CSI G).
    /// Parameter is 1-indexed, converted to 0-indexed internally.
    pub fn handle_cursor_horizontal_absolute(&mut self, col: usize) {
        *self.cursor_col = col.saturating_sub(1).min(self.width.saturating_sub(1));
    }

    /// Set cursor row (CSI d).
    /// Parameter is 1-indexed, converted to 0-indexed internally.
    pub fn handle_cursor_vertical_absolute(&mut self, row: usize) {
        *self.cursor_row = row.saturating_sub(1).min(self.height.saturating_sub(1));
    }

    /// Save cursor position (CSI s).
    pub fn handle_save_cursor(&mut self) {
        *self.saved_cursor = Some((*self.cursor_row, *self.cursor_col));
    }

    /// Restore cursor position (CSI u).
    pub fn handle_restore_cursor(&mut self) {
        if let Some((row, col)) = *self.saved_cursor {
            *self.cursor_row = row.min(self.height.saturating_sub(1));
            *self.cursor_col = col.min(self.width.saturating_sub(1));
        }
    }

    /// DEC save cursor (ESC 7).
    pub fn handle_dec_save_cursor(&mut self) {
        *self.saved_cursor = Some((*self.cursor_row, *self.cursor_col));
    }

    /// DEC restore cursor (ESC 8).
    pub fn handle_dec_restore_cursor(&mut self) {
        if let Some((row, col)) = *self.saved_cursor {
            *self.cursor_row = row.min(self.height.saturating_sub(1));
            *self.cursor_col = col.min(self.width.saturating_sub(1));
        }
    }
}
