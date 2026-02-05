//! Progress line deduplication transform.
//!
//! Terminal progress bars often use carriage return (`\r`) to rewrite the same
//! line thousands of times. This transform keeps only the final state of each
//! line, dramatically reducing content size while preserving meaning.

use crate::asciicast::{Event, Transform};

/// Deduplicates progress lines that use `\r` to overwrite themselves.
///
/// **Algorithm**:
/// 1. Track "current line buffer" with timestamp of FIRST char
/// 2. When `\r` is followed by content (not `\n`), clear buffer - it's a progress overwrite
/// 3. When `\r\n` is encountered, treat it as a normal line ending (preserve content)
/// 4. When `\n` is encountered, emit the line with relative timestamp
/// 5. Non-output events (markers, input) pass through unchanged
///
/// **Important**: Output events use RELATIVE timestamps (time since previous event),
/// not absolute timestamps.
pub struct DeduplicateProgressLines {
    current_line: String,
    line_start_time: f64,
    pending_cr: bool, // Track if we have a pending \r that might be \r\n
    is_progress_line: bool,
    deduped_count: usize,
}

impl DeduplicateProgressLines {
    /// Create a new progress line deduplicator.
    pub fn new() -> Self {
        Self {
            current_line: String::new(),
            line_start_time: 0.0,
            pending_cr: false,
            is_progress_line: false,
            deduped_count: 0,
        }
    }

    /// Get the count of deduplicated progress lines.
    pub fn deduped_count(&self) -> usize {
        self.deduped_count
    }
}

impl Default for DeduplicateProgressLines {
    fn default() -> Self {
        Self::new()
    }
}

impl Transform for DeduplicateProgressLines {
    fn transform(&mut self, events: &mut Vec<Event>) {
        let mut output_events = Vec::with_capacity(events.len());

        // Track cumulative time for computing relative timestamps
        let mut cumulative_time = 0.0;
        let mut last_emit_time = 0.0;

        for event in events.drain(..) {
            cumulative_time += event.time;

            // Preserve non-output events (markers, input, resize)
            if !event.is_output() {
                // Handle any pending CR before marker
                if self.pending_cr {
                    // Pending CR with no following char - treat as progress overwrite
                    self.is_progress_line = true;
                    self.current_line.clear();
                    self.pending_cr = false;
                }
                // Emit any pending line content before the marker
                if !self.current_line.is_empty() {
                    let relative_time = self.line_start_time - last_emit_time;
                    output_events.push(Event::output(
                        relative_time.max(0.0),
                        std::mem::take(&mut self.current_line),
                    ));
                    last_emit_time = self.line_start_time;
                }
                // Emit marker with relative time
                let relative_time = cumulative_time - last_emit_time;
                let mut marker_event = event;
                marker_event.time = relative_time.max(0.0);
                output_events.push(marker_event);
                last_emit_time = cumulative_time;
                continue;
            }

            for ch in event.data.chars() {
                // First, resolve any pending CR
                if self.pending_cr {
                    self.pending_cr = false;
                    if ch == '\n' {
                        // \r\n is a normal line ending - emit the line with \n
                        if !self.current_line.is_empty() {
                            let relative_time = self.line_start_time - last_emit_time;
                            output_events.push(Event::output(
                                relative_time.max(0.0),
                                format!("{}\n", self.current_line),
                            ));
                            last_emit_time = self.line_start_time;
                        } else {
                            // Emit standalone newline
                            let relative_time = cumulative_time - last_emit_time;
                            output_events.push(Event::output(relative_time.max(0.0), "\n".to_string()));
                            last_emit_time = cumulative_time;
                        }
                        self.current_line.clear();
                        self.is_progress_line = false;
                        continue;
                    } else {
                        // \r followed by content = progress line overwrite
                        self.is_progress_line = true;
                        if self.is_progress_line && !self.current_line.is_empty() {
                            self.deduped_count += 1;
                        }
                        self.current_line.clear();
                        self.line_start_time = cumulative_time;
                    }
                }

                match ch {
                    '\r' => {
                        // Mark CR as pending - we need to see next char
                        self.pending_cr = true;
                    }
                    '\n' => {
                        // Standalone \n (not preceded by \r)
                        if !self.current_line.is_empty() {
                            let relative_time = self.line_start_time - last_emit_time;
                            output_events.push(Event::output(
                                relative_time.max(0.0),
                                format!("{}\n", self.current_line),
                            ));
                            last_emit_time = self.line_start_time;
                        } else {
                            // Emit standalone newline
                            let relative_time = cumulative_time - last_emit_time;
                            output_events.push(Event::output(relative_time.max(0.0), "\n".to_string()));
                            last_emit_time = cumulative_time;
                        }
                        self.current_line.clear();
                        self.is_progress_line = false;
                    }
                    _ => {
                        // First char of new line sets the timestamp
                        if self.current_line.is_empty() {
                            self.line_start_time = cumulative_time;
                        }
                        self.current_line.push(ch);
                    }
                }
            }
        }

        // Handle any pending CR at end
        if self.pending_cr {
            // Pending CR with no following char - treat as progress overwrite
            self.is_progress_line = true;
            self.current_line.clear();
            self.pending_cr = false;
        }

        // Don't forget trailing content without \n
        if !self.current_line.is_empty() {
            let relative_time = self.line_start_time - last_emit_time;
            output_events.push(Event::output(
                relative_time.max(0.0),
                std::mem::take(&mut self.current_line),
            ));
        }

        *events = output_events;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_cr_lines() {
        let mut deduper = DeduplicateProgressLines::new();
        let mut events = vec![
            Event::output(0.1, "\r⠋ Building..."),
            Event::output(0.1, "\r⠙ Building..."),
            Event::output(0.1, "\r⠹ Building..."),
            Event::output(0.1, "\r✓ Build complete\n"),
        ];

        deduper.transform(&mut events);

        // Should have one event with final content
        assert_eq!(events.len(), 1);
        assert!(events[0].data.contains("Build complete"));
    }

    #[test]
    fn preserves_markers() {
        let mut deduper = DeduplicateProgressLines::new();
        let mut events = vec![
            Event::output(0.1, "line1\n"),
            Event::marker(0.1, "marker"),
            Event::output(0.1, "line2\n"),
        ];

        deduper.transform(&mut events);

        // Marker should be preserved in order
        let markers: Vec<_> = events.iter().filter(|e| e.is_marker()).collect();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].data, "marker");
    }

    #[test]
    fn preserves_non_progress_lines() {
        let mut deduper = DeduplicateProgressLines::new();
        let mut events = vec![
            Event::output(0.1, "first line\n"),
            Event::output(0.1, "second line\n"),
            Event::output(0.1, "third line\n"),
        ];

        deduper.transform(&mut events);

        // All three lines should be preserved
        let content: String = events.iter().map(|e| e.data.as_str()).collect();
        assert!(content.contains("first line"));
        assert!(content.contains("second line"));
        assert!(content.contains("third line"));
    }

    #[test]
    fn preserves_crlf_line_endings() {
        // \r\n is a standard line ending (Windows-style), not a progress overwrite
        let mut deduper = DeduplicateProgressLines::new();
        let mut events = vec![
            Event::output(0.5, "$ echo hello\r\n"),
            Event::output(0.1, "hello\r\n"),
            Event::output(0.2, "$ "),
        ];

        deduper.transform(&mut events);

        // All content should be preserved
        let content: String = events.iter().map(|e| e.data.as_str()).collect();
        assert!(content.contains("$ echo hello"), "Command should be preserved");
        assert!(content.contains("hello"), "Output should be preserved");
        assert!(content.contains("$ "), "Prompt should be preserved");
    }

    #[test]
    fn uses_relative_timestamps() {
        let mut deduper = DeduplicateProgressLines::new();
        let mut events = vec![
            Event::output(0.5, "line1\n"),
            Event::output(0.3, "line2\n"),
            Event::output(0.2, "line3\n"),
        ];

        deduper.transform(&mut events);

        // Timestamps should remain relative (not cumulative)
        // First event starts at 0.5 (cumulative), relative is 0.5
        // Second event content starts around cumulative 0.8, but its line starts at 0.8
        // The relative time should reflect gaps between emissions
        assert_eq!(events.len(), 3);
        // Total time should add up to ~1.0
        let total_time: f64 = events.iter().map(|e| e.time).sum();
        assert!(
            (total_time - 1.0).abs() < 0.01,
            "Total time should be ~1.0, got {}",
            total_time
        );
    }
}
