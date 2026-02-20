//! Native asciicast player module
//!
//! Provides functionality for playing back asciicast recordings with:
//!
//! - Size-independent rendering via virtual terminal
//! - Progress bar with marker indicators
//! - Seeking and speed control
//! - Viewport scrolling
//! - Help overlay
//!
//! # Architecture
//!
//! The player is organized into submodules:
//! - `state`: PlaybackState struct and shared types (MarkerPosition, InputResult)
//! - `input/`: Keyboard and mouse input handling
//! - `playback/`: Seeking, marker collection, and time management
//! - `render/`: UI rendering (viewport, progress bar, status bar, help, scroll indicators)
//!
//! # Usage
//!
//! ```no_run
//! use agr::player::{play_session, PlaybackResult};
//! use std::path::Path;
//!
//! let result = play_session(Path::new("session.cast")).unwrap();
//! match result {
//!     PlaybackResult::Success(name) => println!("Finished: {}", name),
//!     PlaybackResult::Interrupted => println!("Stopped by user"),
//!     PlaybackResult::Error(e) => eprintln!("Error: {}", e),
//! }
//! ```

pub(crate) mod input;
pub(crate) mod playback;
pub mod render;
pub mod state;

pub use state::{InputResult, MarkerPosition, PlaybackState};

use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    cursor::{Hide, Show},
    event::{self},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::asciicast::AsciicastFile;
use crate::terminal::TerminalBuffer;

use input::handle_event;
use playback::collect_markers;
use render::{
    render_help, render_progress_bar, render_scroll_indicator, render_separator_line,
    render_single_line, render_status_bar, render_viewport,
};

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
    execute!(stdout, EnterAlternateScreen, Hide)?;

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
    execute!(stdout, Show, LeaveAlternateScreen)?;
    crossterm::terminal::disable_raw_mode()?;

    result
}

/// Poll and handle all pending input events.
///
/// Returns `Some(PlaybackResult)` if the user requests quit, or `None` to continue.
/// First poll waits up to 16ms; subsequent polls use zero timeout to drain the queue.
#[allow(clippy::too_many_arguments)]
fn process_pending_events(
    state: &mut PlaybackState,
    buffer: &mut TerminalBuffer,
    cast: &AsciicastFile,
    markers: &[MarkerPosition],
    total_duration: f64,
    rec_cols: u32,
    rec_rows: u32,
    name: &str,
) -> Result<Option<PlaybackResult>> {
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
            InputResult::Quit => return Ok(Some(PlaybackResult::Interrupted)),
            InputResult::QuitWithFile => {
                return Ok(Some(PlaybackResult::Success(name.to_string())))
            }
            InputResult::Continue => {}
        }
    }
    Ok(None)
}

/// Advance playback by processing cast events up to the current elapsed time.
///
/// No-op when paused. Sets `needs_render = true` while playing since time always changes.
pub fn advance_playback(
    state: &mut PlaybackState,
    buffer: &mut TerminalBuffer,
    cast: &AsciicastFile,
    total_duration: f64,
) {
    if state.paused {
        return;
    }

    let elapsed = state.start_time.elapsed().as_secs_f64() * state.speed + state.time_offset();
    let elapsed = elapsed.min(total_duration);
    state.set_current_time(elapsed, total_duration);
    state.needs_render = true;

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

/// Render one frame to stdout.
///
/// Returns `true` if the caller should skip the end-of-loop sleep (partial update path),
/// `false` if the normal 8ms sleep should occur.
#[allow(clippy::too_many_arguments)]
fn render_frame(
    stdout: &mut io::Stdout,
    buffer: &TerminalBuffer,
    state: &mut PlaybackState,
    markers: &[MarkerPosition],
    total_duration: f64,
) -> Result<bool> {
    if state.show_help {
        render_help(stdout, state.term_cols, state.term_rows)?;
        return Ok(false);
    }

    // Begin synchronized update to prevent flicker
    write!(stdout, "\x1b[?2026h")?;

    // Partial update: only re-render changed highlight lines in free mode
    if state.free_line_only && state.free_mode {
        render_single_line(
            stdout,
            buffer,
            state.prev_free_line,
            state.view_row_offset(),
            state.view_col_offset(),
            state.view_cols,
            false,
        )?;
        render_single_line(
            stdout,
            buffer,
            state.free_line(),
            state.view_row_offset(),
            state.view_col_offset(),
            state.view_cols,
            true,
        )?;
        state.free_line_only = false;
        write!(stdout, "\x1b[?2026l")?;
        stdout.flush()?;
        return Ok(true); // Skip the sleep at end of loop for faster response
    }

    render_full_frame(stdout, buffer, state, markers, total_duration)?;
    Ok(false)
}

/// Render the full frame: viewport, scroll indicator, separator, progress bar, status bar.
///
/// Called from `render_frame()` when a full redraw is needed (not a partial free-mode update).
fn render_full_frame(
    stdout: &mut io::Stdout,
    buffer: &TerminalBuffer,
    state: &PlaybackState,
    markers: &[MarkerPosition],
    total_duration: f64,
) -> Result<()> {
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

    write!(stdout, "\x1b[?2026l")?;
    Ok(())
}

/// Main playback loop
#[allow(clippy::too_many_arguments)]
fn run_main_loop(
    stdout: &mut io::Stdout,
    buffer: &mut TerminalBuffer,
    state: &mut PlaybackState,
    cast: &AsciicastFile,
    markers: &[MarkerPosition],
    total_duration: f64,
    rec_cols: u32,
    rec_rows: u32,
    name: &str,
) -> Result<PlaybackResult> {
    loop {
        if let Some(result) = process_pending_events(
            state,
            buffer,
            cast,
            markers,
            total_duration,
            rec_cols,
            rec_rows,
            name,
        )? {
            return Ok(result);
        }

        advance_playback(state, buffer, cast, total_duration);

        if !state.needs_render {
            std::thread::sleep(Duration::from_millis(8));
            continue;
        }
        state.needs_render = false;

        let skip_sleep = render_frame(stdout, buffer, state, markers, total_duration)?;

        stdout.flush()?;

        if state.event_idx() >= cast.events.len() && !state.paused {
            std::thread::sleep(Duration::from_millis(500));
            return Ok(PlaybackResult::Success(name.to_string()));
        }

        if !skip_sleep {
            std::thread::sleep(Duration::from_millis(8));
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
