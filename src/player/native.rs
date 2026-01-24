//! Native asciicast player
//!
//! Full-featured player with:
//! - Size-independent rendering via virtual terminal
//! - Progress bar with marker indicators
//! - Seeking and speed control
//! - Viewport scrolling
//! - Help overlay

use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::asciicast::AsciicastFile;

use super::terminal::{CellStyle, Color as TermColor, TerminalBuffer};

/// Result of a playback operation
#[derive(Debug, Clone)]
pub enum PlaybackResult {
    /// Playback completed successfully
    Success(String),
    /// Playback was interrupted (e.g., user pressed q)
    Interrupted,
    /// Playback failed with an error
    Error(String),
}

impl PlaybackResult {
    /// Get a human-readable message for this result
    pub fn message(&self) -> String {
        match self {
            PlaybackResult::Success(name) => format!("Played: {}", name),
            PlaybackResult::Interrupted => "Playback interrupted".to_string(),
            PlaybackResult::Error(e) => format!("Failed to play: {}", e),
        }
    }
}

/// Marker information for the progress bar
struct MarkerPosition {
    /// Cumulative time when the marker occurs
    time: f64,
    /// Marker label
    #[allow(dead_code)]
    label: String,
}

/// Play a session using the native renderer (default).
pub fn play_session(path: &Path) -> Result<PlaybackResult> {
    play_session_native(path)
}

/// Play a session using the native renderer.
///
/// This renders the recording through a virtual terminal buffer, allowing
/// playback at any terminal size. The virtual terminal matches the original
/// recording dimensions, and a viewport shows the visible portion.
///
/// Controls:
/// - q/Esc: Quit
/// - Space: Pause/resume
/// - Arrow keys: Seek (or scroll in viewport mode)
/// - +/-: Adjust speed
/// - m: Jump to next marker
/// - </> or ,/.: Seek backward/forward 5s
/// - Home/End: Go to start/end
/// - v: Toggle viewport mode
/// - r: Resize terminal to recording size
/// - ?: Show help
pub fn play_session_native(path: &Path) -> Result<PlaybackResult> {
    let cast = AsciicastFile::parse(path)?;
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // Get recording dimensions and duration
    let (rec_cols, rec_rows) = cast.terminal_size();
    let total_duration = cast.duration();

    // Collect marker positions
    let markers = collect_markers(&cast);

    // Create virtual terminal at recording size
    let mut buffer = TerminalBuffer::new(rec_cols as usize, rec_rows as usize);

    // Get current terminal size for viewport
    let (mut term_cols, mut term_rows) = crossterm::terminal::size()?;
    let status_lines: u16 = 3; // Separator + progress bar + status bar
    let mut view_rows = (term_rows.saturating_sub(status_lines)) as usize;
    let mut view_cols = term_cols as usize;

    // Viewport offset (for scrolling) - start at bottom to see input
    let max_row_offset = (rec_rows as usize).saturating_sub(view_rows);
    let mut view_row_offset: usize = max_row_offset;
    let mut view_col_offset: usize = 0;

    // Playback state
    let mut paused = false;
    let mut speed = 1.0f64;
    let mut event_idx = 0;
    let mut current_time = 0.0f64;
    let mut cumulative_time = 0.0f64; // Track cumulative event time at current event_idx
    let mut show_help = false;
    let mut viewport_mode = false;
    let mut free_mode = false;
    let mut free_line: usize = 0; // Highlighted line in free mode (buffer row)
    let mut prev_free_line: usize = 0; // Previous highlight for partial updates
    let mut start_time = Instant::now();
    let mut time_offset = 0.0f64;
    let mut needs_render = true; // Track when screen needs redraw
    let mut free_line_only = false; // True if only free_line changed (partial update)

    // Setup terminal
    let mut stdout = io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, Hide)?;

    let result = (|| -> Result<PlaybackResult> {
        // Helper closure to process events up to a time
        let process_up_to_time =
            |buffer: &mut TerminalBuffer, target_time: f64, cast: &AsciicastFile| {
                let mut cumulative = 0.0f64;
                for event in &cast.events {
                    cumulative += event.time;
                    if cumulative > target_time {
                        break;
                    }
                    if event.is_output() {
                        buffer.process(&event.data);
                    }
                }
            };

        loop {
            // Handle all pending input events before rendering
            // First poll waits up to 16ms, then drain any queued events with zero timeout
            let mut first_poll = true;
            while event::poll(if first_poll {
                Duration::from_millis(16)
            } else {
                Duration::ZERO
            })? {
                first_poll = false;
                match event::read()? {
                    Event::Resize(new_cols, new_rows) => {
                        // Terminal was resized - update view dimensions
                        term_cols = new_cols;
                        term_rows = new_rows;
                        view_rows = (new_rows.saturating_sub(status_lines)) as usize;
                        view_cols = new_cols as usize;
                        // Clamp viewport offset to valid range
                        let max_row_offset = (rec_rows as usize).saturating_sub(view_rows);
                        let max_col_offset = (rec_cols as usize).saturating_sub(view_cols);
                        view_row_offset = view_row_offset.min(max_row_offset);
                        view_col_offset = view_col_offset.min(max_col_offset);
                        needs_render = true;
                    }
                    Event::Key(key) => {
                        if show_help {
                            show_help = false;
                            needs_render = true;
                            continue;
                        }

                        match key.code {
                            KeyCode::Char('q') => {
                                return Ok(PlaybackResult::Interrupted);
                            }
                            KeyCode::Esc => {
                                if viewport_mode {
                                    viewport_mode = false;
                                } else if free_mode {
                                    free_mode = false;
                                } else {
                                    return Ok(PlaybackResult::Interrupted);
                                }
                            }
                            KeyCode::Char('v') => {
                                viewport_mode = !viewport_mode;
                                if viewport_mode {
                                    free_mode = false; // Exit free mode when entering viewport mode
                                }
                            }
                            KeyCode::Char('f') => {
                                free_mode = !free_mode;
                                if free_mode {
                                    viewport_mode = false; // Exit viewport mode when entering free mode
                                    paused = true; // Enforce pause in free mode
                                                   // Start at current cursor position or middle of viewport
                                    free_line = buffer.cursor_row();
                                }
                            }
                            KeyCode::Char(' ') => {
                                paused = !paused;
                                if !paused {
                                    // Exit free mode when resuming playback
                                    free_mode = false;
                                    // Reset timing when resuming
                                    start_time = Instant::now();
                                    time_offset = current_time;
                                }
                            }
                            KeyCode::Char('+') | KeyCode::Char('=') => {
                                speed = (speed * 1.5).min(16.0);
                            }
                            KeyCode::Char('-') | KeyCode::Char('_') => {
                                speed = (speed / 1.5).max(0.1);
                            }
                            KeyCode::Char('?') => {
                                show_help = true;
                            }
                            KeyCode::Char('r') => {
                                // Resize terminal to match recording size
                                let target_rows = rec_rows + status_lines as u32;
                                write!(stdout, "\x1b[8;{};{}t", target_rows, rec_cols)?;
                                stdout.flush()?;
                                // Small delay for terminal to resize
                                std::thread::sleep(Duration::from_millis(50));
                                // Update view dimensions after resize
                                if let Ok((new_cols, new_rows)) = crossterm::terminal::size() {
                                    term_cols = new_cols;
                                    term_rows = new_rows;
                                    view_rows = (new_rows.saturating_sub(status_lines)) as usize;
                                    view_cols = new_cols as usize;
                                    // Reset viewport offset since we now fit
                                    if view_rows >= rec_rows as usize {
                                        view_row_offset = 0;
                                    }
                                    if view_cols >= rec_cols as usize {
                                        view_col_offset = 0;
                                    }
                                }
                            }
                            // Marker navigation (forward only)
                            KeyCode::Char('m') => {
                                if let Some(next) =
                                    markers.iter().find(|m| m.time > current_time + 0.1)
                                {
                                    seek_to_time(&mut buffer, &cast, next.time, rec_cols, rec_rows);
                                    current_time = next.time;
                                    time_offset = current_time;
                                    (event_idx, cumulative_time) =
                                        find_event_index_at_time(&cast, current_time);
                                    paused = true;
                                }
                            }
                            // Seeking
                            KeyCode::Char('<') | KeyCode::Char(',') => {
                                let new_time = (current_time - 5.0).max(0.0);
                                seek_to_time(&mut buffer, &cast, new_time, rec_cols, rec_rows);
                                current_time = new_time;
                                time_offset = current_time;
                                start_time = Instant::now();
                                (event_idx, cumulative_time) =
                                    find_event_index_at_time(&cast, current_time);
                            }
                            KeyCode::Char('>') | KeyCode::Char('.') => {
                                let new_time = (current_time + 5.0).min(total_duration);
                                current_time = new_time;
                                time_offset = current_time;
                                start_time = Instant::now();
                                (event_idx, cumulative_time) =
                                    find_event_index_at_time(&cast, current_time);
                                buffer = TerminalBuffer::new(rec_cols as usize, rec_rows as usize);
                                process_up_to_time(&mut buffer, current_time, &cast);
                            }
                            // Arrow keys: seek by default, viewport scroll in viewport mode
                            // Shift+Arrow: seek by 5% of total duration
                            KeyCode::Left => {
                                if viewport_mode {
                                    view_col_offset = view_col_offset.saturating_sub(1);
                                } else {
                                    let step = if key.modifiers.contains(KeyModifiers::SHIFT) {
                                        total_duration * 0.05 // 5% jump
                                    } else {
                                        5.0 // 5 seconds
                                    };
                                    let new_time = (current_time - step).max(0.0);
                                    seek_to_time(&mut buffer, &cast, new_time, rec_cols, rec_rows);
                                    current_time = new_time;
                                    time_offset = current_time;
                                    start_time = Instant::now();
                                    (event_idx, cumulative_time) =
                                        find_event_index_at_time(&cast, current_time);
                                }
                            }
                            KeyCode::Right => {
                                if viewport_mode {
                                    let max_offset = (rec_cols as usize).saturating_sub(view_cols);
                                    view_col_offset = (view_col_offset + 1).min(max_offset);
                                } else {
                                    let step = if key.modifiers.contains(KeyModifiers::SHIFT) {
                                        total_duration * 0.05 // 5% jump
                                    } else {
                                        5.0 // 5 seconds
                                    };
                                    let new_time = (current_time + step).min(total_duration);
                                    current_time = new_time;
                                    time_offset = current_time;
                                    start_time = Instant::now();
                                    (event_idx, cumulative_time) =
                                        find_event_index_at_time(&cast, current_time);
                                    buffer =
                                        TerminalBuffer::new(rec_cols as usize, rec_rows as usize);
                                    process_up_to_time(&mut buffer, current_time, &cast);
                                }
                            }
                            KeyCode::Up => {
                                if free_mode {
                                    // Move highlight up one line
                                    let old_offset = view_row_offset;
                                    prev_free_line = free_line;
                                    free_line = free_line.saturating_sub(1);
                                    // Auto-scroll viewport to keep highlighted line visible
                                    if free_line < view_row_offset {
                                        view_row_offset = free_line;
                                    }
                                    // If viewport didn't scroll, only update highlight lines
                                    if view_row_offset == old_offset && prev_free_line != free_line
                                    {
                                        free_line_only = true;
                                    }
                                } else if viewport_mode {
                                    view_row_offset = view_row_offset.saturating_sub(1);
                                }
                            }
                            KeyCode::Down => {
                                if free_mode {
                                    // Move highlight down one line
                                    let old_offset = view_row_offset;
                                    prev_free_line = free_line;
                                    let max_line = (rec_rows as usize).saturating_sub(1);
                                    free_line = (free_line + 1).min(max_line);
                                    // Auto-scroll viewport to keep highlighted line visible
                                    if free_line >= view_row_offset + view_rows {
                                        view_row_offset = free_line - view_rows + 1;
                                    }
                                    // If viewport didn't scroll, only update highlight lines
                                    if view_row_offset == old_offset && prev_free_line != free_line
                                    {
                                        free_line_only = true;
                                    }
                                } else if viewport_mode {
                                    let max_offset = (rec_rows as usize).saturating_sub(view_rows);
                                    view_row_offset = (view_row_offset + 1).min(max_offset);
                                }
                            }
                            KeyCode::Home => {
                                seek_to_time(&mut buffer, &cast, 0.0, rec_cols, rec_rows);
                                current_time = 0.0;
                                time_offset = 0.0;
                                start_time = Instant::now();
                                event_idx = 0;
                                cumulative_time = 0.0;
                                view_row_offset = 0;
                                view_col_offset = 0;
                            }
                            KeyCode::End => {
                                buffer = TerminalBuffer::new(rec_cols as usize, rec_rows as usize);
                                process_up_to_time(&mut buffer, total_duration, &cast);
                                current_time = total_duration;
                                time_offset = current_time;
                                event_idx = cast.events.len();
                                cumulative_time = total_duration;
                                paused = true;
                            }
                            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                return Ok(PlaybackResult::Interrupted);
                            }
                            _ => {}
                        }
                        needs_render = true;
                    }
                    _ => {} // Ignore other events (mouse, focus, etc.)
                }
            }

            // Process events if not paused
            if !paused {
                let elapsed = start_time.elapsed().as_secs_f64() * speed + time_offset;
                // Cap elapsed time to total duration
                let elapsed = elapsed.min(total_duration);
                current_time = elapsed;
                needs_render = true; // Always render when playing (time changes)

                while event_idx < cast.events.len() {
                    let evt = &cast.events[event_idx];
                    let next_time = cumulative_time + evt.time;

                    if next_time > elapsed {
                        break;
                    }

                    cumulative_time = next_time;

                    if evt.is_output() {
                        buffer.process(&evt.data);
                    }

                    event_idx += 1;
                }
            }

            // Render only when needed
            if !needs_render {
                std::thread::sleep(Duration::from_millis(8));
                continue;
            }
            needs_render = false;

            if show_help {
                render_help(&mut stdout, term_cols, term_rows)?;
            } else {
                // Begin synchronized update to prevent flicker
                write!(stdout, "\x1b[?2026h")?;

                // Partial update: only re-render changed highlight lines in free mode
                // Skip all UI chrome (progress bar, status bar, etc.) for partial updates
                if free_line_only && free_mode {
                    render_single_line(
                        &mut stdout,
                        &buffer,
                        prev_free_line,
                        view_row_offset,
                        view_col_offset,
                        view_cols,
                        false, // not highlighted
                    )?;
                    render_single_line(
                        &mut stdout,
                        &buffer,
                        free_line,
                        view_row_offset,
                        view_col_offset,
                        view_cols,
                        true, // highlighted
                    )?;
                    free_line_only = false;
                    // End synchronized update and skip UI chrome
                    write!(stdout, "\x1b[?2026l")?;
                    stdout.flush()?;
                    continue; // Skip the sleep at end of loop for faster response
                } else {
                    render_viewport(
                        &mut stdout,
                        &buffer,
                        view_row_offset,
                        view_col_offset,
                        view_rows,
                        view_cols,
                        if free_mode { Some(free_line) } else { None },
                    )?;

                    // Show scroll indicator if viewport can scroll
                    render_scroll_indicator(
                        &mut stdout,
                        term_cols,
                        view_row_offset,
                        view_col_offset,
                        view_rows,
                        view_cols,
                        rec_rows as usize,
                        rec_cols as usize,
                    )?;

                    render_separator_line(&mut stdout, term_cols, term_rows - 3)?;

                    render_progress_bar(
                        &mut stdout,
                        term_cols,
                        term_rows - 2,
                        current_time,
                        total_duration,
                        &markers,
                    )?;

                    render_status_bar(
                        &mut stdout,
                        term_cols,
                        term_rows - 1,
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
                    )?;

                    // End synchronized update
                    write!(stdout, "\x1b[?2026l")?;
                }
            }

            stdout.flush()?;

            if event_idx >= cast.events.len() && !paused {
                std::thread::sleep(Duration::from_millis(500));
                return Ok(PlaybackResult::Success(name));
            }

            std::thread::sleep(Duration::from_millis(8));
        }
    })();

    // Cleanup
    execute!(stdout, Show, LeaveAlternateScreen)?;
    crossterm::terminal::disable_raw_mode()?;

    result
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

/// Find the event index and cumulative time at a given target time.
/// Returns (event_index, cumulative_time_before_that_event)
fn find_event_index_at_time(cast: &AsciicastFile, target_time: f64) -> (usize, f64) {
    let mut cumulative = 0.0f64;
    for (i, event) in cast.events.iter().enumerate() {
        let next_cumulative = cumulative + event.time;
        if next_cumulative > target_time {
            return (i, cumulative);
        }
        cumulative = next_cumulative;
    }
    (cast.events.len(), cumulative)
}

/// Seek to a specific time by re-rendering the buffer from scratch.
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
            buffer.process(&event.data);
        }
    }
}

/// Render the progress bar with markers (no background).
fn render_progress_bar(
    stdout: &mut io::Stdout,
    width: u16,
    row: u16,
    current_time: f64,
    total_duration: f64,
    markers: &[MarkerPosition],
) -> Result<()> {
    let bar_width = (width as usize).saturating_sub(14); // Account for padding and time display
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

    let current_str = format_duration(current_time);
    let total_str = format_duration(total_duration);
    let time_display = format!(" {}/{}", current_str, total_str);

    // Build output string
    let mut output = String::with_capacity(width as usize * 4);
    output.push_str(&format!("\x1b[{};1H", row + 1)); // Move cursor
    output.push_str("\x1b[48;5;236m "); // Dark gray background + padding

    // ANSI color codes
    const GREEN: &str = "\x1b[32m";
    const YELLOW: &str = "\x1b[33m";
    const WHITE: &str = "\x1b[97m";
    const DARK_GREY: &str = "\x1b[90m";
    const GREY: &str = "\x1b[37m";

    output.push_str(GREEN);
    for (i, &c) in bar.iter().enumerate() {
        if i < filled {
            if c == '◆' {
                output.push_str(YELLOW);
                output.push(c);
                output.push_str(GREEN);
            } else {
                output.push('━');
            }
        } else if i == filled {
            output.push_str(WHITE);
            output.push(c);
        } else if c == '◆' {
            output.push_str(YELLOW);
            output.push(c);
        } else {
            output.push_str(DARK_GREY);
            output.push(c);
        }
    }

    output.push_str(GREY);
    output.push_str(&time_display);

    // Fill remaining width
    let used_width = 1 + bar_width + time_display.len();
    let remaining = (width as usize).saturating_sub(used_width);
    for _ in 0..remaining {
        output.push(' ');
    }

    output.push_str("\x1b[0m"); // Reset
    write!(stdout, "{}", output)?;

    Ok(())
}

/// Render scroll indicator in top-right showing available scroll directions.
#[allow(clippy::too_many_arguments)]
fn render_scroll_indicator(
    stdout: &mut io::Stdout,
    term_cols: u16,
    row_offset: usize,
    col_offset: usize,
    view_rows: usize,
    view_cols: usize,
    rec_rows: usize,
    rec_cols: usize,
) -> Result<()> {
    // Calculate which directions have more content
    let can_up = row_offset > 0;
    let can_down = row_offset + view_rows < rec_rows;
    let can_left = col_offset > 0;
    let can_right = col_offset + view_cols < rec_cols;

    // Only show if there's something to scroll
    if !can_up && !can_down && !can_left && !can_right {
        return Ok(());
    }

    // Build arrow string with only active arrows (with spacing)
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
        return Ok(());
    }

    let arrow_str = arrows.join(" ");

    // Draw at top-right, completely aligned to edge
    let arrow_color = Color::Yellow;
    let bg_color = Color::AnsiValue(236); // Same as progress bar
                                          // Width = arrows + spaces between + padding on sides
    let display_width = (arrows.len() * 2 + 1) as u16; // each arrow + space, plus padding
    let start_col = term_cols.saturating_sub(display_width);

    execute!(
        stdout,
        MoveTo(start_col, 0),
        SetBackgroundColor(bg_color),
        SetForegroundColor(arrow_color),
        Print(" "),
        Print(&arrow_str),
        Print(" "),
        ResetColor,
    )?;
    Ok(())
}

/// Render a separator line.
fn render_separator_line(stdout: &mut io::Stdout, width: u16, row: u16) -> Result<()> {
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
#[allow(clippy::too_many_arguments)]
fn render_status_bar(
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

/// Count digits in a number (for width calculation).
#[inline]
fn count_digits(n: usize) -> usize {
    if n == 0 {
        1
    } else {
        (n as f64).log10().floor() as usize + 1
    }
}

/// Render the help overlay.
fn render_help(stdout: &mut io::Stdout, width: u16, height: u16) -> Result<()> {
    let help_lines = [
        "",
        "  ╔═══════════════════════════════════════════╗",
        "  ║          AGR Native Player Help           ║",
        "  ╠═══════════════════════════════════════════╣",
        "  ║                                           ║",
        "  ║  Playback                                 ║",
        "  ║    Space      Pause / Resume              ║",
        "  ║    ←/→        Seek ±5s                    ║",
        "  ║    Shift+←/→  Seek ±5%                    ║",
        "  ║    +/-        Speed up / down             ║",
        "  ║    Home/End   Go to start / end           ║",
        "  ║                                           ║",
        "  ║  Markers                                  ║",
        "  ║    m          Jump to next marker         ║",
        "  ║                                           ║",
        "  ║  Free Mode (line-by-line navigation)       ║",
        "  ║    f          Toggle free mode            ║",
        "  ║    ↑/↓        Move highlight up/down      ║",
        "  ║    Esc        Exit free mode              ║",
        "  ║                                           ║",
        "  ║  Viewport                                 ║",
        "  ║    v          Toggle viewport mode        ║",
        "  ║    ↑↓←→       Scroll viewport (v mode)    ║",
        "  ║    r          Resize to recording         ║",
        "  ║    Esc        Exit viewport mode          ║",
        "  ║                                           ║",
        "  ║  General                                  ║",
        "  ║    ?          Show this help              ║",
        "  ║    q          Quit player                 ║",
        "  ║                                           ║",
        "  ║         Press any key to close            ║",
        "  ╚═══════════════════════════════════════════╝",
        "",
    ];

    let box_height = help_lines.len() as u16;
    let start_row = (height.saturating_sub(box_height)) / 2;

    execute!(stdout, Clear(ClearType::All))?;

    for (i, line) in help_lines.iter().enumerate() {
        let row = start_row + i as u16;
        let col = (width as usize).saturating_sub(47) / 2;
        execute!(
            stdout,
            MoveTo(col as u16, row),
            SetForegroundColor(Color::Green),
            Print(line),
            ResetColor,
        )?;
    }

    Ok(())
}

/// Format a duration in seconds to MM:SS format.
fn format_duration(seconds: f64) -> String {
    let total_secs = seconds as u64;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{:02}:{:02}", mins, secs)
}

/// Render a viewport of the terminal buffer to stdout.
/// If `highlight_line` is Some, that line (in buffer coordinates) gets a green background.
#[allow(clippy::too_many_arguments)]
fn render_viewport(
    stdout: &mut io::Stdout,
    buffer: &TerminalBuffer,
    row_offset: usize,
    col_offset: usize,
    view_rows: usize,
    view_cols: usize,
    highlight_line: Option<usize>,
) -> Result<()> {
    // Build output string to minimize syscalls
    let mut output = String::with_capacity(view_rows * view_cols * 2);

    for view_row in 0..view_rows {
        let buf_row = view_row + row_offset;
        let is_highlighted = highlight_line == Some(buf_row);

        // Move cursor to start of line (no clear - we'll overwrite)
        output.push_str(&format!("\x1b[{};1H", view_row + 1));

        // Set highlight style if needed
        if is_highlighted {
            output.push_str("\x1b[97;42m"); // White text on green background
        }

        let mut chars_written = 0;

        if let Some(row) = buffer.row(buf_row) {
            let mut current_style = CellStyle::default();
            let mut in_highlight_style = is_highlighted;

            for view_col in 0..view_cols {
                let buf_col = view_col + col_offset;

                if buf_col < row.len() {
                    let cell = &row[buf_col];

                    if !is_highlighted && cell.style != current_style {
                        // Apply style using ANSI codes directly
                        output.push_str("\x1b[0m"); // Reset
                        style_to_ansi_fg(&cell.style, &mut output);
                        style_to_ansi_bg(&cell.style, &mut output);
                        current_style = cell.style;
                        in_highlight_style = false;
                    } else if is_highlighted && !in_highlight_style {
                        output.push_str("\x1b[97;42m");
                        in_highlight_style = true;
                    }

                    output.push(cell.char);
                    chars_written += 1;
                } else {
                    // Past end of row content - fill with spaces
                    if !is_highlighted && current_style != CellStyle::default() {
                        output.push_str("\x1b[0m");
                        current_style = CellStyle::default();
                    }
                    output.push(' ');
                    chars_written += 1;
                }
            }

            // Reset at end of line
            if current_style != CellStyle::default() || is_highlighted {
                output.push_str("\x1b[0m");
            }
        } else {
            // Empty row - fill with spaces
            if is_highlighted {
                for _ in 0..view_cols {
                    output.push(' ');
                }
                output.push_str("\x1b[0m");
            } else {
                for _ in 0..view_cols {
                    output.push(' ');
                }
            }
            chars_written = view_cols;
        }

        // Ensure we've written the full width (clear any trailing content)
        let _ = chars_written; // Already writing full width above
    }

    write!(stdout, "{}", output)?;
    Ok(())
}

/// Render a single line of the viewport (for partial updates in free mode).
#[allow(clippy::too_many_arguments)]
fn render_single_line(
    stdout: &mut io::Stdout,
    buffer: &TerminalBuffer,
    buf_row: usize,
    view_row_offset: usize,
    col_offset: usize,
    view_cols: usize,
    is_highlighted: bool,
) -> Result<()> {
    // Calculate screen row from buffer row
    if buf_row < view_row_offset {
        return Ok(()); // Line is above viewport
    }
    let screen_row = buf_row - view_row_offset;

    let mut output = String::with_capacity(view_cols * 2);

    // Move cursor to start of line
    output.push_str(&format!("\x1b[{};1H", screen_row + 1));

    if is_highlighted {
        output.push_str("\x1b[97;42m"); // White on green
    }

    if let Some(row) = buffer.row(buf_row) {
        let mut current_style = CellStyle::default();

        for view_col in 0..view_cols {
            let buf_col = view_col + col_offset;

            if buf_col < row.len() {
                let cell = &row[buf_col];

                if !is_highlighted && cell.style != current_style {
                    output.push_str("\x1b[0m");
                    style_to_ansi_fg(&cell.style, &mut output);
                    style_to_ansi_bg(&cell.style, &mut output);
                    current_style = cell.style;
                }

                output.push(cell.char);
            } else {
                if !is_highlighted && current_style != CellStyle::default() {
                    output.push_str("\x1b[0m");
                    current_style = CellStyle::default();
                }
                output.push(' ');
            }
        }

        if current_style != CellStyle::default() || is_highlighted {
            output.push_str("\x1b[0m");
        }
    } else {
        // Empty row
        for _ in 0..view_cols {
            output.push(' ');
        }
        if is_highlighted {
            output.push_str("\x1b[0m");
        }
    }

    write!(stdout, "{}", output)?;
    Ok(())
}

/// Convert cell style foreground to ANSI escape code.
/// Returns static string for basic colors to avoid allocation.
fn style_to_ansi_fg(style: &CellStyle, buf: &mut String) -> bool {
    match &style.fg {
        TermColor::Default => false,
        TermColor::Black => {
            buf.push_str("\x1b[30m");
            true
        }
        TermColor::Red => {
            buf.push_str("\x1b[31m");
            true
        }
        TermColor::Green => {
            buf.push_str("\x1b[32m");
            true
        }
        TermColor::Yellow => {
            buf.push_str("\x1b[33m");
            true
        }
        TermColor::Blue => {
            buf.push_str("\x1b[34m");
            true
        }
        TermColor::Magenta => {
            buf.push_str("\x1b[35m");
            true
        }
        TermColor::Cyan => {
            buf.push_str("\x1b[36m");
            true
        }
        TermColor::White => {
            buf.push_str("\x1b[37m");
            true
        }
        TermColor::BrightBlack => {
            buf.push_str("\x1b[90m");
            true
        }
        TermColor::BrightRed => {
            buf.push_str("\x1b[91m");
            true
        }
        TermColor::BrightGreen => {
            buf.push_str("\x1b[92m");
            true
        }
        TermColor::BrightYellow => {
            buf.push_str("\x1b[93m");
            true
        }
        TermColor::BrightBlue => {
            buf.push_str("\x1b[94m");
            true
        }
        TermColor::BrightMagenta => {
            buf.push_str("\x1b[95m");
            true
        }
        TermColor::BrightCyan => {
            buf.push_str("\x1b[96m");
            true
        }
        TermColor::BrightWhite => {
            buf.push_str("\x1b[97m");
            true
        }
        TermColor::Indexed(n) => {
            buf.push_str("\x1b[38;5;");
            buf.push_str(&n.to_string());
            buf.push('m');
            true
        }
        TermColor::Rgb(r, g, b) => {
            buf.push_str("\x1b[38;2;");
            buf.push_str(&r.to_string());
            buf.push(';');
            buf.push_str(&g.to_string());
            buf.push(';');
            buf.push_str(&b.to_string());
            buf.push('m');
            true
        }
    }
}

/// Convert cell style background to ANSI escape code.
/// Returns static string for basic colors to avoid allocation.
fn style_to_ansi_bg(style: &CellStyle, buf: &mut String) -> bool {
    match &style.bg {
        TermColor::Default => false,
        TermColor::Black => {
            buf.push_str("\x1b[40m");
            true
        }
        TermColor::Red => {
            buf.push_str("\x1b[41m");
            true
        }
        TermColor::Green => {
            buf.push_str("\x1b[42m");
            true
        }
        TermColor::Yellow => {
            buf.push_str("\x1b[43m");
            true
        }
        TermColor::Blue => {
            buf.push_str("\x1b[44m");
            true
        }
        TermColor::Magenta => {
            buf.push_str("\x1b[45m");
            true
        }
        TermColor::Cyan => {
            buf.push_str("\x1b[46m");
            true
        }
        TermColor::White => {
            buf.push_str("\x1b[47m");
            true
        }
        TermColor::BrightBlack => {
            buf.push_str("\x1b[100m");
            true
        }
        TermColor::BrightRed => {
            buf.push_str("\x1b[101m");
            true
        }
        TermColor::BrightGreen => {
            buf.push_str("\x1b[102m");
            true
        }
        TermColor::BrightYellow => {
            buf.push_str("\x1b[103m");
            true
        }
        TermColor::BrightBlue => {
            buf.push_str("\x1b[104m");
            true
        }
        TermColor::BrightMagenta => {
            buf.push_str("\x1b[105m");
            true
        }
        TermColor::BrightCyan => {
            buf.push_str("\x1b[106m");
            true
        }
        TermColor::BrightWhite => {
            buf.push_str("\x1b[107m");
            true
        }
        TermColor::Indexed(n) => {
            buf.push_str("\x1b[48;5;");
            buf.push_str(&n.to_string());
            buf.push('m');
            true
        }
        TermColor::Rgb(r, g, b) => {
            buf.push_str("\x1b[48;2;");
            buf.push_str(&r.to_string());
            buf.push(';');
            buf.push_str(&g.to_string());
            buf.push(';');
            buf.push_str(&b.to_string());
            buf.push('m');
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_result_success_message() {
        let result = PlaybackResult::Success("test.cast".to_string());
        assert_eq!(result.message(), "Played: test.cast");
    }

    #[test]
    fn playback_result_interrupted_message() {
        let result = PlaybackResult::Interrupted;
        assert_eq!(result.message(), "Playback interrupted");
    }

    #[test]
    fn playback_result_error_message() {
        let result = PlaybackResult::Error("not found".to_string());
        assert_eq!(result.message(), "Failed to play: not found");
    }

    #[test]
    fn format_duration_formats_correctly() {
        assert_eq!(format_duration(0.0), "00:00");
        assert_eq!(format_duration(65.0), "01:05");
        assert_eq!(format_duration(3661.0), "61:01");
    }

    #[test]
    fn style_to_ansi_fg_default_returns_false() {
        let style = CellStyle::default();
        let mut buf = String::new();
        assert!(!style_to_ansi_fg(&style, &mut buf));
        assert!(buf.is_empty());
    }

    #[test]
    fn style_to_ansi_fg_red_appends_code() {
        let style = CellStyle {
            fg: TermColor::Red,
            ..Default::default()
        };
        let mut buf = String::new();
        assert!(style_to_ansi_fg(&style, &mut buf));
        assert_eq!(buf, "\x1b[31m");
    }

    #[test]
    fn count_digits_works() {
        assert_eq!(count_digits(0), 1);
        assert_eq!(count_digits(1), 1);
        assert_eq!(count_digits(9), 1);
        assert_eq!(count_digits(10), 2);
        assert_eq!(count_digits(99), 2);
        assert_eq!(count_digits(100), 3);
    }
}
