//! Progress line deduplication transform.
//!
//! Terminal progress bars often use carriage return (`\r`) to rewrite the same
//! line thousands of times. This transform keeps only the final state of each
//! line, dramatically reducing content size while preserving meaning.

use crate::asciicast::{Event, Transform};

/// Deduplicates progress lines that use `\r` to overwrite themselves.
pub struct DeduplicateProgressLines {
    current_line: String,
    pending_cr: bool,
    accumulated_time: f64,
    deduped_count: usize,
}

impl DeduplicateProgressLines {
    pub fn new() -> Self {
        Self {
            current_line: String::new(),
            pending_cr: false,
            accumulated_time: 0.0,
            deduped_count: 0,
        }
    }

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

        for event in events.drain(..) {
            if !event.is_output() {
                // Flash pending CR as overwrite if followed by non-output
                if self.pending_cr {
                    self.current_line.clear();
                    self.pending_cr = false;
                }

                let mut marker = event;
                marker.time += self.accumulated_time;
                self.accumulated_time = 0.0;

                if !self.current_line.is_empty() {
                    output_events.push(Event::output(
                        marker.time,
                        std::mem::take(&mut self.current_line),
                    ));
                    marker.time = 0.0;
                }

                output_events.push(marker);
                continue;
            }

            self.accumulated_time += event.time;

            for ch in event.data.chars() {
                if self.pending_cr {
                    self.pending_cr = false;
                    if ch == '\n' {
                        let data = if !self.current_line.is_empty() {
                            format!("{}\n", std::mem::take(&mut self.current_line))
                        } else {
                            "\n".to_string()
                        };
                        output_events.push(Event::output(self.accumulated_time, data));
                        self.accumulated_time = 0.0;
                        continue;
                    } else {
                        if !self.current_line.is_empty() {
                            self.deduped_count += 1;
                        }
                        self.current_line.clear();
                    }
                }

                match ch {
                    '\r' => {
                        self.pending_cr = true;
                    }
                    '\n' => {
                        let data = if !self.current_line.is_empty() {
                            format!("{}\n", std::mem::take(&mut self.current_line))
                        } else {
                            "\n".to_string()
                        };
                        output_events.push(Event::output(self.accumulated_time, data));
                        self.accumulated_time = 0.0;
                    }
                    _ => {
                        self.current_line.push(ch);
                    }
                }
            }
        }

        if !self.current_line.is_empty() {
            output_events.push(Event::output(
                self.accumulated_time,
                std::mem::take(&mut self.current_line),
            ));
            self.accumulated_time = 0.0;
        }

        // Final trailing time delta if any
        if self.accumulated_time > 0.0 {
            if let Some(last) = output_events.last_mut() {
                last.time += self.accumulated_time;
            } else {
                // If the whole session was emptied, we can't really preserve the final gap easily
                // but this shouldn't happen in a valid recording
            }
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
        assert_eq!(events.len(), 1);
        assert!(events[0].data.contains("Build complete"));
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
        assert_eq!(events.len(), 3);
        let total_time: f64 = events.iter().map(|e| e.time).sum();
        assert!((total_time - 1.0).abs() < 0.001);
    }

    #[test]
    fn carries_time_from_collapsed_progress() {
        let mut deduper = DeduplicateProgressLines::new();
        let mut events = vec![
            Event::output(0.5, "\rframe1"),
            Event::output(0.5, "\rframe2"),
            Event::output(0.5, "Done\n"),
        ];
        deduper.transform(&mut events);
        assert_eq!(events.len(), 1);
        let total_time: f64 = events.iter().map(|e| e.time).sum();
        assert!((total_time - 1.5).abs() < 0.001);
    }
}
