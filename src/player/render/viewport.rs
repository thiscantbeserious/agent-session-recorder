//! Viewport rendering for the native player.
//!
//! Renders the terminal buffer content within the visible viewport area.

use std::io::{self, Write};

use anyhow::Result;

use crate::player::render::ansi::{style_to_ansi_attrs, style_to_ansi_bg, style_to_ansi_fg};
use crate::terminal::{Cell, CellStyle, TerminalBuffer};

/// Render a single row's content into `output`.
///
/// Handles the column loop, per-cell style tracking, highlight vs. normal mode,
/// filling spaces past content, and the end-of-row ANSI reset.
fn render_row(
    output: &mut String,
    row: Option<&[Cell]>,
    col_offset: usize,
    view_cols: usize,
    is_highlighted: bool,
) {
    if let Some(cells) = row {
        render_row_cells(output, cells, col_offset, view_cols, is_highlighted);
    } else {
        render_empty_row(output, view_cols, is_highlighted);
    }
}

/// Render cells from a populated row, tracking style changes.
fn render_row_cells(
    output: &mut String,
    cells: &[Cell],
    col_offset: usize,
    view_cols: usize,
    is_highlighted: bool,
) {
    let mut current_style = CellStyle::default();
    let mut in_highlight_style = is_highlighted;

    for view_col in 0..view_cols {
        let buf_col = view_col + col_offset;
        if buf_col < cells.len() {
            render_cell(
                output,
                &cells[buf_col],
                is_highlighted,
                &mut current_style,
                &mut in_highlight_style,
            );
        } else {
            render_space_past_content(output, is_highlighted, &mut current_style);
        }
    }

    if current_style != CellStyle::default() || is_highlighted {
        output.push_str("\x1b[0m");
    }
}

/// Render a single cell, applying or tracking style changes.
fn render_cell(
    output: &mut String,
    cell: &Cell,
    is_highlighted: bool,
    current_style: &mut CellStyle,
    in_highlight_style: &mut bool,
) {
    if !is_highlighted && cell.style != *current_style {
        output.push_str("\x1b[0m");
        style_to_ansi_fg(&cell.style, output);
        style_to_ansi_bg(&cell.style, output);
        style_to_ansi_attrs(&cell.style, output);
        *current_style = cell.style;
        *in_highlight_style = false;
    } else if is_highlighted && !*in_highlight_style {
        output.push_str("\x1b[97;42m");
        *in_highlight_style = true;
    }
    output.push(cell.char);
}

/// Render a space for a column past the end of row content, resetting style if needed.
fn render_space_past_content(
    output: &mut String,
    is_highlighted: bool,
    current_style: &mut CellStyle,
) {
    if !is_highlighted && *current_style != CellStyle::default() {
        output.push_str("\x1b[0m");
        *current_style = CellStyle::default();
    }
    output.push(' ');
}

/// Render an empty row (no cells), filling with spaces and resetting if highlighted.
fn render_empty_row(output: &mut String, view_cols: usize, is_highlighted: bool) {
    for _ in 0..view_cols {
        output.push(' ');
    }
    if is_highlighted {
        output.push_str("\x1b[0m");
    }
}

/// Render a viewport of the terminal buffer to stdout.
///
/// If `highlight_line` is Some, that line (in buffer coordinates) gets a green background.
///
/// # Arguments
/// * `stdout` - The stdout handle to write to
/// * `buffer` - The terminal buffer to render
/// * `row_offset` - Vertical scroll offset
/// * `col_offset` - Horizontal scroll offset
/// * `view_rows` - Number of visible rows
/// * `view_cols` - Number of visible columns
/// * `highlight_line` - Optional line to highlight (for free mode)
#[allow(clippy::too_many_arguments)]
pub fn render_viewport(
    stdout: &mut io::Stdout,
    buffer: &TerminalBuffer,
    row_offset: usize,
    col_offset: usize,
    view_rows: usize,
    view_cols: usize,
    highlight_line: Option<usize>,
) -> Result<()> {
    let mut output = String::with_capacity(view_rows * view_cols * 2);

    for view_row in 0..view_rows {
        let buf_row = view_row + row_offset;
        let is_highlighted = highlight_line == Some(buf_row);

        output.push_str(&format!("\x1b[{};1H", view_row + 1));
        if is_highlighted {
            output.push_str("\x1b[97;42m");
        }

        render_row(
            &mut output,
            buffer.row(buf_row),
            col_offset,
            view_cols,
            is_highlighted,
        );
    }

    write!(stdout, "{}", output)?;
    Ok(())
}

/// Render a single line of the viewport (for partial updates in free mode).
///
/// This is an optimization to avoid re-rendering the entire viewport when
/// only the highlight position changes.
///
/// # Arguments
/// * `stdout` - The stdout handle to write to
/// * `buffer` - The terminal buffer to render
/// * `buf_row` - Buffer row to render
/// * `view_row_offset` - Current viewport vertical offset
/// * `col_offset` - Horizontal scroll offset
/// * `view_cols` - Number of visible columns
/// * `is_highlighted` - Whether this line should be highlighted
#[allow(clippy::too_many_arguments)]
pub fn render_single_line(
    stdout: &mut io::Stdout,
    buffer: &TerminalBuffer,
    buf_row: usize,
    view_row_offset: usize,
    col_offset: usize,
    view_cols: usize,
    is_highlighted: bool,
) -> Result<()> {
    if buf_row < view_row_offset {
        return Ok(());
    }
    let screen_row = buf_row - view_row_offset;

    let mut output = String::with_capacity(view_cols * 2);
    output.push_str(&format!("\x1b[{};1H", screen_row + 1));
    if is_highlighted {
        output.push_str("\x1b[97;42m");
    }

    render_row(
        &mut output,
        buffer.row(buf_row),
        col_offset,
        view_cols,
        is_highlighted,
    );

    write!(stdout, "{}", output)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::TerminalBuffer;

    fn create_buffer_with_content(width: usize, height: usize, content: &str) -> TerminalBuffer {
        let mut buffer = TerminalBuffer::new(width, height);
        buffer.process(content, None);
        buffer
    }

    // === render_viewport tests ===

    #[test]
    fn render_viewport_does_not_panic_empty_buffer() {
        let mut stdout = io::stdout();
        let buffer = TerminalBuffer::new(80, 24);
        let result = render_viewport(&mut stdout, &buffer, 0, 0, 24, 80, None);
        assert!(result.is_ok());
    }

    #[test]
    fn render_viewport_does_not_panic_with_content() {
        let mut stdout = io::stdout();
        let buffer = create_buffer_with_content(80, 24, "Hello, World!");
        let result = render_viewport(&mut stdout, &buffer, 0, 0, 24, 80, None);
        assert!(result.is_ok());
    }

    #[test]
    fn render_viewport_with_row_offset() {
        let mut stdout = io::stdout();
        let buffer = create_buffer_with_content(80, 24, "Line 1\nLine 2\nLine 3");
        let result = render_viewport(&mut stdout, &buffer, 1, 0, 20, 80, None);
        assert!(result.is_ok());
    }

    #[test]
    fn render_viewport_with_col_offset() {
        let mut stdout = io::stdout();
        let buffer = create_buffer_with_content(80, 24, "Hello, World!");
        let result = render_viewport(&mut stdout, &buffer, 0, 5, 24, 75, None);
        assert!(result.is_ok());
    }

    #[test]
    fn render_viewport_with_both_offsets() {
        let mut stdout = io::stdout();
        let buffer = create_buffer_with_content(80, 24, "Line 1\nLine 2\nLine 3");
        let result = render_viewport(&mut stdout, &buffer, 1, 3, 20, 75, None);
        assert!(result.is_ok());
    }

    #[test]
    fn render_viewport_with_highlight_line() {
        let mut stdout = io::stdout();
        let buffer = create_buffer_with_content(80, 24, "Line 1\nLine 2\nLine 3");
        let result = render_viewport(&mut stdout, &buffer, 0, 0, 24, 80, Some(1));
        assert!(result.is_ok());
    }

    #[test]
    fn render_viewport_highlight_at_top() {
        let mut stdout = io::stdout();
        let buffer = create_buffer_with_content(80, 24, "Line 1\nLine 2\nLine 3");
        let result = render_viewport(&mut stdout, &buffer, 0, 0, 24, 80, Some(0));
        assert!(result.is_ok());
    }

    #[test]
    fn render_viewport_highlight_at_bottom() {
        let mut stdout = io::stdout();
        let buffer = create_buffer_with_content(80, 24, "Line 1\nLine 2\nLine 3");
        let result = render_viewport(&mut stdout, &buffer, 0, 0, 24, 80, Some(23));
        assert!(result.is_ok());
    }

    #[test]
    fn render_viewport_small_view() {
        let mut stdout = io::stdout();
        let buffer = create_buffer_with_content(80, 24, "Hello");
        let result = render_viewport(&mut stdout, &buffer, 0, 0, 5, 10, None);
        assert!(result.is_ok());
    }

    #[test]
    fn render_viewport_larger_than_buffer() {
        let mut stdout = io::stdout();
        let buffer = TerminalBuffer::new(40, 10);
        // View is larger than buffer
        let result = render_viewport(&mut stdout, &buffer, 0, 0, 24, 80, None);
        assert!(result.is_ok());
    }

    #[test]
    fn render_viewport_offset_beyond_content() {
        let mut stdout = io::stdout();
        let buffer = TerminalBuffer::new(80, 24);
        // Offset would be past buffer content
        let result = render_viewport(&mut stdout, &buffer, 20, 70, 24, 80, None);
        assert!(result.is_ok());
    }

    #[test]
    fn render_viewport_with_ansi_colors() {
        let mut stdout = io::stdout();
        // Add content with ANSI color codes
        let buffer = create_buffer_with_content(80, 24, "\x1b[31mRed\x1b[0m \x1b[32mGreen\x1b[0m");
        let result = render_viewport(&mut stdout, &buffer, 0, 0, 24, 80, None);
        assert!(result.is_ok());
    }

    #[test]
    fn render_viewport_with_bold_text() {
        let mut stdout = io::stdout();
        let buffer = create_buffer_with_content(80, 24, "\x1b[1mBold\x1b[0m Normal");
        let result = render_viewport(&mut stdout, &buffer, 0, 0, 24, 80, None);
        assert!(result.is_ok());
    }

    #[test]
    fn render_viewport_multiline() {
        let mut stdout = io::stdout();
        let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
        let buffer = create_buffer_with_content(80, 24, content);
        let result = render_viewport(&mut stdout, &buffer, 0, 0, 5, 80, None);
        assert!(result.is_ok());
    }

    // === render_single_line tests ===

    #[test]
    fn render_single_line_does_not_panic_empty() {
        let mut stdout = io::stdout();
        let buffer = TerminalBuffer::new(80, 24);
        let result = render_single_line(&mut stdout, &buffer, 0, 0, 0, 80, false);
        assert!(result.is_ok());
    }

    #[test]
    fn render_single_line_with_content() {
        let mut stdout = io::stdout();
        let buffer = create_buffer_with_content(80, 24, "Hello, World!");
        let result = render_single_line(&mut stdout, &buffer, 0, 0, 0, 80, false);
        assert!(result.is_ok());
    }

    #[test]
    fn render_single_line_with_highlight() {
        let mut stdout = io::stdout();
        let buffer = create_buffer_with_content(80, 24, "Hello, World!");
        let result = render_single_line(&mut stdout, &buffer, 0, 0, 0, 80, true);
        assert!(result.is_ok());
    }

    #[test]
    fn render_single_line_above_viewport_returns_early() {
        let mut stdout = io::stdout();
        let buffer = create_buffer_with_content(80, 24, "Hello, World!");
        // buf_row 0 is above view_row_offset 5
        let result = render_single_line(&mut stdout, &buffer, 0, 5, 0, 80, false);
        assert!(result.is_ok());
    }

    #[test]
    fn render_single_line_within_viewport() {
        let mut stdout = io::stdout();
        let content = "Line 1\nLine 2\nLine 3";
        let buffer = create_buffer_with_content(80, 24, content);
        // Render line 2 (buf_row 1), viewport starts at 0
        let result = render_single_line(&mut stdout, &buffer, 1, 0, 0, 80, false);
        assert!(result.is_ok());
    }

    #[test]
    fn render_single_line_with_col_offset() {
        let mut stdout = io::stdout();
        let buffer = create_buffer_with_content(80, 24, "Hello, World!");
        let result = render_single_line(&mut stdout, &buffer, 0, 0, 5, 75, false);
        assert!(result.is_ok());
    }

    #[test]
    fn render_single_line_with_ansi_colors() {
        let mut stdout = io::stdout();
        let buffer = create_buffer_with_content(80, 24, "\x1b[31mRed\x1b[0m");
        let result = render_single_line(&mut stdout, &buffer, 0, 0, 0, 80, false);
        assert!(result.is_ok());
    }

    #[test]
    fn render_single_line_highlighted_with_colors() {
        let mut stdout = io::stdout();
        let buffer = create_buffer_with_content(80, 24, "\x1b[31mRed\x1b[0m");
        // When highlighted, colors should be overridden
        let result = render_single_line(&mut stdout, &buffer, 0, 0, 0, 80, true);
        assert!(result.is_ok());
    }

    #[test]
    fn render_single_line_empty_row() {
        let mut stdout = io::stdout();
        let buffer = TerminalBuffer::new(80, 24);
        // Row 10 is empty
        let result = render_single_line(&mut stdout, &buffer, 10, 0, 0, 80, false);
        assert!(result.is_ok());
    }

    #[test]
    fn render_single_line_empty_row_highlighted() {
        let mut stdout = io::stdout();
        let buffer = TerminalBuffer::new(80, 24);
        let result = render_single_line(&mut stdout, &buffer, 10, 0, 0, 80, true);
        assert!(result.is_ok());
    }

    #[test]
    fn render_single_line_narrow_view() {
        let mut stdout = io::stdout();
        let buffer = create_buffer_with_content(80, 24, "Hello, World! This is a longer line.");
        let result = render_single_line(&mut stdout, &buffer, 0, 0, 0, 10, false);
        assert!(result.is_ok());
    }

    #[test]
    fn render_single_line_past_content() {
        let mut stdout = io::stdout();
        let buffer = create_buffer_with_content(80, 24, "Short");
        // col_offset beyond content length
        let result = render_single_line(&mut stdout, &buffer, 0, 0, 50, 30, false);
        assert!(result.is_ok());
    }

    #[test]
    fn render_single_line_row_beyond_buffer() {
        let mut stdout = io::stdout();
        let buffer = TerminalBuffer::new(80, 10);
        // Render row 15, but buffer only has 10 rows
        let result = render_single_line(&mut stdout, &buffer, 15, 0, 0, 80, false);
        assert!(result.is_ok());
    }
}
