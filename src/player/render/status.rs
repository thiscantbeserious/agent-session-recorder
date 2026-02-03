//! Status bar rendering for the native player.
//!
//! Displays playback state, mode indicators, and keyboard shortcuts.

use std::io::{self, Write};

use anyhow::Result;

/// Count digits in a number (for width calculation).
///
/// # Arguments
/// * `n` - The number to count digits of
///
/// # Returns
/// The number of digits in the base-10 representation
#[inline]
pub fn count_digits(n: usize) -> usize {
    if n == 0 {
        1
    } else {
        n.ilog10() as usize + 1
    }
}

/// Render a separator line.
///
/// # Arguments
/// * `stdout` - The stdout handle to write to
/// * `width` - Terminal width
/// * `row` - Row to render at (0-indexed)
pub fn render_separator_line(stdout: &mut io::Stdout, width: u16, row: u16) -> Result<()> {
    // Build line as string to minimize syscalls
    let mut output = String::with_capacity(width as usize + 20);
    output.push_str(&format!("\x1b[{};1H\x1b[90m", row + 1)); // Move + dark gray
    for _ in 0..width {
        output.push('─');
    }
    output.push_str("\x1b[0m"); // Reset
    write!(stdout, "{}", output)?;
    Ok(())
}

/// Render the status/controls bar.
///
/// # Arguments
/// * `stdout` - The stdout handle to write to
/// * `width` - Terminal width
/// * `row` - Row to render at (0-indexed)
/// * `paused` - Whether playback is paused
/// * `speed` - Current playback speed
/// * `rec_cols` - Recording width
/// * `rec_rows` - Recording height
/// * `view_cols` - Viewport width
/// * `view_rows` - Viewport height
/// * `col_offset` - Current horizontal scroll offset
/// * `row_offset` - Current vertical scroll offset
/// * `marker_count` - Number of markers in the recording
/// * `viewport_mode` - Whether viewport mode is active
/// * `free_mode` - Whether free mode is active
#[allow(clippy::too_many_arguments)]
pub fn render_status_bar(
    stdout: &mut io::Stdout,
    width: u16,
    row: u16,
    paused: bool,
    speed: f64,
    rec_cols: u32,
    rec_rows: u32,
    view_cols: usize,
    view_rows: usize,
    col_offset: usize,
    row_offset: usize,
    marker_count: usize,
    viewport_mode: bool,
    free_mode: bool,
) -> Result<()> {
    // ANSI color codes
    const WHITE: &str = "\x1b[97m";
    const MAGENTA: &str = "\x1b[35m";
    const GREEN: &str = "\x1b[32m";
    const DARK_GREY: &str = "\x1b[90m";
    const YELLOW: &str = "\x1b[33m";
    const CYAN: &str = "\x1b[36m";
    const RESET: &str = "\x1b[0m";

    let mut output = String::with_capacity(256);
    let mut visible_len: usize = 0; // Track visible width manually

    output.push_str(&format!("\x1b[{};1H", row + 1));

    output.push_str(WHITE);
    output.push(' ');
    visible_len += 1;

    // State icon (▶ and ⏸ are double-width unicode)
    let state = if paused { "▶  " } else { "⏸  " };
    output.push_str(state);
    visible_len += 4; // icon (2) + 2 spaces

    if viewport_mode {
        output.push_str(MAGENTA);
        output.push_str("[V] ");
        visible_len += 4;
    }

    if free_mode {
        output.push_str(GREEN);
        output.push_str("[F] ");
        visible_len += 4;
    }

    output.push_str(DARK_GREY);
    output.push_str("spd:");
    visible_len += 4;
    output.push_str(WHITE);
    let speed_str = format!("{:.1}x ", speed);
    visible_len += speed_str.len();
    output.push_str(&speed_str);

    if marker_count > 0 {
        output.push_str(YELLOW);
        let marker_str = format!("◆{} ", marker_count);
        visible_len += 1 + count_digits(marker_count) + 1; // ◆ + digits + space
        output.push_str(&marker_str);
    }

    if rec_cols as usize > view_cols || rec_rows as usize > view_rows {
        output.push_str(DARK_GREY);
        let offset_str = format!("[{},{}] ", col_offset, row_offset);
        visible_len += offset_str.len();
        output.push_str(&offset_str);
    }

    let play_action = if paused { ":play " } else { ":pause " };
    output.push_str(DARK_GREY);
    output.push_str("│ ");
    visible_len += 2;
    output.push_str(CYAN);
    output.push_str("space");
    visible_len += 5;
    output.push_str(DARK_GREY);
    output.push_str(play_action);
    visible_len += play_action.len();
    output.push_str(CYAN);
    output.push('m');
    visible_len += 1;
    output.push_str(DARK_GREY);
    output.push_str(":mrk ");
    visible_len += 5;
    output.push_str(CYAN);
    output.push('f');
    visible_len += 1;
    output.push_str(DARK_GREY);
    output.push_str(":fre ");
    visible_len += 5;
    output.push_str(CYAN);
    output.push('v');
    visible_len += 1;
    output.push_str(DARK_GREY);
    output.push_str(":vpt ");
    visible_len += 5;
    output.push_str(CYAN);
    output.push('r');
    visible_len += 1;
    output.push_str(DARK_GREY);
    output.push_str(":rsz ");
    visible_len += 5;
    output.push_str(CYAN);
    output.push('?');
    visible_len += 1;
    output.push_str(DARK_GREY);
    output.push_str(":hlp ");
    visible_len += 5;
    output.push_str(CYAN);
    output.push('q');
    visible_len += 1;
    output.push_str(DARK_GREY);
    output.push_str(":quit");
    visible_len += 5;

    // Pad to full width to overwrite any leftover content
    let padding = (width as usize).saturating_sub(visible_len);
    for _ in 0..padding {
        output.push(' ');
    }

    output.push_str(RESET);
    write!(stdout, "{}", output)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_digits_works() {
        assert_eq!(count_digits(0), 1);
        assert_eq!(count_digits(1), 1);
        assert_eq!(count_digits(9), 1);
        assert_eq!(count_digits(10), 2);
        assert_eq!(count_digits(99), 2);
        assert_eq!(count_digits(100), 3);
    }

    #[test]
    fn count_digits_large_numbers() {
        assert_eq!(count_digits(999), 3);
        assert_eq!(count_digits(1000), 4);
        assert_eq!(count_digits(9999), 4);
        assert_eq!(count_digits(10000), 5);
        assert_eq!(count_digits(1_000_000), 7);
    }

    #[test]
    fn count_digits_boundary_values() {
        // Test powers of 10 boundaries
        assert_eq!(count_digits(9), 1);
        assert_eq!(count_digits(10), 2);
        assert_eq!(count_digits(99), 2);
        assert_eq!(count_digits(100), 3);
        assert_eq!(count_digits(999), 3);
        assert_eq!(count_digits(1000), 4);
    }

    #[test]
    fn render_separator_line_does_not_panic() {
        // Just verify it doesn't panic with various inputs
        let mut stdout = io::stdout();
        assert!(render_separator_line(&mut stdout, 80, 0).is_ok());
        assert!(render_separator_line(&mut stdout, 80, 10).is_ok());
        assert!(render_separator_line(&mut stdout, 120, 5).is_ok());
    }

    #[test]
    fn render_separator_line_handles_small_width() {
        let mut stdout = io::stdout();
        assert!(render_separator_line(&mut stdout, 10, 0).is_ok());
        assert!(render_separator_line(&mut stdout, 1, 0).is_ok());
    }

    #[test]
    fn render_status_bar_does_not_panic_basic() {
        let mut stdout = io::stdout();
        let result = render_status_bar(
            &mut stdout,
            80,    // width
            25,    // row
            false, // paused
            1.0,   // speed
            80,    // rec_cols
            24,    // rec_rows
            80,    // view_cols
            24,    // view_rows
            0,     // col_offset
            0,     // row_offset
            0,     // marker_count
            false, // viewport_mode
            false, // free_mode
        );
        assert!(result.is_ok());
    }

    #[test]
    fn render_status_bar_with_paused() {
        let mut stdout = io::stdout();
        let result = render_status_bar(
            &mut stdout,
            80,
            25,
            true, // paused
            1.0,
            80,
            24,
            80,
            24,
            0,
            0,
            0,
            false,
            false,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn render_status_bar_with_viewport_mode() {
        let mut stdout = io::stdout();
        let result = render_status_bar(
            &mut stdout,
            80,
            25,
            false,
            1.0,
            80,
            24,
            80,
            24,
            0,
            0,
            0,
            true, // viewport_mode
            false,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn render_status_bar_with_free_mode() {
        let mut stdout = io::stdout();
        let result = render_status_bar(
            &mut stdout,
            80,
            25,
            true, // paused (always paused in free mode)
            1.0,
            80,
            24,
            80,
            24,
            0,
            0,
            0,
            false,
            true, // free_mode
        );
        assert!(result.is_ok());
    }

    #[test]
    fn render_status_bar_with_markers() {
        let mut stdout = io::stdout();
        let result = render_status_bar(
            &mut stdout,
            80,
            25,
            false,
            1.0,
            80,
            24,
            80,
            24,
            0,
            0,
            5, // marker_count
            false,
            false,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn render_status_bar_with_scroll_offsets() {
        let mut stdout = io::stdout();
        // Recording larger than viewport to trigger offset display
        let result = render_status_bar(
            &mut stdout,
            80,
            25,
            false,
            1.0,
            120, // rec_cols (larger than view)
            48,  // rec_rows (larger than view)
            80,  // view_cols
            24,  // view_rows
            10,  // col_offset
            5,   // row_offset
            0,
            false,
            false,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn render_status_bar_with_custom_speed() {
        let mut stdout = io::stdout();
        let result = render_status_bar(
            &mut stdout,
            80,
            25,
            false,
            2.5, // custom speed
            80,
            24,
            80,
            24,
            0,
            0,
            0,
            false,
            false,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn render_status_bar_wide_terminal() {
        let mut stdout = io::stdout();
        let result = render_status_bar(
            &mut stdout,
            200, // wide terminal
            25,
            false,
            1.0,
            80,
            24,
            200,
            24,
            0,
            0,
            0,
            false,
            false,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn render_status_bar_narrow_terminal() {
        let mut stdout = io::stdout();
        let result = render_status_bar(
            &mut stdout,
            40, // narrow terminal
            25,
            false,
            1.0,
            80,
            24,
            40,
            24,
            0,
            0,
            0,
            false,
            false,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn render_status_bar_all_options() {
        let mut stdout = io::stdout();
        // All features at once
        let result = render_status_bar(
            &mut stdout,
            100,
            25,
            true,  // paused
            3.0,   // speed
            120,   // rec_cols (larger)
            48,    // rec_rows (larger)
            100,   // view_cols
            30,    // view_rows
            15,    // col_offset
            10,    // row_offset
            7,     // marker_count
            true,  // viewport_mode
            false, // free_mode (can't be both)
        );
        assert!(result.is_ok());
    }

    #[test]
    fn count_digits_handles_powers_of_ten() {
        // Specific edge cases around powers of 10
        assert_eq!(count_digits(0), 1);
        assert_eq!(count_digits(1), 1);
        assert_eq!(count_digits(10), 2);
        assert_eq!(count_digits(100), 3);
        assert_eq!(count_digits(1000), 4);
        assert_eq!(count_digits(10000), 5);
        assert_eq!(count_digits(100000), 6);
    }

    #[test]
    fn count_digits_just_below_powers() {
        assert_eq!(count_digits(9), 1);
        assert_eq!(count_digits(99), 2);
        assert_eq!(count_digits(999), 3);
        assert_eq!(count_digits(9999), 4);
        assert_eq!(count_digits(99999), 5);
    }
}
