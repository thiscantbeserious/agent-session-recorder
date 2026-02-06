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
                let mut prev_space = false;
                let mut newline_count = 0;

                for c in event.data.chars() {
                    if c == '\n' {
                        newline_count += 1;
                        if newline_count <= self.max_consecutive_newlines {
                            result.push(c);
                        }
                        prev_space = false;
                    } else if c == ' ' || c == '\t' {
                        newline_count = 0;
                        if !prev_space {
                            result.push(' ');
                            prev_space = true;
                        }
                    } else {
                        newline_count = 0;
                        prev_space = false;
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

            // Keep output events only if they have non-whitespace content
            if !event.data.trim().is_empty() {
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

#[cfg(test)]
mod tests {
    use super::*;

    // NormalizeWhitespace tests

    #[test]
    fn collapses_multiple_spaces() {
        let mut normalizer = NormalizeWhitespace::new(2);
        let mut events = vec![Event::output(0.1, "hello    world")];

        normalizer.transform(&mut events);

        assert_eq!(events[0].data, "hello world");
    }

    #[test]
    fn limits_consecutive_newlines() {
        let mut normalizer = NormalizeWhitespace::new(2);
        let mut events = vec![Event::output(0.1, "line1\n\n\n\n\nline2")];

        normalizer.transform(&mut events);

        assert_eq!(events[0].data, "line1\n\nline2");
    }

    #[test]
    fn converts_tabs_to_space() {
        let mut normalizer = NormalizeWhitespace::new(2);
        let mut events = vec![Event::output(0.1, "hello\t\tworld")];

        normalizer.transform(&mut events);

        assert_eq!(events[0].data, "hello world");
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
            Event::output(0.1, "   \n\t  "),
            Event::output(0.1, "world"),
        ];

        FilterEmptyEvents.transform(&mut events);

        assert_eq!(events.len(), 2);
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
            Event::output(2.0, ""),     // removed
            Event::output(3.0, "   "),  // removed
            Event::output(4.0, "\t\n"), // removed
            Event::output(5.0, "end"),
        ];

        FilterEmptyEvents.transform(&mut events);

        assert_eq!(events.len(), 2);
        assert!((events[0].time - 1.0).abs() < 0.001);
        // Second event: 2 + 3 + 4 + 5 = 14
        assert!(
            (events[1].time - 14.0).abs() < 0.001,
            "Expected 14.0, got {}",
            events[1].time
        );
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
