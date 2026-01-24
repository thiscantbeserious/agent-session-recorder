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
/// - Arrow keys: Scroll viewport
/// - +/-: Adjust speed
/// - [/]: Jump to prev/next marker
/// - </> or ,/.: Seek backward/forward 5s
/// - Home/End: Go to start/end
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
    let status_lines = 3; // Separator + progress bar + status bar
    let view_rows = (term_rows.saturating_sub(status_lines)) as usize;
    let view_cols = term_cols as usize;

    // Viewport offset (for scrolling)
    let mut view_row_offset: usize = 0;
    let mut view_col_offset: usize = 0;

    // Playback state
    let mut paused = false;
    let mut speed = 1.0f64;
    let mut event_idx = 0;
    let mut current_time = 0.0f64;
    let mut show_help = false;
    let start_time = Instant::now();
    let mut time_offset = 0.0f64;

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
            // Handle input
            if event::poll(Duration::from_millis(16))? {
                if let Event::Key(key) = event::read()? {
                    if show_help {
                        show_help = false;
                        continue;
                    }

                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            return Ok(PlaybackResult::Interrupted);
                        }
                        KeyCode::Char(' ') => {
                            paused = !paused;
                            if !paused {
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
                        // Marker navigation
                        KeyCode::Char(']') => {
                            if let Some(next) = markers.iter().find(|m| m.time > current_time + 0.1)
                            {
                                seek_to_time(&mut buffer, &cast, next.time, rec_cols, rec_rows);
                                current_time = next.time;
                                time_offset = current_time;
                                event_idx = find_event_index_at_time(&cast, current_time);
                                paused = true;
                            }
                        }
                        KeyCode::Char('[') => {
                            if let Some(prev) =
                                markers.iter().rev().find(|m| m.time < current_time - 0.1)
                            {
                                seek_to_time(&mut buffer, &cast, prev.time, rec_cols, rec_rows);
                                current_time = prev.time;
                                time_offset = current_time;
                                event_idx = find_event_index_at_time(&cast, current_time);
                                paused = true;
                            }
                        }
                        // Seeking
                        KeyCode::Char('<') | KeyCode::Char(',') => {
                            let new_time = (current_time - 5.0).max(0.0);
                            seek_to_time(&mut buffer, &cast, new_time, rec_cols, rec_rows);
                            current_time = new_time;
                            time_offset = current_time;
                            event_idx = find_event_index_at_time(&cast, current_time);
                        }
                        KeyCode::Char('>') | KeyCode::Char('.') => {
                            let new_time = (current_time + 5.0).min(total_duration);
                            current_time = new_time;
                            time_offset = current_time;
                            event_idx = find_event_index_at_time(&cast, current_time);
                            buffer = TerminalBuffer::new(rec_cols as usize, rec_rows as usize);
                            process_up_to_time(&mut buffer, current_time, &cast);
                        }
                        // Viewport scrolling
                        KeyCode::Up => {
                            view_row_offset = view_row_offset.saturating_sub(1);
                        }
                        KeyCode::Down => {
                            let max_offset = (rec_rows as usize).saturating_sub(view_rows);
                            view_row_offset = (view_row_offset + 1).min(max_offset);
                        }
                        KeyCode::Left => {
                            view_col_offset = view_col_offset.saturating_sub(1);
                        }
                        KeyCode::Right => {
                            let max_offset = (rec_cols as usize).saturating_sub(view_cols);
                            view_col_offset = (view_col_offset + 1).min(max_offset);
                        }
                        KeyCode::Home => {
                            seek_to_time(&mut buffer, &cast, 0.0, rec_cols, rec_rows);
                            current_time = 0.0;
                            time_offset = 0.0;
                            event_idx = 0;
                            view_row_offset = 0;
                            view_col_offset = 0;
                        }
                        KeyCode::End => {
                            buffer = TerminalBuffer::new(rec_cols as usize, rec_rows as usize);
                            process_up_to_time(&mut buffer, total_duration, &cast);
                            current_time = total_duration;
                            time_offset = current_time;
                            event_idx = cast.events.len();
                            paused = true;
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
                let elapsed = start_time.elapsed().as_secs_f64() * speed + time_offset;
                current_time = elapsed;

                let mut cumulative = 0.0f64;
                for (i, evt) in cast.events.iter().enumerate() {
                    if i < event_idx {
                        cumulative += evt.time;
                    }
                }

                while event_idx < cast.events.len() {
                    let evt = &cast.events[event_idx];
                    cumulative += evt.time;

                    if cumulative > elapsed {
                        break;
                    }

                    if evt.is_output() {
                        buffer.process(&evt.data);
                    }

                    event_idx += 1;
                    current_time = cumulative;
                }
            }

            // Render
            if show_help {
                render_help(&mut stdout, term_cols, term_rows)?;
            } else {
                render_viewport(
                    &mut stdout,
                    &buffer,
                    view_row_offset,
                    view_col_offset,
                    view_rows,
                    view_cols,
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
                )?;
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

/// Find the event index at a given time.
fn find_event_index_at_time(cast: &AsciicastFile, target_time: f64) -> usize {
    let mut cumulative = 0.0f64;
    for (i, event) in cast.events.iter().enumerate() {
        cumulative += event.time;
        if cumulative > target_time {
            return i;
        }
    }
    cast.events.len()
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

    execute!(
        stdout,
        MoveTo(0, row),
        Clear(ClearType::CurrentLine),
        SetBackgroundColor(Color::AnsiValue(236)),
        Print(" "),
    )?;

    execute!(stdout, SetForegroundColor(Color::Green))?;
    for (i, &c) in bar.iter().enumerate() {
        if i < filled {
            if c == '◆' {
                execute!(
                    stdout,
                    SetForegroundColor(Color::Yellow),
                    Print(c),
                    SetForegroundColor(Color::Green)
                )?;
            } else {
                write!(stdout, "━")?;
            }
        } else if i == filled {
            execute!(stdout, SetForegroundColor(Color::White), Print(c))?;
        } else if c == '◆' {
            execute!(stdout, SetForegroundColor(Color::Yellow), Print(c))?;
        } else {
            execute!(stdout, SetForegroundColor(Color::DarkGrey), Print(c))?;
        }
    }

    execute!(
        stdout,
        SetForegroundColor(Color::Grey),
        Print(&time_display),
    )?;

    // Fill remaining width with background
    let used_width = 1 + bar_width + time_display.len(); // padding + bar + time
    let remaining = (width as usize).saturating_sub(used_width);
    for _ in 0..remaining {
        write!(stdout, " ")?;
    }

    execute!(stdout, ResetColor)?;

    Ok(())
}

/// Render a separator line.
fn render_separator_line(stdout: &mut io::Stdout, width: u16, row: u16) -> Result<()> {
    execute!(
        stdout,
        MoveTo(0, row),
        Clear(ClearType::CurrentLine),
        SetForegroundColor(Color::DarkGrey),
    )?;

    for _ in 0..width {
        write!(stdout, "─")?;
    }

    execute!(stdout, ResetColor)?;
    Ok(())
}

/// Render the status/controls bar.
#[allow(clippy::too_many_arguments)]
fn render_status_bar(
    stdout: &mut io::Stdout,
    _width: u16,
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
) -> Result<()> {
    execute!(
        stdout,
        MoveTo(0, row),
        Clear(ClearType::CurrentLine),
        Print(" "),
        SetForegroundColor(Color::White),
    )?;

    let state = if paused { "⏸  " } else { "▶  " };
    execute!(stdout, Print(state))?;

    execute!(
        stdout,
        SetForegroundColor(Color::DarkGrey),
        Print("spd:"),
        SetForegroundColor(Color::White),
        Print(format!("{:.1}x ", speed)),
    )?;

    if marker_count > 0 {
        execute!(
            stdout,
            SetForegroundColor(Color::Yellow),
            Print(format!("◆{} ", marker_count)),
        )?;
    }

    if rec_cols as usize > view_cols || rec_rows as usize > view_rows {
        execute!(
            stdout,
            SetForegroundColor(Color::DarkGrey),
            Print(format!("[{},{}] ", col_offset, row_offset)),
        )?;
    }

    execute!(
        stdout,
        SetForegroundColor(Color::DarkGrey),
        Print("│ "),
        SetForegroundColor(Color::Cyan),
        Print("space"),
        SetForegroundColor(Color::DarkGrey),
        Print(":play "),
        SetForegroundColor(Color::Cyan),
        Print("+/-"),
        SetForegroundColor(Color::DarkGrey),
        Print(":speed "),
        SetForegroundColor(Color::Cyan),
        Print("[/]"),
        SetForegroundColor(Color::DarkGrey),
        Print(":marker "),
        SetForegroundColor(Color::Cyan),
        Print("</>"),
        SetForegroundColor(Color::DarkGrey),
        Print(":seek "),
        SetForegroundColor(Color::Cyan),
        Print("?"),
        SetForegroundColor(Color::DarkGrey),
        Print(":help "),
        SetForegroundColor(Color::Cyan),
        Print("q"),
        SetForegroundColor(Color::DarkGrey),
        Print(":quit"),
        ResetColor,
    )?;

    Ok(())
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
        "  ║    +/-        Increase / Decrease speed   ║",
        "  ║    < / >      Seek backward / forward 5s  ║",
        "  ║    Home       Go to start                 ║",
        "  ║    End        Go to end                   ║",
        "  ║                                           ║",
        "  ║  Markers                                  ║",
        "  ║    [          Jump to previous marker     ║",
        "  ║    ]          Jump to next marker         ║",
        "  ║                                           ║",
        "  ║  Viewport                                 ║",
        "  ║    ↑↓←→       Scroll viewport             ║",
        "  ║                                           ║",
        "  ║  General                                  ║",
        "  ║    ?          Show this help              ║",
        "  ║    q / Esc    Quit player                 ║",
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
fn render_viewport(
    stdout: &mut io::Stdout,
    buffer: &TerminalBuffer,
    row_offset: usize,
    col_offset: usize,
    view_rows: usize,
    view_cols: usize,
) -> Result<()> {
    let styled_lines = buffer.styled_lines();

    for view_row in 0..view_rows {
        let buf_row = view_row + row_offset;

        execute!(
            stdout,
            MoveTo(0, view_row as u16),
            Clear(ClearType::CurrentLine)
        )?;

        if buf_row < styled_lines.len() {
            let line = &styled_lines[buf_row];
            let mut current_style = CellStyle::default();

            for view_col in 0..view_cols {
                let buf_col = view_col + col_offset;

                if buf_col < line.cells.len() {
                    let cell = &line.cells[buf_col];

                    if cell.style != current_style {
                        apply_style(stdout, &cell.style)?;
                        current_style = cell.style;
                    }

                    write!(stdout, "{}", cell.char)?;
                } else {
                    if current_style != CellStyle::default() {
                        execute!(stdout, ResetColor)?;
                        current_style = CellStyle::default();
                    }
                    write!(stdout, " ")?;
                }
            }

            if current_style != CellStyle::default() {
                execute!(stdout, ResetColor)?;
            }
        }
    }

    Ok(())
}

/// Apply a cell style to the terminal.
fn apply_style(stdout: &mut io::Stdout, style: &CellStyle) -> Result<()> {
    execute!(stdout, ResetColor)?;

    if let Some(color) = convert_color(&style.fg) {
        execute!(stdout, SetForegroundColor(color))?;
    }

    if let Some(color) = convert_color(&style.bg) {
        execute!(stdout, SetBackgroundColor(color))?;
    }

    Ok(())
}

/// Convert our color enum to crossterm Color.
fn convert_color(color: &TermColor) -> Option<Color> {
    match color {
        TermColor::Default => None,
        TermColor::Black => Some(Color::Black),
        TermColor::Red => Some(Color::DarkRed),
        TermColor::Green => Some(Color::DarkGreen),
        TermColor::Yellow => Some(Color::DarkYellow),
        TermColor::Blue => Some(Color::DarkBlue),
        TermColor::Magenta => Some(Color::DarkMagenta),
        TermColor::Cyan => Some(Color::DarkCyan),
        TermColor::White => Some(Color::Grey),
        TermColor::BrightBlack => Some(Color::DarkGrey),
        TermColor::BrightRed => Some(Color::Red),
        TermColor::BrightGreen => Some(Color::Green),
        TermColor::BrightYellow => Some(Color::Yellow),
        TermColor::BrightBlue => Some(Color::Blue),
        TermColor::BrightMagenta => Some(Color::Magenta),
        TermColor::BrightCyan => Some(Color::Cyan),
        TermColor::BrightWhite => Some(Color::White),
        TermColor::Indexed(idx) => Some(Color::AnsiValue(*idx)),
        TermColor::Rgb(r, g, b) => Some(Color::Rgb {
            r: *r,
            g: *g,
            b: *b,
        }),
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
    fn convert_color_default_is_none() {
        assert!(convert_color(&TermColor::Default).is_none());
    }

    #[test]
    fn convert_color_basic_colors() {
        assert!(matches!(
            convert_color(&TermColor::Red),
            Some(Color::DarkRed)
        ));
    }
}
