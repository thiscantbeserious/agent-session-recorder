//! Marker collection and navigation for the native player.
//!
//! Markers are special events in the cast file that can be used
//! to navigate to specific points in the recording.

use crate::asciicast::AsciicastFile;
use crate::player::state::MarkerPosition;

/// Collect markers from the cast file with their cumulative times.
///
/// Iterates through all events and extracts markers, calculating
/// their cumulative time position in the recording.
///
/// # Arguments
/// * `cast` - The parsed asciicast file
///
/// # Returns
/// A vector of `MarkerPosition` structs sorted by time
pub fn collect_markers(cast: &AsciicastFile) -> Vec<MarkerPosition> {
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
            ..Default::default()
        }
    }

    #[test]
    fn empty_cast_returns_no_markers() {
        let cast = AsciicastFile {
            header: make_header(),
            events: vec![],
        };
        let markers = collect_markers(&cast);
        assert!(markers.is_empty());
    }

    #[test]
    fn cast_with_only_output_returns_no_markers() {
        let cast = AsciicastFile {
            header: make_header(),
            events: vec![
                Event {
                    time: 1.0,
                    event_type: EventType::Output,
                    data: "hello".to_string(),
                },
                Event {
                    time: 1.0,
                    event_type: EventType::Output,
                    data: "world".to_string(),
                },
            ],
        };
        let markers = collect_markers(&cast);
        assert!(markers.is_empty());
    }

    #[test]
    fn cast_with_markers_collects_them() {
        let cast = AsciicastFile {
            header: make_header(),
            events: vec![
                Event {
                    time: 1.0,
                    event_type: EventType::Output,
                    data: "hello".to_string(),
                },
                Event {
                    time: 1.0,
                    event_type: EventType::Marker,
                    data: "marker1".to_string(),
                },
                Event {
                    time: 2.0,
                    event_type: EventType::Output,
                    data: "world".to_string(),
                },
                Event {
                    time: 1.0,
                    event_type: EventType::Marker,
                    data: "marker2".to_string(),
                },
            ],
        };
        let markers = collect_markers(&cast);
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].time, 2.0); // 1.0 + 1.0
        assert_eq!(markers[0].label, "marker1");
        assert_eq!(markers[1].time, 5.0); // 1.0 + 1.0 + 2.0 + 1.0
        assert_eq!(markers[1].label, "marker2");
    }

    #[test]
    fn marker_at_start() {
        let cast = AsciicastFile {
            header: make_header(),
            events: vec![
                Event {
                    time: 0.0,
                    event_type: EventType::Marker,
                    data: "start".to_string(),
                },
                Event {
                    time: 1.0,
                    event_type: EventType::Output,
                    data: "output".to_string(),
                },
            ],
        };
        let markers = collect_markers(&cast);
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].time, 0.0);
        assert_eq!(markers[0].label, "start");
    }
}
