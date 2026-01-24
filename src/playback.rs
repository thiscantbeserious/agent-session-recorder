//! Session playback functionality
//!
//! Provides multiple playback options:
//! - Native player: Renders through virtual terminal, works at any size
//! - asciinema: Shells out to asciinema (may have size issues)

use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::asciicast::AsciicastFile;
use crate::terminal_buffer::{self, TerminalBuffer};

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

/// Play a session using the native renderer.
///
/// This renders the recording through a virtual terminal buffer, allowing
/// playback at any terminal size. The virtual terminal matches the original
/// recording dimensions, and a viewport shows the visible portion.
///
/// Controls:
/// - q/Esc: Quit
/// - Space: Pause/resume
/// - Arrow keys: Scroll viewport (when paused)
/// - +/-: Adjust speed
pub fn play_session_native(path: &Path) -> Result<PlaybackResult> {
    let cast = AsciicastFile::parse(path)?;
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // Get recording dimensions
    let (rec_cols, rec_rows) = cast.terminal_size();

    // Create virtual terminal at recording size
    let mut buffer = TerminalBuffer::new(rec_cols as usize, rec_rows as usize);

    // Get current terminal size for viewport
    let (term_cols, term_rows) = terminal::size()?;
    let view_rows = (term_rows.saturating_sub(1)) as usize; // Leave room for status line
    let view_cols = term_cols as usize;

    // Viewport offset (for scrolling)
    let mut view_row_offset: usize = 0;
    let mut view_col_offset: usize = 0;

    // Playback state
    let mut paused = false;
    let mut speed = 1.0f64;
    let mut event_idx = 0;
    let mut cumulative_time = 0.0f64;
    let last_render = Instant::now();

    // Setup terminal
    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, Hide)?;

    let result = (|| -> Result<PlaybackResult> {
        loop {
            // Handle input
            if event::poll(Duration::from_millis(16))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            return Ok(PlaybackResult::Interrupted);
                        }
                        KeyCode::Char(' ') => paused = !paused,
                        KeyCode::Char('+') | KeyCode::Char('=') => {
                            speed = (speed * 1.5).min(16.0);
                        }
                        KeyCode::Char('-') | KeyCode::Char('_') => {
                            speed = (speed / 1.5).max(0.1);
                        }
                        KeyCode::Up if paused => {
                            view_row_offset = view_row_offset.saturating_sub(1);
                        }
                        KeyCode::Down if paused => {
                            let max_offset = (rec_rows as usize).saturating_sub(view_rows);
                            view_row_offset = (view_row_offset + 1).min(max_offset);
                        }
                        KeyCode::Left if paused => {
                            view_col_offset = view_col_offset.saturating_sub(1);
                        }
                        KeyCode::Right if paused => {
                            let max_offset = (rec_cols as usize).saturating_sub(view_cols);
                            view_col_offset = (view_col_offset + 1).min(max_offset);
                        }
                        KeyCode::Home => {
                            view_row_offset = 0;
                            view_col_offset = 0;
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            return Ok(PlaybackResult::Interrupted);
                        }
                        _ => {}
                    }
                }
            }

            // Process events if not paused
            if !paused && event_idx < cast.events.len() {
                let elapsed = last_render.elapsed().as_secs_f64() * speed;

                while event_idx < cast.events.len() {
                    let event = &cast.events[event_idx];

                    if cumulative_time + event.time > elapsed {
                        break;
                    }

                    cumulative_time += event.time;

                    if event.is_output() {
                        buffer.process(&event.data);
                    }

                    event_idx += 1;
                }
            }

            // Render viewport
            render_viewport(
                &mut stdout,
                &buffer,
                view_row_offset,
                view_col_offset,
                view_rows,
                view_cols,
            )?;

            // Render status line
            let progress = if cast.events.is_empty() {
                100.0
            } else {
                (event_idx as f64 / cast.events.len() as f64) * 100.0
            };

            let status = format!(
                " {}  {:.0}%  {:.1}x  {}",
                if paused { "⏸ PAUSED" } else { "▶ PLAYING" },
                progress,
                speed,
                if rec_cols as usize > view_cols || rec_rows as usize > view_rows {
                    format!(
                        "[{},{}]/[{},{}] ↑↓←→:scroll",
                        view_col_offset, view_row_offset, rec_cols, rec_rows
                    )
                } else {
                    String::new()
                }
            );

            execute!(
                stdout,
                MoveTo(0, term_rows - 1),
                Clear(ClearType::CurrentLine),
                SetBackgroundColor(Color::AnsiValue(236)), // True dark grey
                SetForegroundColor(Color::White),
                Print(&status),
                ResetColor
            )?;

            stdout.flush()?;

            // Check if playback is complete
            if event_idx >= cast.events.len() && !paused {
                // Wait a moment at the end
                std::thread::sleep(Duration::from_millis(500));
                return Ok(PlaybackResult::Success(name));
            }

            // Small sleep to prevent busy loop
            std::thread::sleep(Duration::from_millis(8));
        }
    })();

    // Cleanup terminal
    execute!(stdout, Show, LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;

    result
}

/// Render a viewport of the terminal buffer to stdout.
fn render_viewport(
    stdout: &mut io::Stdout,
    buffer: &TerminalBuffer,
    row_offset: usize,
    col_offset: usize,
    view_rows: usize,
    view_cols: usize,
) -> Result<()> {
    let styled_lines = buffer.styled_lines();

    execute!(stdout, MoveTo(0, 0))?;

    for view_row in 0..view_rows {
        let buf_row = view_row + row_offset;

        if buf_row < styled_lines.len() {
            let line = &styled_lines[buf_row];

            // Move to start of line
            execute!(
                stdout,
                MoveTo(0, view_row as u16),
                Clear(ClearType::CurrentLine)
            )?;

            let mut current_style = terminal_buffer::CellStyle::default();

            for view_col in 0..view_cols {
                let buf_col = view_col + col_offset;

                if buf_col < line.cells.len() {
                    let cell = &line.cells[buf_col];

                    // Apply style changes
                    if cell.style != current_style {
                        apply_style(stdout, &cell.style)?;
                        current_style = cell.style;
                    }

                    write!(stdout, "{}", cell.char)?;
                } else {
                    // Past end of line content
                    if current_style != terminal_buffer::CellStyle::default() {
                        execute!(stdout, ResetColor)?;
                        current_style = terminal_buffer::CellStyle::default();
                    }
                    write!(stdout, " ")?;
                }
            }

            // Reset at end of line
            if current_style != terminal_buffer::CellStyle::default() {
                execute!(stdout, ResetColor)?;
            }
        } else {
            // Empty line
            execute!(
                stdout,
                MoveTo(0, view_row as u16),
                Clear(ClearType::CurrentLine)
            )?;
        }
    }

    Ok(())
}

/// Apply a cell style to the terminal.
fn apply_style(stdout: &mut io::Stdout, style: &terminal_buffer::CellStyle) -> Result<()> {
    execute!(stdout, ResetColor)?;

    // Apply foreground color
    if let Some(color) = convert_color(&style.fg) {
        execute!(stdout, SetForegroundColor(color))?;
    }

    // Apply background color
    if let Some(color) = convert_color(&style.bg) {
        execute!(stdout, SetBackgroundColor(color))?;
    }

    // Note: crossterm doesn't have simple bold/italic/underline in this API
    // Would need to use SetAttribute for full support

    Ok(())
}

/// Convert our color enum to crossterm Color.
fn convert_color(color: &terminal_buffer::Color) -> Option<Color> {
    match color {
        terminal_buffer::Color::Default => None,
        terminal_buffer::Color::Black => Some(Color::Black),
        terminal_buffer::Color::Red => Some(Color::DarkRed),
        terminal_buffer::Color::Green => Some(Color::DarkGreen),
        terminal_buffer::Color::Yellow => Some(Color::DarkYellow),
        terminal_buffer::Color::Blue => Some(Color::DarkBlue),
        terminal_buffer::Color::Magenta => Some(Color::DarkMagenta),
        terminal_buffer::Color::Cyan => Some(Color::DarkCyan),
        terminal_buffer::Color::White => Some(Color::Grey),
        terminal_buffer::Color::BrightBlack => Some(Color::DarkGrey),
        terminal_buffer::Color::BrightRed => Some(Color::Red),
        terminal_buffer::Color::BrightGreen => Some(Color::Green),
        terminal_buffer::Color::BrightYellow => Some(Color::Yellow),
        terminal_buffer::Color::BrightBlue => Some(Color::Blue),
        terminal_buffer::Color::BrightMagenta => Some(Color::Magenta),
        terminal_buffer::Color::BrightCyan => Some(Color::Cyan),
        terminal_buffer::Color::BrightWhite => Some(Color::White),
        terminal_buffer::Color::Indexed(idx) => Some(Color::AnsiValue(*idx)),
        terminal_buffer::Color::Rgb(r, g, b) => Some(Color::Rgb {
            r: *r,
            g: *g,
            b: *b,
        }),
    }
}

/// Play a session recording using asciinema (legacy method).
///
/// This function assumes the terminal is in normal mode (not TUI mode).
/// The caller is responsible for suspending/resuming any TUI before/after calling this.
///
/// Note: This may have display issues if the recording was made at a different
/// terminal size. Use `play_session_native` for better size handling.
///
/// # Arguments
/// * `path` - Path to the .cast file to play
///
/// # Returns
/// A `PlaybackResult` indicating success, interruption, or error.
pub fn play_session(path: &Path) -> Result<PlaybackResult> {
    // Use native player by default for better size handling
    play_session_native(path)
}

/// Play a session recording using asciinema directly.
///
/// Use this if you want the original asciinema experience with potential
/// size mismatch issues.
pub fn play_session_asciinema(path: &Path) -> Result<PlaybackResult> {
    let status = Command::new("asciinema").arg("play").arg(path).status();

    let result = match status {
        Ok(exit) if exit.success() => {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            PlaybackResult::Success(name)
        }
        Ok(_) => PlaybackResult::Interrupted,
        Err(e) => PlaybackResult::Error(e.to_string()),
    };

    Ok(result)
}

/// Play a session recording with speed multiplier.
///
/// # Arguments
/// * `path` - Path to the .cast file to play
/// * `speed` - Speed multiplier (e.g., 2.0 for 2x speed)
pub fn play_session_with_speed(path: &Path, speed: f64) -> Result<PlaybackResult> {
    let status = Command::new("asciinema")
        .arg("play")
        .arg("--speed")
        .arg(speed.to_string())
        .arg(path)
        .status();

    let result = match status {
        Ok(exit) if exit.success() => {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            PlaybackResult::Success(name)
        }
        Ok(_) => PlaybackResult::Interrupted,
        Err(e) => PlaybackResult::Error(e.to_string()),
    };

    Ok(result)
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
        let debug = format!("{:?}", result);
        assert!(debug.contains("Interrupted"));
    }

    #[test]
    fn convert_color_default_is_none() {
        assert!(convert_color(&terminal_buffer::Color::Default).is_none());
    }

    #[test]
    fn convert_color_basic_colors() {
        assert!(matches!(
            convert_color(&terminal_buffer::Color::Red),
            Some(Color::DarkRed)
        ));
        assert!(matches!(
            convert_color(&terminal_buffer::Color::Green),
            Some(Color::DarkGreen)
        ));
    }

    #[test]
    fn convert_color_bright_colors() {
        assert!(matches!(
            convert_color(&terminal_buffer::Color::BrightRed),
            Some(Color::Red)
        ));
    }

    #[test]
    fn convert_color_indexed() {
        assert!(matches!(
            convert_color(&terminal_buffer::Color::Indexed(196)),
            Some(Color::AnsiValue(196))
        ));
    }

    #[test]
    fn convert_color_rgb() {
        assert!(matches!(
            convert_color(&terminal_buffer::Color::Rgb(255, 128, 64)),
            Some(Color::Rgb {
                r: 255,
                g: 128,
                b: 64
            })
        ));
    }
}
