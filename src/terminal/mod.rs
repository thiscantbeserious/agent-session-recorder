//! Virtual terminal emulator module.
//!
//! Provides a VTE-based terminal buffer for replaying asciicast output.
//! Handles ANSI escape sequences and maintains terminal state.
//!
//! This module is designed as a general-purpose VT emulator that can be used
//! by the player, TUI widgets, and future analysis features.

mod handlers;
mod performer;
mod types;

pub use types::{Cell, CellStyle, Color, StyledLine};

use std::fmt;
use vte::Parser;

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
    /// Top margin of scroll region (0-indexed, inclusive)
    scroll_top: usize,
    /// Bottom margin of scroll region (0-indexed, inclusive)
    scroll_bottom: usize,
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
            scroll_top: 0,
            scroll_bottom: height.saturating_sub(1),
        }
    }

    /// Process output data through the terminal emulator.
    ///
    /// This parses ANSI escape sequences and updates the buffer state.
    pub fn process(&mut self, data: &str, mut scroll_callback: Option<&mut dyn FnMut(Vec<Cell>)>) {
        let mut perf = performer::TerminalPerformer {
            buffer: &mut self.buffer,
            width: self.width,
            height: self.height,
            cursor_col: &mut self.cursor_col,
            cursor_row: &mut self.cursor_row,
            current_style: &mut self.current_style,
            saved_cursor: &mut self.saved_cursor,
            scroll_top: self.scroll_top,
            scroll_bottom: self.scroll_bottom,
            scroll_callback: scroll_callback
                .as_mut()
                .map(|cb| *cb as &mut dyn FnMut(Vec<Cell>)),
        };
        self.parser.advance(&mut perf, data.as_bytes());
        // Update scroll region in case it was changed by DECSTBM
        self.scroll_top = perf.scroll_top;
        self.scroll_bottom = perf.scroll_bottom;
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

        // Reset scroll region to full screen on resize
        self.scroll_top = 0;
        self.scroll_bottom = new_height.saturating_sub(1);

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

    /// Get the current cursor column (0-indexed).
    pub fn cursor_col(&self) -> usize {
        self.cursor_col
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
