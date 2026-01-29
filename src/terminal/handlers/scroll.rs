//! Scroll region handlers.
//!
//! Handles CSI sequences for scroll region control:
//! - r: DECSTBM - Set Top and Bottom Margins
//! - S: Scroll Up (pan down)
//! - T: Scroll Down (pan up)
//! - ESC M: Reverse Index

use super::super::performer::TerminalPerformer;

impl TerminalPerformer<'_> {
    /// Handle DECSTBM - Set Top and Bottom Margins (CSI r).
    /// Parameters are 1-indexed, converted to 0-indexed internally.
    /// Default is full screen.
    pub fn handle_set_scroll_region(&mut self, top: usize, bottom: usize) {
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

    /// Handle Scroll Up (CSI S) - pan down.
    /// Scrolls the scroll region up by n lines.
    pub fn handle_scroll_up(&mut self, n: usize) {
        self.scroll_up_region(n);
    }

    /// Handle Scroll Down (CSI T) - pan up.
    /// Scrolls the scroll region down by n lines.
    pub fn handle_scroll_down(&mut self, n: usize) {
        self.scroll_down_region(n);
    }

    /// Handle Reverse Index (ESC M).
    /// Moves cursor up, scrolling the scroll region down if at top.
    pub fn handle_reverse_index(&mut self) {
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
}
