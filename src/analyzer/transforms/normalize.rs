//! Whitespace normalization and empty event filtering transforms.
//!
//! These transforms reduce noise from excessive whitespace and empty events.

use crate::asciicast::{Event, Transform};

/// Normalizes excessive whitespace in event content.
///
/// - Collapses multiple consecutive spaces to a single space
/// - Limits consecutive newlines to a configurable maximum
pub struct NormalizeWhitespace {
    max_consecutive_newlines: usize,
}

impl NormalizeWhitespace {
    /// Create a new whitespace normalizer.
    pub fn new(max_consecutive_newlines: usize) -> Self {
        Self {
            max_consecutive_newlines,
        }
    }
}

impl Default for NormalizeWhitespace {
    fn default() -> Self {
        Self::new(2)
    }
}

impl Transform for NormalizeWhitespace {
    fn transform(&mut self, events: &mut Vec<Event>) {
        for event in events.iter_mut() {
            if event.is_output() {
                let mut result = String::with_capacity(event.data.len());
                let mut newline_count = 0;

                for c in event.data.chars() {
                    if c == '\n' {
                        newline_count += 1;
                        if newline_count <= self.max_consecutive_newlines {
                            result.push(c);
                        }
                    } else {
                        newline_count = 0;
                        result.push(c);
                    }
                }
                event.data = result;
            }
        }
    }
}

/// Filters out events with no content.
///
/// Removes output events that are empty or contain only whitespace.
/// **Always preserves**: markers, input events, resize events.
///
/// **Important**: When removing events, their time deltas are accumulated
/// and added to the next kept event to preserve timeline integrity.
pub struct FilterEmptyEvents;

impl Transform for FilterEmptyEvents {
    fn transform(&mut self, events: &mut Vec<Event>) {
        let mut accumulated_time = 0.0;
        let mut output = Vec::with_capacity(events.len());

        for mut event in events.drain(..) {
            // Always keep non-output events (markers, input, resize)
            if !event.is_output() {
                // Add accumulated time to this event
                event.time += accumulated_time;
                accumulated_time = 0.0;
                output.push(event);
                continue;
            }

            // Keep output events if they have non-whitespace content OR contain spaces
            // (TUI often sends spaces in separate events which we must preserve)
            if !event.data.trim().is_empty() || event.data.contains(' ') {
                // Add accumulated time from removed events
                event.time += accumulated_time;
                accumulated_time = 0.0;
                output.push(event);
            } else {
                // Accumulate time from removed event
                accumulated_time += event.time;
            }
        }

        *events = output;
    }
}

/// Collapses consecutive empty lines within and across events.
pub struct EmptyLineFilter {
    last_line_was_empty: bool,
}

impl EmptyLineFilter {
    pub fn new() -> Self {
        Self {
            last_line_was_empty: false,
        }
    }
}

impl Default for EmptyLineFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl Transform for EmptyLineFilter {
    fn transform(&mut self, events: &mut Vec<Event>) {
        let mut accumulated_time = 0.0;
        let mut output = Vec::with_capacity(events.len());

        for mut event in events.drain(..) {
            if !event.is_output() {
                event.time += accumulated_time;
                accumulated_time = 0.0;
                output.push(event);
                continue;
            }

            let mut new_data = String::with_capacity(event.data.len());
            for line in event.data.split_inclusive('\n') {
                // A line is truly empty if it only contains \n or \r\n
                let is_empty = line == "\n" || line == "\r\n";

                if is_empty && self.last_line_was_empty {
                    // Skip consecutive truly empty line
                    continue;
                }

                new_data.push_str(line);
                self.last_line_was_empty = is_empty;
            }

            if !new_data.is_empty() {
                event.data = new_data;
                event.time += accumulated_time;
                accumulated_time = 0.0;
                output.push(event);
            } else {
                accumulated_time += event.time;
            }
        }

        if accumulated_time > 0.0 {
            if let Some(last) = output.last_mut() {
                last.time += accumulated_time;
            }
        }

        *events = output;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // NormalizeWhitespace tests

    #[test]
    fn preserves_spaces_and_tabs() {
        let mut normalizer = NormalizeWhitespace::new(2);
        let mut events = vec![Event::output(0.1, "hello    world")];

        normalizer.transform(&mut events);

        // NormalizeWhitespace only limits consecutive newlines, not spaces
        assert_eq!(events[0].data, "hello    world");
    }

    #[test]
    fn limits_consecutive_newlines() {
        let mut normalizer = NormalizeWhitespace::new(2);
        let mut events = vec![Event::output(0.1, "line1\n\n\n\n\nline2")];

        normalizer.transform(&mut events);

        assert_eq!(events[0].data, "line1\n\nline2");
    }

    #[test]
    fn preserves_tabs() {
        let mut normalizer = NormalizeWhitespace::new(2);
        let mut events = vec![Event::output(0.1, "hello\t\tworld")];

        normalizer.transform(&mut events);

        // Tabs are preserved (only consecutive newlines are limited)
        assert_eq!(events[0].data, "hello\t\tworld");
    }

    // FilterEmptyEvents tests

    #[test]
    fn removes_empty_events() {
        let mut events = vec![
            Event::output(0.1, "hello"),
            Event::output(0.1, ""),
            Event::output(0.1, "world"),
        ];

        FilterEmptyEvents.transform(&mut events);

        assert_eq!(events.len(), 2);
    }

    #[test]
    fn removes_whitespace_only_events() {
        let mut events = vec![
            Event::output(0.1, "hello"),
            Event::output(0.1, "   "),  // preserved (contains spaces)
            Event::output(0.1, "\t\n"), // removed (no spaces)
            Event::output(0.1, "world"),
        ];

        FilterEmptyEvents.transform(&mut events);

        // hello, "   ", and world are kept
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn preserves_markers() {
        let mut events = vec![
            Event::output(0.1, ""),
            Event::marker(0.1, "marker"),
            Event::output(0.1, ""),
        ];

        FilterEmptyEvents.transform(&mut events);

        assert_eq!(events.len(), 1);
        assert!(events[0].is_marker());
    }

    #[test]
    fn accumulates_time_from_removed_events() {
        let mut events = vec![
            Event::output(10.0, "content1"),
            Event::output(5.0, ""), // empty - removed, but 5.0 should be accumulated
            Event::output(3.0, "content2"),
        ];

        FilterEmptyEvents.transform(&mut events);

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "content1");
        assert!((events[0].time - 10.0).abs() < 0.001);
        assert_eq!(events[1].data, "content2");
        // Second event should have 5.0 + 3.0 = 8.0 time delta
        assert!(
            (events[1].time - 8.0).abs() < 0.001,
            "Expected 8.0, got {}",
            events[1].time
        );
    }

    #[test]
    fn accumulates_time_across_multiple_removed_events() {
        let mut events = vec![
            Event::output(1.0, "start"),
            Event::output(2.0, ""),     // removed (empty)
            Event::output(3.0, " "),    // preserved (space)
            Event::output(4.0, "\t\n"), // removed (no space)
            Event::output(5.0, "end"),
        ];

        FilterEmptyEvents.transform(&mut events);

        // start, " ", end are kept
        assert_eq!(events.len(), 3);
        assert!((events[0].time - 1.0).abs() < 0.001);
        // Second event (" "): accumulated 2.0 from previous empty
        assert!((events[1].time - 5.0).abs() < 0.001);
        // Third event ("end"): accumulated 4.0 from previous tab/nl
        assert!((events[2].time - 9.0).abs() < 0.001);
    }

    #[test]
    fn accumulated_time_passes_to_marker() {
        let mut events = vec![
            Event::output(1.0, "content"),
            Event::output(5.0, ""), // removed
            Event::marker(2.0, "test"),
        ];

        FilterEmptyEvents.transform(&mut events);

        assert_eq!(events.len(), 2);
        // Marker should have 5.0 + 2.0 = 7.0 time delta
        assert!(
            (events[1].time - 7.0).abs() < 0.001,
            "Expected 7.0, got {}",
            events[1].time
        );
    }
}
