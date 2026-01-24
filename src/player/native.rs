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
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseButton,
        MouseEventKind,
    },
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

    // Viewport offset (for scrolling) - start at top since buffer is empty initially
    let mut view_row_offset: usize = 0;
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
    execute!(stdout, EnterAlternateScreen, Hide, EnableMouseCapture)?;

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
                    } else if let Some((cols, rows)) = event.parse_resize() {
                        buffer.resize(cols as usize, rows as usize);
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
                                // Resize terminal to match recording size.
                                // NOTE: This uses xterm escape sequence which only works on
                                // xterm-compatible terminals (iTerm2, xterm, etc.). Other terminals
                                // may ignore this request silently.
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
                                    // Check if resize succeeded (terminal at least as big as recording)
                                    let resize_ok = new_cols as u32 >= rec_cols
                                        && new_rows >= status_lines + rec_rows as u16;
                                    if resize_ok {
                                        // Reset viewport offset since we now fit
                                        if view_rows >= rec_rows as usize {
                                            view_row_offset = 0;
                                        }
                                        if view_cols >= rec_cols as usize {
                                            view_col_offset = 0;
                                        }
                                    }
                                    // Note: If resize failed, viewport mode still works for navigation
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
                    Event::Mouse(mouse) => {
                        // Handle mouse click on progress bar to seek
                        if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                            let progress_row = term_rows - 2;
                            if mouse.row == progress_row {
                                // Calculate time from x position on progress bar
                                // Bar starts at column 1, width is term_cols - 14
                                let bar_start = 1u16;
                                let bar_width = (term_cols as usize).saturating_sub(14);
                                if mouse.column >= bar_start
                                    && mouse.column < bar_start + bar_width as u16
                                {
                                    let click_pos = (mouse.column - bar_start) as f64;
                                    let ratio = click_pos / bar_width as f64;
                                    let new_time =
                                        (ratio * total_duration).clamp(0.0, total_duration);

                                    // Exit free mode if active
                                    free_mode = false;

                                    // Seek to clicked position
                                    seek_to_time(&mut buffer, &cast, new_time, rec_cols, rec_rows);
                                    current_time = new_time;
                                    time_offset = current_time;
                                    start_time = Instant::now();
                                    (event_idx, cumulative_time) =
                                        find_event_index_at_time(&cast, current_time);

                                    // Resume playback after seeking
                                    paused = false;
                                    needs_render = true;
                                }
                            }
                        }
                    }
                    _ => {} // Ignore other events (focus, etc.)
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
                    } else if let Some((cols, rows)) = evt.parse_resize() {
                        buffer.resize(cols as usize, rows as usize);
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
    execute!(stdout, Show, DisableMouseCapture, LeaveAlternateScreen)?;
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
        } else if let Some((new_cols, new_rows)) = event.parse_resize() {
            buffer.resize(new_cols as usize, new_rows as usize);
        }
    }
}

/// Build the progress bar character array.
/// Returns (bar_chars, filled_count) where bar_chars contains the visual representation.
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
    let (bar, filled) = build_progress_bar_chars(bar_width, current_time, total_duration, markers);

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

/// Calculate which scroll directions are available.
/// Returns (can_up, can_down, can_left, can_right).
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
/// Returns None if no scrolling is possible.
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
    let (can_up, can_down, can_left, can_right) = calc_scroll_directions(
        row_offset, col_offset, view_rows, view_cols, rec_rows, rec_cols,
    );

    let arrow_str = match build_scroll_arrows(can_up, can_down, can_left, can_right) {
        Some(s) => s,
        None => return Ok(()),
    };

    let arrows_count = [can_up, can_down, can_left, can_right]
        .iter()
        .filter(|&&x| x)
        .count();

    // Draw at top-right, completely aligned to edge
    let arrow_color = Color::Yellow;
    let bg_color = Color::AnsiValue(236); // Same as progress bar
                                          // Width = arrows + spaces between + padding on sides
    let display_width = (arrows_count * 2 + 1) as u16; // each arrow + space, plus padding
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

/// Help text lines for the help overlay.
const HELP_LINES: &[&str] = &[
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

/// Width of the help box (for centering calculations).
const HELP_BOX_WIDTH: usize = 47;

/// Calculate the starting row for centering the help box.
fn calc_help_start_row(term_height: u16) -> u16 {
    let box_height = HELP_LINES.len() as u16;
    (term_height.saturating_sub(box_height)) / 2
}

/// Calculate the starting column for centering the help box.
fn calc_help_start_col(term_width: u16) -> u16 {
    ((term_width as usize).saturating_sub(HELP_BOX_WIDTH) / 2) as u16
}

/// Render the help overlay.
fn render_help(stdout: &mut io::Stdout, width: u16, height: u16) -> Result<()> {
    let start_row = calc_help_start_row(height);
    let col = calc_help_start_col(width);

    execute!(stdout, Clear(ClearType::All))?;

    for (i, line) in HELP_LINES.iter().enumerate() {
        let row = start_row + i as u16;
        execute!(
            stdout,
            MoveTo(col, row),
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
                        style_to_ansi_attrs(&cell.style, &mut output);
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
                    style_to_ansi_attrs(&cell.style, &mut output);
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

/// Append ANSI codes for text attributes (bold, dim, italic, underline, reverse) to buffer
fn style_to_ansi_attrs(style: &CellStyle, buf: &mut String) {
    if style.bold {
        buf.push_str("\x1b[1m");
    }
    if style.dim {
        buf.push_str("\x1b[2m");
    }
    if style.italic {
        buf.push_str("\x1b[3m");
    }
    if style.underline {
        buf.push_str("\x1b[4m");
    }
    if style.reverse {
        buf.push_str("\x1b[7m");
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

    #[test]
    fn count_digits_large_numbers() {
        assert_eq!(count_digits(999), 3);
        assert_eq!(count_digits(1000), 4);
        assert_eq!(count_digits(9999), 4);
        assert_eq!(count_digits(10000), 5);
        assert_eq!(count_digits(1_000_000), 7);
    }

    #[test]
    fn format_duration_edge_cases() {
        // Fractional seconds are truncated
        assert_eq!(format_duration(0.9), "00:00");
        assert_eq!(format_duration(1.5), "00:01");
        assert_eq!(format_duration(59.9), "00:59");
        // Very large durations (hours)
        assert_eq!(format_duration(7200.0), "120:00"); // 2 hours
    }

    #[test]
    fn style_to_ansi_fg_all_basic_colors() {
        let test_cases = [
            (TermColor::Black, "\x1b[30m"),
            (TermColor::Red, "\x1b[31m"),
            (TermColor::Green, "\x1b[32m"),
            (TermColor::Yellow, "\x1b[33m"),
            (TermColor::Blue, "\x1b[34m"),
            (TermColor::Magenta, "\x1b[35m"),
            (TermColor::Cyan, "\x1b[36m"),
            (TermColor::White, "\x1b[37m"),
        ];

        for (color, expected) in test_cases {
            let style = CellStyle {
                fg: color,
                ..Default::default()
            };
            let mut buf = String::new();
            assert!(style_to_ansi_fg(&style, &mut buf));
            assert_eq!(buf, expected, "Failed for {:?}", color);
        }
    }

    #[test]
    fn style_to_ansi_fg_all_bright_colors() {
        let test_cases = [
            (TermColor::BrightBlack, "\x1b[90m"),
            (TermColor::BrightRed, "\x1b[91m"),
            (TermColor::BrightGreen, "\x1b[92m"),
            (TermColor::BrightYellow, "\x1b[93m"),
            (TermColor::BrightBlue, "\x1b[94m"),
            (TermColor::BrightMagenta, "\x1b[95m"),
            (TermColor::BrightCyan, "\x1b[96m"),
            (TermColor::BrightWhite, "\x1b[97m"),
        ];

        for (color, expected) in test_cases {
            let style = CellStyle {
                fg: color,
                ..Default::default()
            };
            let mut buf = String::new();
            assert!(style_to_ansi_fg(&style, &mut buf));
            assert_eq!(buf, expected, "Failed for {:?}", color);
        }
    }

    #[test]
    fn style_to_ansi_fg_indexed_color() {
        let style = CellStyle {
            fg: TermColor::Indexed(196),
            ..Default::default()
        };
        let mut buf = String::new();
        assert!(style_to_ansi_fg(&style, &mut buf));
        assert_eq!(buf, "\x1b[38;5;196m");
    }

    #[test]
    fn style_to_ansi_fg_rgb_color() {
        let style = CellStyle {
            fg: TermColor::Rgb(255, 128, 64),
            ..Default::default()
        };
        let mut buf = String::new();
        assert!(style_to_ansi_fg(&style, &mut buf));
        assert_eq!(buf, "\x1b[38;2;255;128;64m");
    }

    #[test]
    fn style_to_ansi_bg_default_returns_false() {
        let style = CellStyle::default();
        let mut buf = String::new();
        assert!(!style_to_ansi_bg(&style, &mut buf));
        assert!(buf.is_empty());
    }

    #[test]
    fn style_to_ansi_bg_all_basic_colors() {
        let test_cases = [
            (TermColor::Black, "\x1b[40m"),
            (TermColor::Red, "\x1b[41m"),
            (TermColor::Green, "\x1b[42m"),
            (TermColor::Yellow, "\x1b[43m"),
            (TermColor::Blue, "\x1b[44m"),
            (TermColor::Magenta, "\x1b[45m"),
            (TermColor::Cyan, "\x1b[46m"),
            (TermColor::White, "\x1b[47m"),
        ];

        for (color, expected) in test_cases {
            let style = CellStyle {
                bg: color,
                ..Default::default()
            };
            let mut buf = String::new();
            assert!(style_to_ansi_bg(&style, &mut buf));
            assert_eq!(buf, expected, "Failed for {:?}", color);
        }
    }

    #[test]
    fn style_to_ansi_bg_all_bright_colors() {
        let test_cases = [
            (TermColor::BrightBlack, "\x1b[100m"),
            (TermColor::BrightRed, "\x1b[101m"),
            (TermColor::BrightGreen, "\x1b[102m"),
            (TermColor::BrightYellow, "\x1b[103m"),
            (TermColor::BrightBlue, "\x1b[104m"),
            (TermColor::BrightMagenta, "\x1b[105m"),
            (TermColor::BrightCyan, "\x1b[106m"),
            (TermColor::BrightWhite, "\x1b[107m"),
        ];

        for (color, expected) in test_cases {
            let style = CellStyle {
                bg: color,
                ..Default::default()
            };
            let mut buf = String::new();
            assert!(style_to_ansi_bg(&style, &mut buf));
            assert_eq!(buf, expected, "Failed for {:?}", color);
        }
    }

    #[test]
    fn style_to_ansi_bg_indexed_color() {
        let style = CellStyle {
            bg: TermColor::Indexed(236),
            ..Default::default()
        };
        let mut buf = String::new();
        assert!(style_to_ansi_bg(&style, &mut buf));
        assert_eq!(buf, "\x1b[48;5;236m");
    }

    #[test]
    fn style_to_ansi_bg_rgb_color() {
        let style = CellStyle {
            bg: TermColor::Rgb(0, 128, 255),
            ..Default::default()
        };
        let mut buf = String::new();
        assert!(style_to_ansi_bg(&style, &mut buf));
        assert_eq!(buf, "\x1b[48;2;0;128;255m");
    }

    // Tests for find_event_index_at_time
    mod find_event_index_tests {
        use super::*;
        use crate::asciicast::{Event, EventType, Header};

        fn make_cast(event_times: &[f64]) -> AsciicastFile {
            let header = Header {
                version: 3,
                width: Some(80),
                height: Some(24),
                term: None,
                timestamp: None,
                duration: None,
                title: None,
                command: None,
                env: None,
                idle_time_limit: None,
            };
            let events: Vec<Event> = event_times
                .iter()
                .map(|&t| Event {
                    time: t,
                    event_type: EventType::Output,
                    data: "x".to_string(),
                })
                .collect();
            AsciicastFile { header, events }
        }

        #[test]
        fn empty_cast_returns_zero_index() {
            let cast = make_cast(&[]);
            let (idx, cumulative) = find_event_index_at_time(&cast, 5.0);
            assert_eq!(idx, 0);
            assert_eq!(cumulative, 0.0);
        }

        #[test]
        fn target_before_first_event() {
            let cast = make_cast(&[1.0, 1.0, 1.0]); // Events at t=1, t=2, t=3
            let (idx, cumulative) = find_event_index_at_time(&cast, 0.5);
            assert_eq!(idx, 0);
            assert_eq!(cumulative, 0.0);
        }

        #[test]
        fn target_at_first_event() {
            let cast = make_cast(&[1.0, 1.0, 1.0]);
            let (idx, cumulative) = find_event_index_at_time(&cast, 1.0);
            assert_eq!(idx, 1); // After first event
            assert_eq!(cumulative, 1.0);
        }

        #[test]
        fn target_between_events() {
            let cast = make_cast(&[1.0, 1.0, 1.0]); // Events at t=1, t=2, t=3
            let (idx, cumulative) = find_event_index_at_time(&cast, 1.5);
            assert_eq!(idx, 1); // At event index 1 (second event)
            assert_eq!(cumulative, 1.0);
        }

        #[test]
        fn target_at_last_event() {
            let cast = make_cast(&[1.0, 1.0, 1.0]); // Events at t=1, t=2, t=3
            let (idx, cumulative) = find_event_index_at_time(&cast, 3.0);
            assert_eq!(idx, 3); // Past all events
            assert_eq!(cumulative, 3.0);
        }

        #[test]
        fn target_past_all_events() {
            let cast = make_cast(&[1.0, 1.0, 1.0]); // Events at t=1, t=2, t=3
            let (idx, cumulative) = find_event_index_at_time(&cast, 10.0);
            assert_eq!(idx, 3); // All events processed
            assert_eq!(cumulative, 3.0);
        }
    }

    // Tests for collect_markers
    mod collect_markers_tests {
        use super::*;
        use crate::asciicast::{Event, EventType, Header};

        fn make_header() -> Header {
            Header {
                version: 3,
                width: Some(80),
                height: Some(24),
                term: None,
                timestamp: None,
                duration: None,
                title: None,
                command: None,
                env: None,
                idle_time_limit: None,
            }
        }

        #[test]
        fn empty_cast_returns_no_markers() {
            let cast = AsciicastFile {
                header: make_header(),
                events: vec![],
            };
            let markers = collect_markers(&cast);
            assert!(markers.is_empty());
        }

        #[test]
        fn cast_with_only_output_returns_no_markers() {
            let cast = AsciicastFile {
                header: make_header(),
                events: vec![
                    Event {
                        time: 1.0,
                        event_type: EventType::Output,
                        data: "hello".to_string(),
                    },
                    Event {
                        time: 1.0,
                        event_type: EventType::Output,
                        data: "world".to_string(),
                    },
                ],
            };
            let markers = collect_markers(&cast);
            assert!(markers.is_empty());
        }

        #[test]
        fn cast_with_markers_collects_them() {
            let cast = AsciicastFile {
                header: make_header(),
                events: vec![
                    Event {
                        time: 1.0,
                        event_type: EventType::Output,
                        data: "hello".to_string(),
                    },
                    Event {
                        time: 1.0,
                        event_type: EventType::Marker,
                        data: "marker1".to_string(),
                    },
                    Event {
                        time: 2.0,
                        event_type: EventType::Output,
                        data: "world".to_string(),
                    },
                    Event {
                        time: 1.0,
                        event_type: EventType::Marker,
                        data: "marker2".to_string(),
                    },
                ],
            };
            let markers = collect_markers(&cast);
            assert_eq!(markers.len(), 2);
            assert_eq!(markers[0].time, 2.0); // 1.0 + 1.0
            assert_eq!(markers[0].label, "marker1");
            assert_eq!(markers[1].time, 5.0); // 1.0 + 1.0 + 2.0 + 1.0
            assert_eq!(markers[1].label, "marker2");
        }

        #[test]
        fn marker_at_start() {
            let cast = AsciicastFile {
                header: make_header(),
                events: vec![
                    Event {
                        time: 0.0,
                        event_type: EventType::Marker,
                        data: "start".to_string(),
                    },
                    Event {
                        time: 1.0,
                        event_type: EventType::Output,
                        data: "output".to_string(),
                    },
                ],
            };
            let markers = collect_markers(&cast);
            assert_eq!(markers.len(), 1);
            assert_eq!(markers[0].time, 0.0);
            assert_eq!(markers[0].label, "start");
        }
    }

    // Tests for seek_to_time
    mod seek_to_time_tests {
        use super::*;
        use crate::asciicast::{Event, EventType, Header};

        fn make_header() -> Header {
            Header {
                version: 3,
                width: Some(80),
                height: Some(24),
                term: None,
                timestamp: None,
                duration: None,
                title: None,
                command: None,
                env: None,
                idle_time_limit: None,
            }
        }

        #[test]
        fn seek_to_zero_clears_buffer() {
            let cast = AsciicastFile {
                header: make_header(),
                events: vec![Event {
                    time: 1.0,
                    event_type: EventType::Output,
                    data: "hello".to_string(),
                }],
            };
            let mut buffer = TerminalBuffer::new(80, 24);
            buffer.process("some content");

            seek_to_time(&mut buffer, &cast, 0.0, 80, 24);

            // Buffer should be cleared (no content at 0.0)
            let row = buffer.row(0).unwrap();
            assert!(row.iter().all(|c| c.char == ' '));
        }

        #[test]
        fn seek_to_after_event_includes_output() {
            let cast = AsciicastFile {
                header: make_header(),
                events: vec![Event {
                    time: 1.0,
                    event_type: EventType::Output,
                    data: "hello".to_string(),
                }],
            };
            let mut buffer = TerminalBuffer::new(80, 24);

            seek_to_time(&mut buffer, &cast, 2.0, 80, 24);

            // Buffer should contain "hello"
            let row = buffer.row(0).unwrap();
            let content: String = row.iter().take(5).map(|c| c.char).collect();
            assert_eq!(content, "hello");
        }

        #[test]
        fn seek_skips_markers() {
            let cast = AsciicastFile {
                header: make_header(),
                events: vec![
                    Event {
                        time: 1.0,
                        event_type: EventType::Marker,
                        data: "marker".to_string(),
                    },
                    Event {
                        time: 1.0,
                        event_type: EventType::Output,
                        data: "text".to_string(),
                    },
                ],
            };
            let mut buffer = TerminalBuffer::new(80, 24);

            seek_to_time(&mut buffer, &cast, 3.0, 80, 24);

            // Buffer should contain "text" (marker data not rendered)
            let row = buffer.row(0).unwrap();
            let content: String = row.iter().take(4).map(|c| c.char).collect();
            assert_eq!(content, "text");
        }
    }

    // Tests for PlaybackResult
    #[test]
    fn playback_result_clone() {
        let result = PlaybackResult::Success("test.cast".to_string());
        let cloned = result.clone();
        assert_eq!(result.message(), cloned.message());
    }

    #[test]
    fn playback_result_debug() {
        let result = PlaybackResult::Interrupted;
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("Interrupted"));
    }

    // Tests for build_progress_bar_chars
    mod progress_bar_tests {
        use super::*;

        #[test]
        fn empty_bar_at_zero() {
            let (bar, filled) = build_progress_bar_chars(10, 0.0, 10.0, &[]);
            assert_eq!(filled, 0);
            assert_eq!(bar[0], '⏺'); // Playhead at start
            assert_eq!(bar[1], '─');
        }

        #[test]
        fn full_bar_at_end() {
            let (bar, filled) = build_progress_bar_chars(10, 10.0, 10.0, &[]);
            assert_eq!(filled, 10);
            // All positions should be regular bar chars (no playhead since filled == bar_width)
            assert!(bar.iter().all(|&c| c == '─'));
        }

        #[test]
        fn half_progress() {
            let (bar, filled) = build_progress_bar_chars(10, 5.0, 10.0, &[]);
            assert_eq!(filled, 5);
            assert_eq!(bar[5], '⏺'); // Playhead at middle
        }

        #[test]
        fn marker_at_position() {
            let markers = vec![MarkerPosition {
                time: 5.0,
                label: "test".to_string(),
            }];
            let (bar, _) = build_progress_bar_chars(10, 0.0, 10.0, &markers);
            assert_eq!(bar[5], '◆'); // Marker at position 5
        }

        #[test]
        fn marker_not_overwritten_by_playhead() {
            // Marker at same position as playhead - playhead wins
            let markers = vec![MarkerPosition {
                time: 5.0,
                label: "test".to_string(),
            }];
            let (bar, _) = build_progress_bar_chars(10, 5.0, 10.0, &markers);
            assert_eq!(bar[5], '⏺'); // Playhead takes precedence
        }

        #[test]
        fn multiple_markers() {
            let markers = vec![
                MarkerPosition {
                    time: 2.0,
                    label: "m1".to_string(),
                },
                MarkerPosition {
                    time: 8.0,
                    label: "m2".to_string(),
                },
            ];
            let (bar, _) = build_progress_bar_chars(10, 0.0, 10.0, &markers);
            assert_eq!(bar[2], '◆');
            assert_eq!(bar[8], '◆');
        }

        #[test]
        fn zero_duration_returns_full() {
            let (_, filled) = build_progress_bar_chars(10, 5.0, 0.0, &[]);
            assert_eq!(filled, 10); // progress = 1.0 when duration is 0
        }

        #[test]
        fn progress_clamped_to_one() {
            // Current time exceeds total duration
            let (_, filled) = build_progress_bar_chars(10, 15.0, 10.0, &[]);
            assert_eq!(filled, 10); // Clamped to 100%
        }

        #[test]
        fn marker_at_zero_duration() {
            let markers = vec![MarkerPosition {
                time: 5.0,
                label: "m".to_string(),
            }];
            let (bar, _) = build_progress_bar_chars(10, 0.0, 0.0, &markers);
            // When duration is 0, marker_pos = 0
            assert_eq!(bar[0], '◆');
        }
    }

    // Tests for calc_scroll_directions
    mod scroll_direction_tests {
        use super::*;

        #[test]
        fn no_scroll_when_viewport_fits() {
            let (up, down, left, right) = calc_scroll_directions(0, 0, 24, 80, 24, 80);
            assert!(!up);
            assert!(!down);
            assert!(!left);
            assert!(!right);
        }

        #[test]
        fn can_scroll_down_when_content_below() {
            let (up, down, left, right) = calc_scroll_directions(0, 0, 24, 80, 48, 80);
            assert!(!up);
            assert!(down);
            assert!(!left);
            assert!(!right);
        }

        #[test]
        fn can_scroll_up_when_offset_positive() {
            let (up, down, left, right) = calc_scroll_directions(10, 0, 24, 80, 48, 80);
            assert!(up);
            assert!(down); // Still more content below
            assert!(!left);
            assert!(!right);
        }

        #[test]
        fn can_scroll_right_when_content_wider() {
            let (up, down, left, right) = calc_scroll_directions(0, 0, 24, 80, 24, 120);
            assert!(!up);
            assert!(!down);
            assert!(!left);
            assert!(right);
        }

        #[test]
        fn can_scroll_left_when_col_offset() {
            let (up, down, left, right) = calc_scroll_directions(0, 20, 24, 80, 24, 120);
            assert!(!up);
            assert!(!down);
            assert!(left);
            assert!(right);
        }

        #[test]
        fn all_directions_when_in_middle() {
            // Viewport in middle of larger content
            let (up, down, left, right) = calc_scroll_directions(10, 10, 24, 80, 48, 160);
            assert!(up);
            assert!(down);
            assert!(left);
            assert!(right);
        }

        #[test]
        fn at_bottom_right_corner() {
            // At bottom-right, can only scroll up and left
            let (up, down, left, right) = calc_scroll_directions(24, 40, 24, 80, 48, 120);
            assert!(up);
            assert!(!down); // At bottom
            assert!(left);
            assert!(!right); // At right edge
        }
    }

    // Tests for build_scroll_arrows
    mod scroll_arrows_tests {
        use super::*;

        #[test]
        fn no_arrows_when_no_scroll() {
            let result = build_scroll_arrows(false, false, false, false);
            assert!(result.is_none());
        }

        #[test]
        fn up_arrow_only() {
            let result = build_scroll_arrows(true, false, false, false);
            assert_eq!(result, Some("▲".to_string()));
        }

        #[test]
        fn down_arrow_only() {
            let result = build_scroll_arrows(false, true, false, false);
            assert_eq!(result, Some("▼".to_string()));
        }

        #[test]
        fn left_arrow_only() {
            let result = build_scroll_arrows(false, false, true, false);
            assert_eq!(result, Some("◀".to_string()));
        }

        #[test]
        fn right_arrow_only() {
            let result = build_scroll_arrows(false, false, false, true);
            assert_eq!(result, Some("▶".to_string()));
        }

        #[test]
        fn up_and_down_arrows() {
            let result = build_scroll_arrows(true, true, false, false);
            assert_eq!(result, Some("▲ ▼".to_string()));
        }

        #[test]
        fn all_arrows() {
            let result = build_scroll_arrows(true, true, true, true);
            assert_eq!(result, Some("▲ ▼ ◀ ▶".to_string()));
        }

        #[test]
        fn horizontal_arrows_only() {
            let result = build_scroll_arrows(false, false, true, true);
            assert_eq!(result, Some("◀ ▶".to_string()));
        }
    }

    // Tests for help box calculations
    mod help_box_tests {
        use super::*;

        #[test]
        fn help_lines_not_empty() {
            assert!(!HELP_LINES.is_empty());
        }

        #[test]
        fn help_lines_has_title() {
            let has_title = HELP_LINES
                .iter()
                .any(|line| line.contains("AGR Native Player Help"));
            assert!(has_title);
        }

        #[test]
        fn help_lines_has_quit_instruction() {
            let has_quit = HELP_LINES
                .iter()
                .any(|line| line.contains("q") && line.contains("Quit"));
            assert!(has_quit);
        }

        #[test]
        fn help_lines_has_close_instruction() {
            let has_close = HELP_LINES
                .iter()
                .any(|line| line.contains("Press any key to close"));
            assert!(has_close);
        }

        #[test]
        fn help_box_width_is_correct() {
            // The widest line in the help box should match HELP_BOX_WIDTH
            assert_eq!(HELP_BOX_WIDTH, 47);
        }

        #[test]
        fn calc_help_start_row_centers_vertically() {
            // With height 100 and box of 34 lines, should center
            let start = calc_help_start_row(100);
            let box_height = HELP_LINES.len() as u16;
            assert_eq!(start, (100 - box_height) / 2);
        }

        #[test]
        fn calc_help_start_row_handles_small_terminal() {
            // When terminal is smaller than help box
            let start = calc_help_start_row(10);
            assert_eq!(start, 0); // saturating_sub prevents underflow
        }

        #[test]
        fn calc_help_start_col_centers_horizontally() {
            let col = calc_help_start_col(120);
            // (120 - 47) / 2 = 36
            assert_eq!(col, 36);
        }

        #[test]
        fn calc_help_start_col_handles_narrow_terminal() {
            let col = calc_help_start_col(40);
            // (40 - 47) saturating_sub = 0, / 2 = 0
            assert_eq!(col, 0);
        }
    }

    // Additional edge case tests
    #[test]
    fn format_duration_negative_treated_as_zero() {
        // Negative durations should still format (as 0 due to u64 cast)
        assert_eq!(format_duration(-5.0), "00:00");
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

    // Behavior tests - test realistic playback scenarios
    mod playback_behavior_tests {
        use super::*;
        use crate::asciicast::{Event, EventType, Header};

        fn make_header(cols: u32, rows: u32) -> Header {
            Header {
                version: 3,
                width: Some(cols),
                height: Some(rows),
                term: None,
                timestamp: None,
                duration: None,
                title: None,
                command: None,
                env: None,
                idle_time_limit: None,
            }
        }

        /// Helper to create a cast with shell-like output
        fn make_shell_cast() -> AsciicastFile {
            AsciicastFile {
                header: make_header(80, 24),
                events: vec![
                    // Prompt appears
                    Event {
                        time: 0.5,
                        event_type: EventType::Output,
                        data: "$ ".to_string(),
                    },
                    // User types command
                    Event {
                        time: 1.0,
                        event_type: EventType::Output,
                        data: "echo hello\r\n".to_string(),
                    },
                    // Marker for command execution
                    Event {
                        time: 0.1,
                        event_type: EventType::Marker,
                        data: "Command executed".to_string(),
                    },
                    // Command output
                    Event {
                        time: 0.5,
                        event_type: EventType::Output,
                        data: "hello\r\n".to_string(),
                    },
                    // Next prompt
                    Event {
                        time: 0.2,
                        event_type: EventType::Output,
                        data: "$ ".to_string(),
                    },
                ],
            }
        }

        #[test]
        fn seeking_to_before_command_shows_prompt_only() {
            let cast = make_shell_cast();
            let mut buffer = TerminalBuffer::new(80, 24);

            // Seek to just after the prompt appears (0.5s)
            seek_to_time(&mut buffer, &cast, 0.5, 80, 24);

            let row = buffer.row(0).unwrap();
            let content: String = row.iter().take(2).map(|c| c.char).collect();
            assert_eq!(content, "$ ");
        }

        #[test]
        fn seeking_to_after_command_shows_full_interaction() {
            let cast = make_shell_cast();
            let mut buffer = TerminalBuffer::new(80, 24);

            // Seek to end (all output processed)
            seek_to_time(&mut buffer, &cast, 10.0, 80, 24);

            // First line should have "$ echo hello"
            let row0 = buffer.row(0).unwrap();
            let line0: String = row0.iter().take(12).map(|c| c.char).collect();
            assert_eq!(line0, "$ echo hello");

            // Second line should have "hello"
            let row1 = buffer.row(1).unwrap();
            let line1: String = row1.iter().take(5).map(|c| c.char).collect();
            assert_eq!(line1, "hello");

            // Third line should have new prompt
            let row2 = buffer.row(2).unwrap();
            let line2: String = row2.iter().take(2).map(|c| c.char).collect();
            assert_eq!(line2, "$ ");
        }

        #[test]
        fn markers_collected_at_correct_cumulative_times() {
            let cast = make_shell_cast();
            let markers = collect_markers(&cast);

            // Should have exactly one marker
            assert_eq!(markers.len(), 1);
            // Marker at cumulative time: 0.5 + 1.0 + 0.1 = 1.6
            assert!((markers[0].time - 1.6).abs() < 0.001);
            assert_eq!(markers[0].label, "Command executed");
        }

        #[test]
        fn find_event_finds_correct_position_for_seeking() {
            let cast = make_shell_cast();

            // Find index at 1.0s (should be after first event, before second)
            let (idx, cumulative) = find_event_index_at_time(&cast, 1.0);
            assert_eq!(idx, 1);
            assert!((cumulative - 0.5).abs() < 0.001);

            // Find index at 2.0s (should be after command output)
            let (idx, cumulative) = find_event_index_at_time(&cast, 2.0);
            assert_eq!(idx, 3); // After first 3 events
            assert!((cumulative - 1.6).abs() < 0.001);
        }

        #[test]
        fn progress_bar_shows_marker_at_correct_position() {
            let markers = vec![MarkerPosition {
                time: 1.6,
                label: "Command executed".to_string(),
            }];
            // Total duration is 2.3s
            let total_duration = 2.3;
            let bar_width = 100;

            let (bar, _) = build_progress_bar_chars(bar_width, 0.0, total_duration, &markers);

            // Marker should be at position (1.6 / 2.3) * 100 ≈ 69
            let marker_pos = ((1.6 / total_duration) * bar_width as f64) as usize;
            assert_eq!(bar[marker_pos], '◆');
        }

        #[test]
        fn viewport_scrolling_behavior() {
            // Simulate a recording larger than terminal
            let rec_rows = 48;
            let rec_cols = 120;
            let view_rows = 24;
            let view_cols = 80;

            // At top-left corner
            let (up, down, left, right) =
                calc_scroll_directions(0, 0, view_rows, view_cols, rec_rows, rec_cols);
            assert!(!up, "Should not scroll up at top");
            assert!(down, "Should scroll down");
            assert!(!left, "Should not scroll left at left edge");
            assert!(right, "Should scroll right");

            // After scrolling down and right
            let (up, down, left, right) =
                calc_scroll_directions(12, 20, view_rows, view_cols, rec_rows, rec_cols);
            assert!(up, "Should scroll up after scrolling down");
            assert!(down, "Should still scroll down more");
            assert!(left, "Should scroll left after scrolling right");
            assert!(right, "Should still scroll right more");

            // At bottom-right corner
            let (up, down, left, right) =
                calc_scroll_directions(24, 40, view_rows, view_cols, rec_rows, rec_cols);
            assert!(up, "Should scroll up");
            assert!(!down, "Should not scroll down at bottom");
            assert!(left, "Should scroll left");
            assert!(!right, "Should not scroll right at right edge");
        }

        #[test]
        fn scroll_arrows_reflect_available_directions() {
            // Verify arrows string matches available directions
            let arrows = build_scroll_arrows(true, false, false, false);
            assert_eq!(arrows, Some("▲".to_string()));

            let arrows = build_scroll_arrows(true, true, false, false);
            assert_eq!(arrows, Some("▲ ▼".to_string()));

            let arrows = build_scroll_arrows(true, true, true, true);
            assert_eq!(arrows, Some("▲ ▼ ◀ ▶".to_string()));
        }

        #[test]
        fn seeking_backwards_rebuilds_buffer_correctly() {
            let cast = make_shell_cast();
            let mut buffer = TerminalBuffer::new(80, 24);

            // First seek to end
            seek_to_time(&mut buffer, &cast, 10.0, 80, 24);

            // Verify full output is there
            let row1 = buffer.row(1).unwrap();
            let line1: String = row1.iter().take(5).map(|c| c.char).collect();
            assert_eq!(line1, "hello");

            // Now seek back to start
            seek_to_time(&mut buffer, &cast, 0.0, 80, 24);

            // Buffer should be cleared (no output before 0.5s)
            let row0 = buffer.row(0).unwrap();
            assert!(
                row0.iter().all(|c| c.char == ' '),
                "Buffer should be empty at t=0"
            );

            // Seek to just after prompt
            seek_to_time(&mut buffer, &cast, 0.6, 80, 24);
            let row0 = buffer.row(0).unwrap();
            let line0: String = row0.iter().take(2).map(|c| c.char).collect();
            assert_eq!(line0, "$ ");
        }

        #[test]
        fn ansi_colors_preserved_in_output() {
            let cast = AsciicastFile {
                header: make_header(80, 24),
                events: vec![Event {
                    time: 0.1,
                    event_type: EventType::Output,
                    data: "\x1b[32mgreen\x1b[0m normal".to_string(),
                }],
            };
            let mut buffer = TerminalBuffer::new(80, 24);
            seek_to_time(&mut buffer, &cast, 1.0, 80, 24);

            // Check that "green" has green color
            let row = buffer.row(0).unwrap();
            assert_eq!(row[0].char, 'g');
            assert_eq!(row[0].style.fg, TermColor::Green);

            // Check that "normal" has default color
            assert_eq!(row[6].char, 'n');
            assert_eq!(row[6].style.fg, TermColor::Default);
        }
    }
}
