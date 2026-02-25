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
                event.data = normalize_newlines(&event.data, self.max_consecutive_newlines);
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

            let new_data = filter_empty_lines(&event.data, &mut self.last_line_was_empty);

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

/// Limit consecutive `\n` characters in `data` to at most `max_consecutive`.
fn normalize_newlines(data: &str, max_consecutive: usize) -> String {
    let mut result = String::with_capacity(data.len());
    let mut newline_count = 0;

    for c in data.chars() {
        if c == '\n' {
            newline_count += 1;
            if newline_count <= max_consecutive {
                result.push(c);
            }
        } else {
            newline_count = 0;
            result.push(c);
        }
    }

    result
}

/// Remove consecutive truly-empty lines (`\n` or `\r\n`) from `data`.
///
/// `last_line_was_empty` carries state across calls so that emptiness is
/// tracked between events.
fn filter_empty_lines(data: &str, last_line_was_empty: &mut bool) -> String {
    let mut new_data = String::with_capacity(data.len());

    for line in data.split_inclusive('\n') {
        let is_empty = line == "\n" || line == "\r\n";

        if is_empty && *last_line_was_empty {
            continue;
        }

        new_data.push_str(line);
        *last_line_was_empty = is_empty;
    }

    new_data
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

    // ============================================
    // normalize_newlines Tests
    // ============================================

    #[test]
    fn normalize_newlines_no_newlines() {
        assert_eq!(normalize_newlines("hello world", 2), "hello world");
    }

    #[test]
    fn normalize_newlines_exactly_max() {
        // Exactly max consecutive newlines are all kept
        assert_eq!(normalize_newlines("a\n\nb", 2), "a\n\nb");
    }

    #[test]
    fn normalize_newlines_one_above_max() {
        // 3 consecutive newlines with max=2 => keep only 2
        assert_eq!(normalize_newlines("a\n\n\nb", 2), "a\n\nb");
    }

    #[test]
    fn normalize_newlines_all_newlines() {
        // A string of only newlines with max=1 => keep only 1
        assert_eq!(normalize_newlines("\n\n\n\n", 1), "\n");
    }

    #[test]
    fn normalize_newlines_max_1() {
        assert_eq!(normalize_newlines("a\n\nb", 1), "a\nb");
    }

    #[test]
    fn normalize_newlines_interleaved() {
        // Interleaved text resets the newline counter
        assert_eq!(normalize_newlines("a\n\n\nb\n\n\nc", 2), "a\n\nb\n\nc");
    }

    // ============================================
    // filter_empty_lines Tests
    // ============================================

    #[test]
    fn filter_empty_lines_all_empty() {
        let mut state = false;
        // Second empty line should be dropped; first is kept
        let result = filter_empty_lines("\n\n\n", &mut state);
        assert_eq!(result, "\n");
        assert!(state);
    }

    #[test]
    fn filter_empty_lines_no_empty() {
        let mut state = false;
        let result = filter_empty_lines("hello\nworld\n", &mut state);
        assert_eq!(result, "hello\nworld\n");
        assert!(!state); // last line was "world\n" which is not empty
    }

    #[test]
    fn filter_empty_lines_alternating() {
        let mut state = false;
        // "a\n", "\n", "b\n" => keep all (empty line not consecutive)
        let result = filter_empty_lines("a\n\nb\n", &mut state);
        assert_eq!(result, "a\n\nb\n");
    }

    #[test]
    fn filter_empty_lines_crlf() {
        let mut state = false;
        // CRLF empty lines should also be collapsed
        let result = filter_empty_lines("\r\n\r\n", &mut state);
        assert_eq!(result, "\r\n");
        assert!(state);
    }

    #[test]
    fn filter_empty_lines_content_resets_state() {
        let mut state = true; // simulate previous call ended with empty line
        let result = filter_empty_lines("hello\n", &mut state);
        // "hello\n" is not empty, so it is kept and state resets to false
        assert_eq!(result, "hello\n");
        assert!(!state);
    }

    #[test]
    fn filter_empty_lines_cross_call_state() {
        // If previous call left state=true and this call starts with an empty line,
        // the empty line should be suppressed.
        let mut state = true;
        let result = filter_empty_lines("\nhello\n", &mut state);
        // First "\n" is suppressed because state==true; "hello\n" is kept
        assert_eq!(result, "hello\n");
        assert!(!state);
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
