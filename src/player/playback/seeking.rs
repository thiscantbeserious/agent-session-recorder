//! Seeking operations for the native player.
//!
//! Handles seeking to specific times in the recording, including
//! rebuilding the terminal buffer state.

use crate::asciicast::AsciicastFile;
use crate::terminal::TerminalBuffer;

/// Find the event index and cumulative time at a given target time.
///
/// This is used when seeking to determine which event to resume playback from.
///
/// # Arguments
/// * `cast` - The parsed asciicast file
/// * `target_time` - The time to seek to (in seconds)
///
/// # Returns
/// A tuple of (event_index, cumulative_time_before_that_event)
pub fn find_event_index_at_time(cast: &AsciicastFile, target_time: f64) -> (usize, f64) {
    let mut cumulative = 0.0f64;
    for (i, event) in cast.events.iter().enumerate() {
        let next_cumulative = cumulative + event.time;
        if next_cumulative > target_time {
            return (i, cumulative);
        }
        cumulative = next_cumulative;
    }
    (cast.events.len(), cumulative)
}

/// Seek to a specific time by re-rendering the buffer from scratch.
///
/// This clears the terminal buffer and replays all events up to the target time.
/// This is necessary because terminal state depends on all previous output.
///
/// # Arguments
/// * `buffer` - The terminal buffer to update
/// * `cast` - The parsed asciicast file
/// * `target_time` - The time to seek to (in seconds)
/// * `cols` - Recording width (for buffer reset)
/// * `rows` - Recording height (for buffer reset)
pub fn seek_to_time(
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
            buffer.process(&event.data, None);
        } else if let Some((new_cols, new_rows)) = event.parse_resize() {
            buffer.resize(new_cols as usize, new_rows as usize);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asciicast::{Event, EventType, Header};

    fn make_header() -> Header {
        Header {
            version: 3,
            width: Some(80),
            height: Some(24),
            term: None,
            timestamp: None,
            duration: None,
            title: None,
            command: None,
            env: None,
            idle_time_limit: None,
        }
    }

    fn make_cast(event_times: &[f64]) -> AsciicastFile {
        let events: Vec<Event> = event_times
            .iter()
            .map(|&t| Event {
                time: t,
                event_type: EventType::Output,
                data: "x".to_string(),
            })
            .collect();
        AsciicastFile {
            header: make_header(),
            events,
        }
    }

    #[test]
    fn empty_cast_returns_zero_index() {
        let cast = make_cast(&[]);
        let (idx, cumulative) = find_event_index_at_time(&cast, 5.0);
        assert_eq!(idx, 0);
        assert_eq!(cumulative, 0.0);
    }

    #[test]
    fn target_before_first_event() {
        let cast = make_cast(&[1.0, 1.0, 1.0]); // Events at t=1, t=2, t=3
        let (idx, cumulative) = find_event_index_at_time(&cast, 0.5);
        assert_eq!(idx, 0);
        assert_eq!(cumulative, 0.0);
    }

    #[test]
    fn target_at_first_event() {
        let cast = make_cast(&[1.0, 1.0, 1.0]);
        let (idx, cumulative) = find_event_index_at_time(&cast, 1.0);
        assert_eq!(idx, 1); // After first event
        assert_eq!(cumulative, 1.0);
    }

    #[test]
    fn target_between_events() {
        let cast = make_cast(&[1.0, 1.0, 1.0]); // Events at t=1, t=2, t=3
        let (idx, cumulative) = find_event_index_at_time(&cast, 1.5);
        assert_eq!(idx, 1); // At event index 1 (second event)
        assert_eq!(cumulative, 1.0);
    }

    #[test]
    fn target_at_last_event() {
        let cast = make_cast(&[1.0, 1.0, 1.0]); // Events at t=1, t=2, t=3
        let (idx, cumulative) = find_event_index_at_time(&cast, 3.0);
        assert_eq!(idx, 3); // Past all events
        assert_eq!(cumulative, 3.0);
    }

    #[test]
    fn target_past_all_events() {
        let cast = make_cast(&[1.0, 1.0, 1.0]); // Events at t=1, t=2, t=3
        let (idx, cumulative) = find_event_index_at_time(&cast, 10.0);
        assert_eq!(idx, 3); // All events processed
        assert_eq!(cumulative, 3.0);
    }

    #[test]
    fn seek_to_zero_clears_buffer() {
        let cast = AsciicastFile {
            header: make_header(),
            events: vec![Event {
                time: 1.0,
                event_type: EventType::Output,
                data: "hello".to_string(),
            }],
        };
        let mut buffer = TerminalBuffer::new(80, 24);
        buffer.process("some content", None);

        seek_to_time(&mut buffer, &cast, 0.0, 80, 24);

        // Buffer should be cleared (no content at 0.0)
        let row = buffer.row(0).unwrap();
        assert!(row.iter().all(|c| c.char == ' '));
    }

    #[test]
    fn seek_to_after_event_includes_output() {
        let cast = AsciicastFile {
            header: make_header(),
            events: vec![Event {
                time: 1.0,
                event_type: EventType::Output,
                data: "hello".to_string(),
            }],
        };
        let mut buffer = TerminalBuffer::new(80, 24);

        seek_to_time(&mut buffer, &cast, 2.0, 80, 24);

        // Buffer should contain "hello"
        let row = buffer.row(0).unwrap();
        let content: String = row.iter().take(5).map(|c| c.char).collect();
        assert_eq!(content, "hello");
    }

    #[test]
    fn seek_skips_markers() {
        let cast = AsciicastFile {
            header: make_header(),
            events: vec![
                Event {
                    time: 1.0,
                    event_type: EventType::Marker,
                    data: "marker".to_string(),
                },
                Event {
                    time: 1.0,
                    event_type: EventType::Output,
                    data: "text".to_string(),
                },
            ],
        };
        let mut buffer = TerminalBuffer::new(80, 24);

        seek_to_time(&mut buffer, &cast, 3.0, 80, 24);

        // Buffer should contain "text" (marker data not rendered)
        let row = buffer.row(0).unwrap();
        let content: String = row.iter().take(4).map(|c| c.char).collect();
        assert_eq!(content, "text");
    }
}
