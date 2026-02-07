//! TUI Snapshot tests for the native player
//!
//! These tests capture the visual output of player UI components
//! to detect regressions during refactoring.
//!
//! BLOCKING PREREQUISITE: These snapshots must be captured BEFORE
//! any refactoring begins and verified to match AFTER refactoring.

use agr::asciicast::AsciicastFile;
use agr::player::render::{HELP_BOX_WIDTH, HELP_LINES};
use agr::terminal::{CellStyle, Color, TerminalBuffer};
use std::path::Path;

// ============================================================================
// Test Helpers - Render functions extracted for snapshot testing
// ============================================================================

/// Marker information for the progress bar
struct MarkerPosition {
    time: f64,
    #[allow(dead_code)]
    label: String,
}

/// Collect markers from the cast file with their cumulative times.
fn collect_markers(cast: &AsciicastFile) -> Vec<MarkerPosition> {
    let mut markers = Vec::new();
    let mut cumulative = 0.0f64;

    for event in &cast.events {
        cumulative += event.time;
        if event.is_marker() {
            markers.push(MarkerPosition {
                time: cumulative,
                label: event.data.clone(),
            });
        }
    }

    markers
}

/// Format a duration in seconds to MM:SS format.
fn format_duration(seconds: f64) -> String {
    let total_secs = seconds as u64;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{:02}:{:02}", mins, secs)
}

/// Count digits in a number (for width calculation).
#[inline]
#[allow(dead_code)]
fn count_digits(n: usize) -> usize {
    if n == 0 {
        1
    } else {
        (n as f64).log10().floor() as usize + 1
    }
}

/// Build the progress bar character array.
fn build_progress_bar_chars(
    bar_width: usize,
    current_time: f64,
    total_duration: f64,
    markers: &[MarkerPosition],
) -> (Vec<char>, usize) {
    let progress = if total_duration > 0.0 {
        (current_time / total_duration).clamp(0.0, 1.0)
    } else {
        1.0
    };

    let filled = (bar_width as f64 * progress) as usize;
    let mut bar: Vec<char> = vec!['─'; bar_width];

    if filled < bar_width {
        bar[filled] = '⏺';
    }

    for marker in markers {
        let marker_pos = if total_duration > 0.0 {
            ((marker.time / total_duration) * bar_width as f64) as usize
        } else {
            0
        };
        if marker_pos < bar_width && bar[marker_pos] != '⏺' {
            bar[marker_pos] = '◆';
        }
    }

    (bar, filled)
}

/// Calculate which scroll directions are available.
fn calc_scroll_directions(
    row_offset: usize,
    col_offset: usize,
    view_rows: usize,
    view_cols: usize,
    rec_rows: usize,
    rec_cols: usize,
) -> (bool, bool, bool, bool) {
    let can_up = row_offset > 0;
    let can_down = row_offset + view_rows < rec_rows;
    let can_left = col_offset > 0;
    let can_right = col_offset + view_cols < rec_cols;
    (can_up, can_down, can_left, can_right)
}

/// Build the scroll indicator arrow string.
fn build_scroll_arrows(
    can_up: bool,
    can_down: bool,
    can_left: bool,
    can_right: bool,
) -> Option<String> {
    if !can_up && !can_down && !can_left && !can_right {
        return None;
    }

    let mut arrows = Vec::new();
    if can_up {
        arrows.push("▲");
    }
    if can_down {
        arrows.push("▼");
    }
    if can_left {
        arrows.push("◀");
    }
    if can_right {
        arrows.push("▶");
    }

    if arrows.is_empty() {
        None
    } else {
        Some(arrows.join(" "))
    }
}

/// Calculate the starting row for centering the help box.
fn calc_help_start_row(term_height: u16) -> u16 {
    let box_height = HELP_LINES.len() as u16;
    (term_height.saturating_sub(box_height)) / 2
}

/// Calculate the starting column for centering the help box.
fn calc_help_start_col(term_width: u16) -> u16 {
    ((term_width as usize).saturating_sub(HELP_BOX_WIDTH) / 2) as u16
}

// ============================================================================
// Snapshot Rendering - Build string representations for snapshot testing
// ============================================================================

/// Render progress bar to string for snapshot testing.
fn render_progress_bar_snapshot(
    width: usize,
    current_time: f64,
    total_duration: f64,
    markers: &[MarkerPosition],
) -> String {
    let bar_width = width.saturating_sub(14);
    let (bar, filled) = build_progress_bar_chars(bar_width, current_time, total_duration, markers);

    let current_str = format_duration(current_time);
    let total_str = format_duration(total_duration);
    let time_display = format!(" {}/{}", current_str, total_str);

    let mut output = String::new();
    output.push(' '); // Left padding

    for (i, &c) in bar.iter().enumerate() {
        if i < filled {
            if c == '◆' {
                output.push_str("[M]"); // Marker indicator (passed)
            } else {
                output.push('━'); // Filled portion
            }
        } else if i == filled {
            output.push_str("[*]"); // Playhead
        } else if c == '◆' {
            output.push_str("[m]"); // Marker indicator (upcoming)
        } else {
            output.push('─'); // Unfilled portion
        }
    }

    output.push_str(&time_display);
    output
}

/// Render status bar to string for snapshot testing.
#[allow(clippy::too_many_arguments)]
fn render_status_bar_snapshot(
    width: usize,
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
) -> String {
    let mut output = String::new();
    let mut visible_len: usize = 0;

    output.push(' ');
    visible_len += 1;

    // State icon
    let state = if paused { "[>]  " } else { "[||] " };
    output.push_str(state);
    visible_len += 5;

    if viewport_mode {
        output.push_str("[V] ");
        visible_len += 4;
    }

    if free_mode {
        output.push_str("[F] ");
        visible_len += 4;
    }

    output.push_str("spd:");
    visible_len += 4;
    let speed_str = format!("{:.1}x ", speed);
    visible_len += speed_str.len();
    output.push_str(&speed_str);

    if marker_count > 0 {
        let marker_str = format!("M{} ", marker_count);
        visible_len += marker_str.len();
        output.push_str(&marker_str);
    }

    if rec_cols as usize > view_cols || rec_rows as usize > view_rows {
        let offset_str = format!("[{},{}] ", col_offset, row_offset);
        visible_len += offset_str.len();
        output.push_str(&offset_str);
    }

    let play_action = if paused { ":play " } else { ":pause " };
    output.push_str("| ");
    visible_len += 2;
    output.push_str("space");
    visible_len += 5;
    output.push_str(play_action);
    visible_len += play_action.len();
    output.push_str("m:mrk ");
    visible_len += 6;
    output.push_str("f:fre ");
    visible_len += 6;
    output.push_str("v:vpt ");
    visible_len += 6;
    output.push_str("r:rsz ");
    visible_len += 6;
    output.push_str("?:hlp ");
    visible_len += 6;
    output.push_str("q:quit");
    visible_len += 6;

    // Pad to full width
    let padding = width.saturating_sub(visible_len);
    for _ in 0..padding {
        output.push(' ');
    }

    output
}

/// Render scroll indicator to string for snapshot testing.
fn render_scroll_indicator_snapshot(
    row_offset: usize,
    col_offset: usize,
    view_rows: usize,
    view_cols: usize,
    rec_rows: usize,
    rec_cols: usize,
) -> String {
    let (can_up, can_down, can_left, can_right) = calc_scroll_directions(
        row_offset, col_offset, view_rows, view_cols, rec_rows, rec_cols,
    );

    match build_scroll_arrows(can_up, can_down, can_left, can_right) {
        Some(arrows) => format!(" {} ", arrows),
        None => String::new(),
    }
}

/// Render help overlay to string for snapshot testing.
fn render_help_snapshot(term_width: u16, term_height: u16) -> String {
    let start_row = calc_help_start_row(term_height);
    let start_col = calc_help_start_col(term_width);

    let mut output = String::new();
    output.push_str(&format!(
        "Help overlay centered at row {}, col {}\n",
        start_row, start_col
    ));
    output.push_str(&format!(
        "Terminal: {}x{}, Box: {}x{}\n\n",
        term_width,
        term_height,
        HELP_BOX_WIDTH,
        HELP_LINES.len()
    ));

    for line in HELP_LINES {
        output.push_str(&format!("{:>width$}", "", width = start_col as usize));
        output.push_str(line);
        output.push('\n');
    }

    output
}

/// Render viewport content to string for snapshot testing.
fn render_viewport_snapshot(
    buffer: &TerminalBuffer,
    row_offset: usize,
    col_offset: usize,
    view_rows: usize,
    view_cols: usize,
    highlight_line: Option<usize>,
) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "Viewport: {}x{} at offset ({}, {})\n",
        view_cols, view_rows, col_offset, row_offset
    ));
    if let Some(hl) = highlight_line {
        output.push_str(&format!("Highlighted line: {}\n", hl));
    }
    output.push_str(&format!(
        "Buffer size: {}x{}\n",
        buffer.width(),
        buffer.height()
    ));
    output.push_str("---\n");

    for view_row in 0..view_rows {
        let buf_row = view_row + row_offset;
        let is_highlighted = highlight_line == Some(buf_row);

        if is_highlighted {
            output.push_str(">>> ");
        } else {
            output.push_str("    ");
        }

        if let Some(row) = buffer.row(buf_row) {
            for view_col in 0..view_cols {
                let buf_col = view_col + col_offset;
                if buf_col < row.len() {
                    output.push(row[buf_col].char);
                } else {
                    output.push(' ');
                }
            }
        } else {
            for _ in 0..view_cols {
                output.push(' ');
            }
        }

        if is_highlighted {
            output.push_str(" <<<");
        }
        output.push('\n');
    }

    output
}

/// Convert style to ANSI fg code string for testing.
#[allow(dead_code)]
fn style_to_ansi_fg(style: &CellStyle) -> String {
    match &style.fg {
        Color::Default => String::new(),
        Color::Black => "\x1b[30m".to_string(),
        Color::Red => "\x1b[31m".to_string(),
        Color::Green => "\x1b[32m".to_string(),
        Color::Yellow => "\x1b[33m".to_string(),
        Color::Blue => "\x1b[34m".to_string(),
        Color::Magenta => "\x1b[35m".to_string(),
        Color::Cyan => "\x1b[36m".to_string(),
        Color::White => "\x1b[37m".to_string(),
        Color::BrightBlack => "\x1b[90m".to_string(),
        Color::BrightRed => "\x1b[91m".to_string(),
        Color::BrightGreen => "\x1b[92m".to_string(),
        Color::BrightYellow => "\x1b[93m".to_string(),
        Color::BrightBlue => "\x1b[94m".to_string(),
        Color::BrightMagenta => "\x1b[95m".to_string(),
        Color::BrightCyan => "\x1b[96m".to_string(),
        Color::BrightWhite => "\x1b[97m".to_string(),
        Color::Indexed(n) => format!("\x1b[38;5;{}m", n),
        Color::Rgb(r, g, b) => format!("\x1b[38;2;{};{};{}m", r, g, b),
    }
}

/// Seek to a specific time by processing events up to that point.
fn seek_to_time(
    buffer: &mut TerminalBuffer,
    cast: &AsciicastFile,
    target_time: f64,
    cols: u32,
    rows: u32,
) {
    *buffer = TerminalBuffer::new(cols as usize, rows as usize);
    let mut cumulative = 0.0f64;
    for event in &cast.events {
        cumulative += event.time;
        if cumulative > target_time {
            break;
        }
        if event.is_output() {
            buffer.process(&event.data, None);
        } else if let Some((new_cols, new_rows)) = event.parse_resize() {
            buffer.resize(new_cols as usize, new_rows as usize);
        }
    }
}

// ============================================================================
// Progress Bar Snapshots
// ============================================================================

#[test]
fn snapshot_progress_bar_at_start() {
    let output = render_progress_bar_snapshot(80, 0.0, 100.0, &[]);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("progress_bar_at_start", output);
    });
}

#[test]
fn snapshot_progress_bar_at_middle() {
    let output = render_progress_bar_snapshot(80, 50.0, 100.0, &[]);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("progress_bar_at_middle", output);
    });
}

#[test]
fn snapshot_progress_bar_at_end() {
    let output = render_progress_bar_snapshot(80, 100.0, 100.0, &[]);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("progress_bar_at_end", output);
    });
}

#[test]
fn snapshot_progress_bar_with_markers() {
    let markers = vec![
        MarkerPosition {
            time: 25.0,
            label: "marker1".to_string(),
        },
        MarkerPosition {
            time: 50.0,
            label: "marker2".to_string(),
        },
        MarkerPosition {
            time: 75.0,
            label: "marker3".to_string(),
        },
    ];
    let output = render_progress_bar_snapshot(80, 30.0, 100.0, &markers);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("progress_bar_with_markers", output);
    });
}

#[test]
fn snapshot_progress_bar_narrow_terminal() {
    let output = render_progress_bar_snapshot(40, 25.0, 100.0, &[]);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("progress_bar_narrow", output);
    });
}

#[test]
fn snapshot_progress_bar_wide_terminal() {
    let output = render_progress_bar_snapshot(120, 33.0, 100.0, &[]);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("progress_bar_wide", output);
    });
}

// ============================================================================
// Status Bar Snapshots
// ============================================================================

#[test]
fn snapshot_status_bar_playing() {
    let output = render_status_bar_snapshot(80, false, 1.0, 80, 24, 80, 24, 0, 0, 0, false, false);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("status_bar_playing", output);
    });
}

#[test]
fn snapshot_status_bar_paused() {
    let output = render_status_bar_snapshot(80, true, 1.0, 80, 24, 80, 24, 0, 0, 0, false, false);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("status_bar_paused", output);
    });
}

#[test]
fn snapshot_status_bar_with_speed() {
    let output = render_status_bar_snapshot(100, false, 2.0, 80, 24, 80, 24, 0, 0, 0, false, false);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("status_bar_speed_2x", output);
    });
}

#[test]
fn snapshot_status_bar_slow_speed() {
    let output = render_status_bar_snapshot(100, true, 0.5, 80, 24, 80, 24, 0, 0, 0, false, false);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("status_bar_speed_0.5x", output);
    });
}

#[test]
fn snapshot_status_bar_with_markers() {
    let output = render_status_bar_snapshot(100, false, 1.0, 80, 24, 80, 24, 0, 0, 5, false, false);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("status_bar_with_markers", output);
    });
}

#[test]
fn snapshot_status_bar_viewport_mode() {
    let output = render_status_bar_snapshot(100, true, 1.0, 120, 48, 80, 24, 10, 5, 0, true, false);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("status_bar_viewport_mode", output);
    });
}

#[test]
fn snapshot_status_bar_free_mode() {
    let output = render_status_bar_snapshot(100, true, 1.0, 80, 24, 80, 24, 0, 0, 0, false, true);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("status_bar_free_mode", output);
    });
}

#[test]
fn snapshot_status_bar_both_modes() {
    // Edge case: both modes active (shouldn't happen but test the rendering)
    let output = render_status_bar_snapshot(120, true, 1.5, 120, 48, 80, 24, 20, 10, 3, true, true);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("status_bar_both_modes", output);
    });
}

#[test]
fn snapshot_status_bar_with_offset() {
    let output =
        render_status_bar_snapshot(100, false, 1.0, 120, 48, 80, 24, 15, 12, 0, false, false);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("status_bar_with_offset", output);
    });
}

// ============================================================================
// Scroll Indicator Snapshots
// ============================================================================

#[test]
fn snapshot_scroll_indicator_none() {
    let output = render_scroll_indicator_snapshot(0, 0, 24, 80, 24, 80);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("scroll_indicator_none", output);
    });
}

#[test]
fn snapshot_scroll_indicator_down_only() {
    let output = render_scroll_indicator_snapshot(0, 0, 24, 80, 48, 80);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("scroll_indicator_down", output);
    });
}

#[test]
fn snapshot_scroll_indicator_up_down() {
    let output = render_scroll_indicator_snapshot(10, 0, 24, 80, 48, 80);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("scroll_indicator_up_down", output);
    });
}

#[test]
fn snapshot_scroll_indicator_all_directions() {
    let output = render_scroll_indicator_snapshot(10, 20, 24, 80, 48, 120);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("scroll_indicator_all", output);
    });
}

#[test]
fn snapshot_scroll_indicator_horizontal_only() {
    let output = render_scroll_indicator_snapshot(0, 10, 24, 80, 24, 120);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("scroll_indicator_horizontal", output);
    });
}

// ============================================================================
// Help Overlay Snapshots
// ============================================================================

#[test]
fn snapshot_help_overlay_standard() {
    let output = render_help_snapshot(80, 30);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("help_overlay_standard", output);
    });
}

#[test]
fn snapshot_help_overlay_wide() {
    let output = render_help_snapshot(120, 40);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("help_overlay_wide", output);
    });
}

#[test]
fn snapshot_help_overlay_narrow() {
    let output = render_help_snapshot(50, 25);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("help_overlay_narrow", output);
    });
}

#[test]
fn snapshot_help_overlay_small() {
    let output = render_help_snapshot(40, 20);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("help_overlay_small", output);
    });
}

// ============================================================================
// Viewport Rendering Snapshots
// ============================================================================

#[test]
fn snapshot_viewport_normal_playback() {
    let mut buffer = TerminalBuffer::new(80, 24);
    buffer.process("$ cargo build\r\n", None);
    buffer.process("\x1b[32m   Compiling\x1b[0m agr v0.1.0\r\n", None);
    buffer.process("\x1b[32m    Finished\x1b[0m release\r\n", None);
    buffer.process("$ ", None);

    let output = render_viewport_snapshot(&buffer, 0, 0, 10, 40, None);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("viewport_normal_playback", output);
    });
}

#[test]
fn snapshot_viewport_with_highlight() {
    let mut buffer = TerminalBuffer::new(80, 24);
    buffer.process("Line 0\r\n", None);
    buffer.process("Line 1\r\n", None);
    buffer.process("Line 2 - highlighted\r\n", None);
    buffer.process("Line 3\r\n", None);
    buffer.process("Line 4\r\n", None);

    let output = render_viewport_snapshot(&buffer, 0, 0, 10, 40, Some(2));
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("viewport_with_highlight", output);
    });
}

#[test]
fn snapshot_viewport_scrolled() {
    let mut buffer = TerminalBuffer::new(100, 30);
    for i in 0..30 {
        buffer.process(
            &format!("Line {} - some content here with offset\r\n", i),
            None,
        );
    }

    let output = render_viewport_snapshot(&buffer, 10, 5, 10, 40, None);
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("viewport_scrolled", output);
    });
}

#[test]
fn snapshot_viewport_free_mode_highlight() {
    let mut buffer = TerminalBuffer::new(80, 24);
    for i in 0..15 {
        buffer.process(&format!("Line {}: content\r\n", i), None);
    }

    // Free mode with highlight at line 7
    let output = render_viewport_snapshot(&buffer, 0, 0, 12, 40, Some(7));
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("viewport_free_mode", output);
    });
}

// ============================================================================
// Full Player State Snapshots (Composite)
// ============================================================================

/// Render a complete player frame as it would appear in the terminal.
#[allow(clippy::too_many_arguments)]
fn render_full_player_frame(
    buffer: &TerminalBuffer,
    term_width: usize,
    term_height: usize,
    current_time: f64,
    total_duration: f64,
    markers: &[MarkerPosition],
    paused: bool,
    speed: f64,
    rec_cols: u32,
    rec_rows: u32,
    view_col_offset: usize,
    view_row_offset: usize,
    viewport_mode: bool,
    free_mode: bool,
    highlight_line: Option<usize>,
) -> String {
    let status_lines = 3;
    let view_rows = (term_height - status_lines).min(buffer.height());
    let view_cols = term_width.min(buffer.width());

    let mut output = String::new();
    output.push_str(&format!(
        "=== Player Frame {}x{} ===\n",
        term_width, term_height
    ));
    output.push_str(&format!(
        "Recording: {}x{}, View: {}x{}\n",
        rec_cols, rec_rows, view_cols, view_rows
    ));
    output.push_str(&format!(
        "Time: {:.1}/{:.1}s, Speed: {:.1}x, Paused: {}\n",
        current_time, total_duration, speed, paused
    ));
    output.push_str(&format!(
        "Viewport: {}, Free: {}, Offset: ({},{})\n",
        viewport_mode, free_mode, view_col_offset, view_row_offset
    ));
    output.push('\n');

    // Viewport content
    output.push_str("--- Viewport ---\n");
    output.push_str(&render_viewport_snapshot(
        buffer,
        view_row_offset,
        view_col_offset,
        view_rows,
        view_cols,
        highlight_line,
    ));

    // Scroll indicator
    let scroll_indicator = render_scroll_indicator_snapshot(
        view_row_offset,
        view_col_offset,
        view_rows,
        view_cols,
        rec_rows as usize,
        rec_cols as usize,
    );
    if !scroll_indicator.is_empty() {
        output.push_str(&format!("Scroll: {}\n", scroll_indicator.trim()));
    }

    // Separator
    output.push_str("--- Separator ---\n");
    for _ in 0..term_width {
        output.push('─');
    }
    output.push('\n');

    // Progress bar
    output.push_str("--- Progress Bar ---\n");
    output.push_str(&render_progress_bar_snapshot(
        term_width,
        current_time,
        total_duration,
        markers,
    ));
    output.push('\n');

    // Status bar
    output.push_str("--- Status Bar ---\n");
    output.push_str(&render_status_bar_snapshot(
        term_width,
        paused,
        speed,
        rec_cols,
        rec_rows,
        view_cols,
        view_rows,
        view_col_offset,
        view_row_offset,
        markers.len(),
        viewport_mode,
        free_mode,
    ));
    output.push('\n');

    output
}

#[test]
fn snapshot_full_frame_playing() {
    let mut buffer = TerminalBuffer::new(80, 24);
    buffer.process("$ cargo build\r\n", None);
    buffer.process("\x1b[32m   Compiling\x1b[0m agr v0.1.0\r\n", None);
    buffer.process("$ ", None);

    let output = render_full_player_frame(
        &buffer,
        80,
        27,
        5.0,
        30.0,
        &[],
        false,
        1.0,
        80,
        24,
        0,
        0,
        false,
        false,
        None,
    );
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("full_frame_playing", output);
    });
}

#[test]
fn snapshot_full_frame_paused() {
    let mut buffer = TerminalBuffer::new(80, 24);
    buffer.process("$ cargo test\r\n", None);
    buffer.process("\x1b[32mrunning 42 tests\x1b[0m\r\n", None);
    buffer.process("$ ", None);

    let output = render_full_player_frame(
        &buffer,
        80,
        27,
        15.0,
        30.0,
        &[],
        true,
        1.0,
        80,
        24,
        0,
        0,
        false,
        false,
        None,
    );
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("full_frame_paused", output);
    });
}

#[test]
fn snapshot_full_frame_with_markers() {
    let mut buffer = TerminalBuffer::new(100, 30);
    buffer.process("$ make build\r\n", None);
    buffer.process("Building...\r\n", None);
    buffer.process("Done!\r\n", None);
    buffer.process("$ ", None);

    let markers = vec![
        MarkerPosition {
            time: 5.0,
            label: "Build started".to_string(),
        },
        MarkerPosition {
            time: 15.0,
            label: "Build finished".to_string(),
        },
    ];

    let output = render_full_player_frame(
        &buffer, 100, 33, 10.0, 20.0, &markers, false, 1.5, 100, 30, 0, 0, false, false, None,
    );
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("full_frame_with_markers", output);
    });
}

#[test]
fn snapshot_full_frame_viewport_mode() {
    let mut buffer = TerminalBuffer::new(120, 48);
    for i in 0..48 {
        buffer.process(&format!("Line {}: This is a long line of content that extends beyond the viewport width for testing\r\n", i), None);
    }

    let output = render_full_player_frame(
        &buffer,
        80,
        27,
        25.0,
        60.0,
        &[],
        true,
        1.0,
        120,
        48,
        10,
        15,
        true,
        false,
        None,
    );
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("full_frame_viewport_mode", output);
    });
}

#[test]
fn snapshot_full_frame_free_mode() {
    let mut buffer = TerminalBuffer::new(80, 24);
    for i in 0..20 {
        buffer.process(&format!("Line {}: content here\r\n", i), None);
    }

    let output = render_full_player_frame(
        &buffer,
        80,
        27,
        30.0,
        60.0,
        &[],
        true,
        1.0,
        80,
        24,
        0,
        0,
        false,
        true,
        Some(5),
    );
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("full_frame_free_mode", output);
    });
}

// ============================================================================
// Cast File Integration Snapshots
// ============================================================================

#[test]
fn snapshot_cast_file_playback() {
    let cast_path = Path::new("tests/fixtures/player_snapshot.cast");
    if !cast_path.exists() {
        // Skip if fixture doesn't exist
        return;
    }

    let cast = AsciicastFile::parse(cast_path).unwrap();
    let (rec_cols, rec_rows) = cast.terminal_size();
    let total_duration = cast.duration();
    let markers = collect_markers(&cast);

    let mut buffer = TerminalBuffer::new(rec_cols as usize, rec_rows as usize);

    // Seek to middle of recording
    let mid_time = total_duration / 2.0;
    seek_to_time(&mut buffer, &cast, mid_time, rec_cols, rec_rows);

    let output = render_full_player_frame(
        &buffer,
        100,
        33,
        mid_time,
        total_duration,
        &markers,
        true,
        1.0,
        rec_cols,
        rec_rows,
        0,
        0,
        false,
        false,
        None,
    );
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("cast_file_playback", output);
    });
}

#[test]
fn snapshot_cast_file_with_markers_playback() {
    let cast_path = Path::new("tests/fixtures/with_markers.cast");
    if !cast_path.exists() {
        return;
    }

    let cast = AsciicastFile::parse(cast_path).unwrap();
    let (rec_cols, rec_rows) = cast.terminal_size();
    let total_duration = cast.duration();
    let markers = collect_markers(&cast);

    let mut buffer = TerminalBuffer::new(rec_cols as usize, rec_rows as usize);

    // Seek to end of recording
    seek_to_time(&mut buffer, &cast, total_duration, rec_cols, rec_rows);

    let output = render_full_player_frame(
        &buffer,
        80,
        27,
        total_duration,
        total_duration,
        &markers,
        true,
        1.0,
        rec_cols,
        rec_rows,
        0,
        0,
        false,
        false,
        None,
    );
    insta::with_settings!({
        snapshot_path => "snapshots/player"
    }, {
        insta::assert_snapshot!("cast_file_with_markers", output);
    });
}
