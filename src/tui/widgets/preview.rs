//! Session preview loading and terminal-to-ratatui style conversion.
//!
//! Handles loading session data from asciicast files and converting
//! terminal output into ratatui rendering primitives for display.
//! Also provides preview prefetching and cache extraction helpers.

use std::path::Path;

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::asciicast::EventType;
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
    /// Single-pass approach:
    /// - Process terminal output for first ~30 seconds (enough for preview)
    /// - Continue scanning for total duration and marker count
    /// - Never stores all events in memory
    fn load_streaming<P: AsRef<Path>>(path: P) -> Option<Self> {
        use crate::asciicast::{EventType, Header};
        use crate::terminal::TerminalBuffer;
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        let file = File::open(path.as_ref()).ok()?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        // Parse header
        let header_line = lines.next()?.ok()?;
        let header: Header = serde_json::from_str(&header_line).ok()?;
        if header.version != 3 {
            return None;
        }

        // Get terminal size
        let cols = header.term.as_ref().and_then(|t| t.cols).unwrap_or(80) as usize;
        let rows = header.term.as_ref().and_then(|t| t.rows).unwrap_or(24) as usize;

        let mut buffer = TerminalBuffer::new(cols, rows);
        let mut total_duration = 0.0;
        let mut marker_count = 0;
        let mut preview_captured = false;
        let mut styled_preview = Vec::new();

        // Single pass: process events
        // - Process terminal output until 30 seconds (capture preview snapshot)
        // - Continue counting markers and duration
        const PREVIEW_THRESHOLD_SECS: f64 = 30.0;

        for line_result in lines {
            let line = match line_result {
                Ok(l) if !l.trim().is_empty() => l,
                _ => continue,
            };

            // Quick parse for time, type, and optionally data
            if let Some((time, event_type, data)) = Self::parse_event_minimal(&line) {
                total_duration += time;

                if event_type == EventType::Marker {
                    marker_count += 1;
                }

                // Only process terminal output before threshold
                if !preview_captured {
                    if event_type == EventType::Output {
                        if let Some(output) = data {
                            buffer.process(&output, None);
                        }
                    }

                    // Capture preview at threshold
                    if total_duration >= PREVIEW_THRESHOLD_SECS {
                        styled_preview = buffer.styled_lines();
                        preview_captured = true;
                        // Don't need buffer anymore, drop it
                    }
                }
            }
        }

        // If file was shorter than threshold, capture final state
        if !preview_captured {
            styled_preview = buffer.styled_lines();
        }

        Some(Self {
            duration_secs: total_duration,
            marker_count,
            styled_preview,
        })
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
            if Some(cell.style) == current_style {
                current_text.push(cell.char);
            } else {
                // Flush previous span
                if !current_text.is_empty() {
                    if let Some(style) = current_style {
                        let mut ratatui_style = Style::default();
                        if style.fg != Color::Default {
                            ratatui_style = ratatui_style.fg(Self::to_ratatui_color(style.fg));
                        }
                        if style.bg != Color::Default {
                            ratatui_style = ratatui_style.bg(Self::to_ratatui_color(style.bg));
                        }
                        if style.bold {
                            ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
                        }
                        if style.dim {
                            ratatui_style = ratatui_style.add_modifier(Modifier::DIM);
                        }
                        if style.italic {
                            ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
                        }
                        if style.underline {
                            ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
                        }
                        spans.push(Span::styled(
                            std::mem::take(&mut current_text),
                            ratatui_style,
                        ));
                    } else {
                        spans.push(Span::raw(std::mem::take(&mut current_text)));
                    }
                }
                current_style = Some(cell.style);
                current_text.push(cell.char);
            }
        }

        // Flush final span
        if !current_text.is_empty() {
            if let Some(style) = current_style {
                let mut ratatui_style = Style::default();
                if style.fg != Color::Default {
                    ratatui_style = ratatui_style.fg(Self::to_ratatui_color(style.fg));
                }
                if style.bg != Color::Default {
                    ratatui_style = ratatui_style.bg(Self::to_ratatui_color(style.bg));
                }
                if style.bold {
                    ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
                }
                if style.dim {
                    ratatui_style = ratatui_style.add_modifier(Modifier::DIM);
                }
                if style.italic {
                    ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
                }
                if style.underline {
                    ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
                }
                spans.push(Span::styled(current_text, ratatui_style));
            } else {
                spans.push(Span::raw(current_text));
            }
        }

        Line::from(spans)
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

/// Prefetch previews for the current, previous, and next items.
///
/// Collects up to 3 paths (current selection, previous with wrap,
/// next with wrap) and submits them to the cache for background loading.
/// Extracted from `list_app.rs` and `cleanup_app.rs` which had identical logic.
pub fn prefetch_adjacent_previews(
    explorer: &super::FileExplorer,
    cache: &mut crate::tui::preview_cache::PreviewCache,
) {
    let selected = explorer.selected();
    let len = explorer.len();
    if len == 0 {
        return;
    }

    // Collect paths to prefetch (current, prev, next)
    let mut paths_to_prefetch = Vec::with_capacity(3);

    // Current selection
    if let Some(item) = explorer.selected_item() {
        paths_to_prefetch.push(item.path.clone());
    }

    // Previous item (with wrap)
    let prev_idx = if selected > 0 { selected - 1 } else { len - 1 };
    if let Some((_, item, _)) = explorer.visible_items().nth(prev_idx) {
        paths_to_prefetch.push(item.path.clone());
    }

    // Next item (with wrap)
    let next_idx = if selected < len - 1 { selected + 1 } else { 0 };
    if let Some((_, item, _)) = explorer.visible_items().nth(next_idx) {
        paths_to_prefetch.push(item.path.clone());
    }

    // Request prefetch for all
    cache.prefetch(&paths_to_prefetch);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_preview_format_duration_seconds() {
        let preview = SessionPreview {
            duration_secs: 45.0,
            marker_count: 0,
            styled_preview: Vec::new(),
        };
        assert_eq!(preview.format_duration(), "45s");
    }

    #[test]
    fn session_preview_format_duration_minutes() {
        let preview = SessionPreview {
            duration_secs: 332.0, // 5m 32s
            marker_count: 0,
            styled_preview: Vec::new(),
        };
        assert_eq!(preview.format_duration(), "5m 32s");
    }

    #[test]
    fn session_preview_format_duration_hours() {
        let preview = SessionPreview {
            duration_secs: 3732.0, // 1h 2m 12s
            marker_count: 0,
            styled_preview: Vec::new(),
        };
        assert_eq!(preview.format_duration(), "1h 2m 12s");
    }
}
