//! Lazy session preview data (duration, markers, terminal snapshot).

use std::io::BufRead;
use std::path::Path;

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::asciicast::{EventType, Header};
use crate::files::codec;
use crate::terminal::TerminalBuffer;

use crate::terminal::{Color, StyledLine};

/// Enhanced preview information for a session file.
///
/// This data is loaded lazily when a file is selected for preview.
#[derive(Debug, Clone)]
pub struct SessionPreview {
    /// Total duration of the recording in seconds
    pub duration_secs: f64,
    /// Number of marker events
    pub marker_count: usize,
    /// Terminal snapshot at 10% of recording (with color info)
    pub styled_preview: Vec<StyledLine>,
    /// Uncompressed size, only set for gzip sessions (plain sessions equal the on-disk size)
    pub uncompressed_size: Option<codec::UncompressedSize>,
}

/// Process terminal output for the first ~30 seconds (enough for the preview snapshot),
/// keep scanning for total duration and marker count.
struct Scan {
    buffer: TerminalBuffer,
    duration: f64,
    markers: usize,
    styled: Option<Vec<StyledLine>>,
}

const PREVIEW_THRESHOLD_SECS: f64 = 30.0;

impl Scan {
    fn from_header(line: &str) -> Option<Self> {
        let header: Header = serde_json::from_str(line).ok()?;
        if header.version != 3 {
            return None;
        }
        let cols = header.term.as_ref().and_then(|t| t.cols).unwrap_or(80) as usize;
        let rows = header.term.as_ref().and_then(|t| t.rows).unwrap_or(24) as usize;
        Some(Self {
            buffer: TerminalBuffer::new(cols, rows),
            duration: 0.0,
            markers: 0,
            styled: None,
        })
    }

    /// Feed all lines, skipping invalid UTF-8 lines but stopping at a real I/O error
    /// (a corrupt gzip stream keeps failing, so it must not be retried).
    fn feed_all(&mut self, lines: impl Iterator<Item = std::io::Result<String>>) {
        for line in lines {
            match line {
                Ok(l) => self.feed(&l),
                Err(e) if e.kind() != std::io::ErrorKind::InvalidData => break,
                Err(_) => {}
            }
        }
    }

    fn feed(&mut self, line: &str) {
        let Some((time, event_type, data)) = SessionPreview::parse_event_minimal(line) else {
            return;
        };
        self.duration += time;
        if event_type == EventType::Marker {
            self.markers += 1;
        }
        if self.styled.is_some() {
            return;
        }
        if let (EventType::Output, Some(output)) = (event_type, data) {
            self.buffer.process(&output, None);
        }
        if self.duration >= PREVIEW_THRESHOLD_SECS {
            self.styled = Some(self.buffer.styled_lines());
        }
    }

    fn finish(self, uncompressed_size: Option<codec::UncompressedSize>) -> SessionPreview {
        SessionPreview {
            duration_secs: self.duration,
            marker_count: self.markers,
            styled_preview: self.styled.unwrap_or_else(|| self.buffer.styled_lines()),
            uncompressed_size,
        }
    }
}

impl SessionPreview {
    /// Load preview information from an asciicast file using streaming parsing.
    ///
    /// This is optimized to avoid loading the entire file into memory:
    /// - Parses header for terminal size
    /// - Streams events, only processing terminal output for first ~10%
    /// - Counts markers and sums times for full duration
    ///
    /// Returns None if the file cannot be parsed.
    pub fn load<P: AsRef<Path>>(path: P) -> Option<Self> {
        Self::load_streaming(path)
    }

    /// Streaming loader that minimizes memory usage and processing time.
    ///
    /// Single-pass approach (see [`Scan`]), never stores all events in memory.
    /// Reads `.cast` and `.cast.gz` alike through `codec::open_reader`.
    fn load_streaming<P: AsRef<Path>>(path: P) -> Option<Self> {
        let path = path.as_ref();
        let mut lines = BufRead::lines(codec::open_reader(path).ok()?);
        let mut scan = Scan::from_header(&lines.next()?.ok()?)?;
        scan.feed_all(lines);
        let uncompressed_size = codec::is_gzip_path(path)
            .then(|| codec::uncompressed_size(path).ok())
            .flatten();
        Some(scan.finish(uncompressed_size))
    }

    /// Minimal event parsing - only extracts what we need
    fn parse_event_minimal(line: &str) -> Option<(f64, EventType, Option<String>)> {
        let value: serde_json::Value = serde_json::from_str(line).ok()?;
        let arr = value.as_array()?;
        if arr.len() < 2 {
            return None;
        }

        let time = arr[0].as_f64()?;
        let type_str = arr[1].as_str()?;
        let event_type = EventType::from_code(type_str)?;

        // Only extract data for output events (avoid string allocation for markers)
        let data = if event_type == EventType::Output && arr.len() >= 3 {
            arr[2].as_str().map(String::from)
        } else {
            None
        };

        Some((time, event_type, data))
    }

    /// Convert our Color enum to ratatui Color
    fn to_ratatui_color(color: Color) -> ratatui::style::Color {
        match color {
            Color::Default => ratatui::style::Color::Reset,
            Color::Black => ratatui::style::Color::Black,
            Color::Red => ratatui::style::Color::Red,
            Color::Green => ratatui::style::Color::Green,
            Color::Yellow => ratatui::style::Color::Yellow,
            Color::Blue => ratatui::style::Color::Blue,
            Color::Magenta => ratatui::style::Color::Magenta,
            Color::Cyan => ratatui::style::Color::Cyan,
            Color::White => ratatui::style::Color::White,
            Color::BrightBlack => ratatui::style::Color::DarkGray,
            Color::BrightRed => ratatui::style::Color::LightRed,
            Color::BrightGreen => ratatui::style::Color::LightGreen,
            Color::BrightYellow => ratatui::style::Color::LightYellow,
            Color::BrightBlue => ratatui::style::Color::LightBlue,
            Color::BrightMagenta => ratatui::style::Color::LightMagenta,
            Color::BrightCyan => ratatui::style::Color::LightCyan,
            Color::BrightWhite => ratatui::style::Color::White,
            Color::Indexed(idx) => ratatui::style::Color::Indexed(idx),
            Color::Rgb(r, g, b) => ratatui::style::Color::Rgb(r, g, b),
        }
    }

    /// Convert a StyledLine to a ratatui Line with colors
    pub fn styled_line_to_ratatui(line: &StyledLine) -> Line<'static> {
        // Group consecutive cells with same style into spans
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut current_text = String::new();
        let mut current_style: Option<crate::terminal::CellStyle> = None;

        for cell in &line.cells {
            if Some(cell.style) != current_style {
                spans.extend(Self::flush_span(&mut current_text, current_style));
                current_style = Some(cell.style);
            }
            current_text.push(cell.char);
        }
        spans.extend(Self::flush_span(&mut current_text, current_style));

        Line::from(spans)
    }

    /// Turn the collected text into a span (none when empty), leaving `text` empty.
    fn flush_span(
        text: &mut String,
        style: Option<crate::terminal::CellStyle>,
    ) -> Option<Span<'static>> {
        if text.is_empty() {
            return None;
        }
        let text = std::mem::take(text);
        Some(match style {
            Some(style) => Span::styled(text, Self::to_ratatui_style(style)),
            None => Span::raw(text),
        })
    }

    fn to_ratatui_style(style: crate::terminal::CellStyle) -> Style {
        let mut out = Style::default();
        if style.fg != Color::Default {
            out = out.fg(Self::to_ratatui_color(style.fg));
        }
        if style.bg != Color::Default {
            out = out.bg(Self::to_ratatui_color(style.bg));
        }
        let modifiers = [
            (style.bold, Modifier::BOLD),
            (style.dim, Modifier::DIM),
            (style.italic, Modifier::ITALIC),
            (style.underline, Modifier::UNDERLINED),
        ];
        for (on, modifier) in modifiers {
            if on {
                out = out.add_modifier(modifier);
            }
        }
        out
    }

    /// Format duration as human-readable string (e.g., "5m 32s").
    pub fn format_duration(&self) -> String {
        let total_secs = self.duration_secs as u64;
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let seconds = total_secs % 60;

        if hours > 0 {
            format!("{}h {}m {}s", hours, minutes, seconds)
        } else if minutes > 0 {
            format!("{}m {}s", minutes, seconds)
        } else {
            format!("{}s", seconds)
        }
    }
}
