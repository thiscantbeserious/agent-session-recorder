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
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    cursor::{Hide, Show},
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::asciicast::AsciicastFile;
use crate::terminal::TerminalBuffer;

use super::input::handle_event;
use super::playback::collect_markers;
use super::render::{
    render_help, render_progress_bar, render_scroll_indicator, render_separator_line,
    render_single_line, render_status_bar, render_viewport,
};
use super::state::{InputResult, PlaybackState};

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
    let (term_cols, term_rows) = crossterm::terminal::size()?;

    // Initialize playback state
    let mut state = PlaybackState::new(term_cols, term_rows);

    // Setup terminal
    let mut stdout = io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, Hide, EnableMouseCapture)?;

    let result = run_main_loop(
        &mut stdout,
        &mut buffer,
        &mut state,
        &cast,
        &markers,
        total_duration,
        rec_cols,
        rec_rows,
        &name,
    );

    // Cleanup
    execute!(stdout, Show, DisableMouseCapture, LeaveAlternateScreen)?;
    crossterm::terminal::disable_raw_mode()?;

    result
}

/// Main playback loop
#[allow(clippy::too_many_arguments)]
fn run_main_loop(
    stdout: &mut io::Stdout,
    buffer: &mut TerminalBuffer,
    state: &mut PlaybackState,
    cast: &AsciicastFile,
    markers: &[super::state::MarkerPosition],
    total_duration: f64,
    rec_cols: u32,
    rec_rows: u32,
    name: &str,
) -> Result<PlaybackResult> {
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
            let event = event::read()?;

            let result = handle_event(
                event,
                state,
                buffer,
                cast,
                markers,
                total_duration,
                rec_cols,
                rec_rows,
            );

            match result {
                InputResult::Quit => return Ok(PlaybackResult::Interrupted),
                InputResult::QuitWithFile => return Ok(PlaybackResult::Success(name.to_string())),
                InputResult::Continue => {}
            }
        }

        // Process events if not paused
        if !state.paused {
            let elapsed =
                state.start_time.elapsed().as_secs_f64() * state.speed + state.time_offset();
            // Cap elapsed time to total duration
            let elapsed = elapsed.min(total_duration);
            state.set_current_time(elapsed, total_duration);
            state.needs_render = true; // Always render when playing (time changes)

            while state.event_idx() < cast.events.len() {
                let evt = &cast.events[state.event_idx()];
                let next_time = state.cumulative_time() + evt.time;

                if next_time > elapsed {
                    break;
                }

                state.set_cumulative_time(next_time);

                if evt.is_output() {
                    buffer.process(&evt.data, None);
                } else if let Some((cols, rows)) = evt.parse_resize() {
                    buffer.resize(cols as usize, rows as usize);
                }

                state.increment_event_idx(cast.events.len());
            }
        }

        // Render only when needed
        if !state.needs_render {
            std::thread::sleep(Duration::from_millis(8));
            continue;
        }
        state.needs_render = false;

        if state.show_help {
            render_help(stdout, state.term_cols, state.term_rows)?;
        } else {
            // Begin synchronized update to prevent flicker
            write!(stdout, "\x1b[?2026h")?;

            // Partial update: only re-render changed highlight lines in free mode
            // Skip all UI chrome (progress bar, status bar, etc.) for partial updates
            if state.free_line_only && state.free_mode {
                render_single_line(
                    stdout,
                    buffer,
                    state.prev_free_line,
                    state.view_row_offset(),
                    state.view_col_offset(),
                    state.view_cols,
                    false, // not highlighted
                )?;
                render_single_line(
                    stdout,
                    buffer,
                    state.free_line(),
                    state.view_row_offset(),
                    state.view_col_offset(),
                    state.view_cols,
                    true, // highlighted
                )?;
                state.free_line_only = false;
                // End synchronized update and skip UI chrome
                write!(stdout, "\x1b[?2026l")?;
                stdout.flush()?;
                continue; // Skip the sleep at end of loop for faster response
            } else {
                render_viewport(
                    stdout,
                    buffer,
                    state.view_row_offset(),
                    state.view_col_offset(),
                    state.view_rows,
                    state.view_cols,
                    if state.free_mode {
                        Some(state.free_line())
                    } else {
                        None
                    },
                )?;

                // Show scroll indicator if viewport can scroll
                render_scroll_indicator(
                    stdout,
                    state.term_cols,
                    state.view_row_offset(),
                    state.view_col_offset(),
                    state.view_rows,
                    state.view_cols,
                    buffer.height(),
                    buffer.width(),
                )?;

                render_separator_line(stdout, state.term_cols, state.term_rows.saturating_sub(3))?;

                render_progress_bar(
                    stdout,
                    state.term_cols,
                    state.term_rows.saturating_sub(2),
                    state.current_time(),
                    total_duration,
                    markers,
                )?;

                render_status_bar(
                    stdout,
                    state.term_cols,
                    state.term_rows.saturating_sub(1),
                    state.paused,
                    state.speed,
                    buffer.width() as u32,
                    buffer.height() as u32,
                    state.view_cols,
                    state.view_rows,
                    state.view_col_offset(),
                    state.view_row_offset(),
                    markers.len(),
                    state.viewport_mode,
                    state.free_mode,
                )?;

                // End synchronized update
                write!(stdout, "\x1b[?2026l")?;
            }
        }

        stdout.flush()?;

        if state.event_idx() >= cast.events.len() && !state.paused {
            std::thread::sleep(Duration::from_millis(500));
            return Ok(PlaybackResult::Success(name.to_string()));
        }

        std::thread::sleep(Duration::from_millis(8));
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
}
