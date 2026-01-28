//! Type definitions for the asciicast v3 format.
//!
//! This module contains all the core types needed to represent asciicast v3
//! terminal recordings. The format uses NDJSON (newline-delimited JSON) with:
//!
//! - A header line containing recording metadata
//! - Event lines representing terminal I/O with relative timestamps
//!
//! # Asciicast v3 Format
//!
//! The asciicast v3 format is designed for efficient streaming of terminal
//! recordings. Each event uses a relative timestamp (time since previous event)
//! rather than absolute timestamps, which simplifies streaming playback.
//!
//! Reference: <https://docs.asciinema.org/manual/asciicast/v3/>
//!
//! # Example
//!
//! ```text
//! {"version":3,"term":{"cols":80,"rows":24}}
//! [0.5,"o","Hello "]
//! [0.3,"o","world!"]
//! [0.1,"m","marker label"]
//! ```

use serde::{Deserialize, Serialize};

// ============================================================================
// Header Types
// ============================================================================

/// Metadata header for an asciicast v3 recording.
///
/// Contains version information, terminal dimensions, and optional metadata
/// like title, command, and environment variables. Only `version` is required.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    /// Format version (must be 3 for v3 format).
    pub version: u8,

    /// Terminal width in columns (deprecated in v3, use `term.cols`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,

    /// Terminal height in rows (deprecated in v3, use `term.rows`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,

    /// Terminal information including dimensions and type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub term: Option<TermInfo>,

    /// Unix timestamp when the recording started.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,

    /// Total duration of the recording in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,

    /// Title of the recording.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Command that was recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// Environment variables captured during recording.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<EnvInfo>,

    /// Maximum idle time between events (for playback speed limiting).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_time_limit: Option<f64>,
}

/// Terminal information embedded in the header.
///
/// Contains the terminal dimensions and type. This is the preferred way to
/// specify dimensions in v3 (over the deprecated `width`/`height` fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermInfo {
    /// Number of columns (width) in the terminal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cols: Option<u32>,

    /// Number of rows (height) in the terminal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<u32>,

    /// Terminal type (e.g., "xterm-256color").
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub term_type: Option<String>,
}

/// Environment variables captured during recording.
///
/// Stores shell and terminal type information that can be useful for playback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvInfo {
    /// The shell used during recording (e.g., "/bin/zsh").
    #[serde(rename = "SHELL", skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,

    /// The TERM environment variable value.
    #[serde(rename = "TERM", skip_serializing_if = "Option::is_none")]
    pub term: Option<String>,
}

// ============================================================================
// Event Types
// ============================================================================

/// Event type codes representing different kinds of terminal events.
///
/// Each variant maps to a single-character code used in the JSON format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    /// Output event ("o"): Data written to the terminal (stdout/stderr).
    Output,

    /// Input event ("i"): Data read from the terminal (user keystrokes).
    Input,

    /// Marker event ("m"): An annotation or bookmark in the recording.
    Marker,

    /// Resize event ("r"): Terminal dimensions changed (format: "COLSxROWS").
    Resize,

    /// Exit event ("x"): Process exit code.
    Exit,
}

impl EventType {
    /// Parse an event type from its single-character code.
    ///
    /// Returns `None` for unrecognized codes.
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "o" => Some(EventType::Output),
            "i" => Some(EventType::Input),
            "m" => Some(EventType::Marker),
            "r" => Some(EventType::Resize),
            "x" => Some(EventType::Exit),
            _ => None,
        }
    }

    /// Convert the event type to its single-character code.
    pub fn to_code(&self) -> &'static str {
        match self {
            EventType::Output => "o",
            EventType::Input => "i",
            EventType::Marker => "m",
            EventType::Resize => "r",
            EventType::Exit => "x",
        }
    }
}

/// A single event in an asciicast recording.
///
/// Events represent terminal I/O operations with relative timestamps.
/// The `time` field indicates seconds since the previous event (not absolute time).
#[derive(Debug, Clone)]
pub struct Event {
    /// Time offset from the previous event in seconds.
    ///
    /// For the first event, this is the time since recording started.
    pub time: f64,

    /// The type of event (output, input, marker, resize, or exit).
    pub event_type: EventType,

    /// Event payload data.
    ///
    /// The interpretation depends on `event_type`:
    /// - Output/Input: The terminal data (text, escape sequences)
    /// - Marker: The marker label
    /// - Resize: Dimensions in "COLSxROWS" format
    /// - Exit: The exit code as a string
    pub data: String,
}

impl Event {
    /// Create a new event with the given parameters.
    pub fn new(time: f64, event_type: EventType, data: impl Into<String>) -> Self {
        Self {
            time,
            event_type,
            data: data.into(),
        }
    }

    /// Create an output event (convenience constructor).
    pub fn output(time: f64, data: impl Into<String>) -> Self {
        Self::new(time, EventType::Output, data)
    }

    /// Create a marker event (convenience constructor).
    pub fn marker(time: f64, label: impl Into<String>) -> Self {
        Self::new(time, EventType::Marker, label)
    }

    /// Check if this is an output event.
    pub fn is_output(&self) -> bool {
        self.event_type == EventType::Output
    }

    /// Check if this is a marker event.
    pub fn is_marker(&self) -> bool {
        self.event_type == EventType::Marker
    }

    /// Check if this is a resize event.
    pub fn is_resize(&self) -> bool {
        self.event_type == EventType::Resize
    }

    /// Parse resize event data into (cols, rows) dimensions.
    ///
    /// Returns `None` if this is not a resize event or the data is malformed.
    /// Resize data is expected in "COLSxROWS" format (e.g., "80x24").
    pub fn parse_resize(&self) -> Option<(u32, u32)> {
        if !self.is_resize() {
            return None;
        }
        let parts: Vec<&str> = self.data.split('x').collect();
        if parts.len() == 2 {
            let cols = parts[0].parse().ok()?;
            let rows = parts[1].parse().ok()?;
            Some((cols, rows))
        } else {
            None
        }
    }
}

// ============================================================================
// AsciicastFile
// ============================================================================

/// A complete asciicast recording with header and events.
///
/// This is the main type for working with asciicast files. It holds the
/// recording metadata (header) and all events in memory. For large recordings,
/// be aware that files can contain millions of events.
///
/// # Example
///
/// ```no_run
/// use agr::{AsciicastFile, Header};
///
/// let mut file = AsciicastFile::new(Header {
///     version: 3,
///     width: None,
///     height: None,
///     term: None,
///     timestamp: None,
///     duration: None,
///     title: Some("My Recording".to_string()),
///     command: None,
///     env: None,
///     idle_time_limit: None,
/// });
/// ```
#[derive(Debug, Clone)]
pub struct AsciicastFile {
    /// Recording metadata.
    pub header: Header,

    /// All events in the recording, in chronological order.
    pub events: Vec<Event>,
}

impl AsciicastFile {
    /// Create a new asciicast file with the given header and no events.
    pub fn new(header: Header) -> Self {
        Self {
            header,
            events: Vec::new(),
        }
    }

    /// Get all marker events in the recording.
    pub fn markers(&self) -> Vec<&Event> {
        self.events.iter().filter(|e| e.is_marker()).collect()
    }

    /// Get all output events in the recording.
    pub fn outputs(&self) -> Vec<&Event> {
        self.events.iter().filter(|e| e.is_output()).collect()
    }

    /// Calculate cumulative (absolute) timestamps for all events.
    ///
    /// Since events store relative times, this computes the running total
    /// to get the absolute time of each event from recording start.
    pub fn cumulative_times(&self) -> Vec<f64> {
        let mut times = Vec::with_capacity(self.events.len());
        let mut cumulative = 0.0;
        for event in &self.events {
            cumulative += event.time;
            times.push(cumulative);
        }
        times
    }

    /// Find the insertion index for an event at the given absolute timestamp.
    ///
    /// Returns the index where a new event should be inserted to maintain
    /// chronological order.
    pub fn find_insertion_index(&self, timestamp: f64) -> usize {
        let cumulative_times = self.cumulative_times();
        for (i, &time) in cumulative_times.iter().enumerate() {
            if time > timestamp {
                return i;
            }
        }
        self.events.len()
    }

    /// Calculate the relative time for insertion at a given index.
    ///
    /// Given an absolute timestamp and target index, computes what the
    /// relative time value should be for an event inserted at that position.
    pub fn calculate_relative_time(&self, index: usize, absolute_timestamp: f64) -> f64 {
        if index == 0 {
            return absolute_timestamp;
        }

        let cumulative_times = self.cumulative_times();
        let prev_cumulative = cumulative_times.get(index - 1).copied().unwrap_or(0.0);
        absolute_timestamp - prev_cumulative
    }

    /// Get the total duration of the recording in seconds.
    pub fn duration(&self) -> f64 {
        self.cumulative_times().last().copied().unwrap_or(0.0)
    }

    /// Get concatenated output text up to a specific timestamp.
    ///
    /// Returns all output data combined up to (and including) the given
    /// absolute timestamp. Useful for getting terminal state at a point in time.
    pub fn output_at(&self, timestamp: f64) -> String {
        let mut output = String::new();
        let mut cumulative = 0.0;

        for event in &self.events {
            cumulative += event.time;
            if cumulative > timestamp {
                break;
            }
            if event.is_output() {
                output.push_str(&event.data);
            }
        }

        output
    }

    /// Get the count of marker events in the recording.
    pub fn marker_count(&self) -> usize {
        self.events.iter().filter(|e| e.is_marker()).count()
    }

    /// Get the terminal dimensions from the header.
    ///
    /// Returns (cols, rows), defaulting to (80, 24) if not specified.
    pub fn terminal_size(&self) -> (u32, u32) {
        let cols = self.header.term.as_ref().and_then(|t| t.cols).unwrap_or(80);
        let rows = self.header.term.as_ref().and_then(|t| t.rows).unwrap_or(24);
        (cols, rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_file() -> AsciicastFile {
        let mut file = AsciicastFile::new(Header {
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
        });
        file.events.push(Event::output(0.1, "hello"));
        file.events.push(Event::output(0.2, " world"));
        file.events.push(Event::marker(0.1, "test marker"));
        file.events.push(Event::output(0.3, "!"));
        file
    }

    #[test]
    fn event_type_from_code() {
        assert_eq!(EventType::from_code("o"), Some(EventType::Output));
        assert_eq!(EventType::from_code("i"), Some(EventType::Input));
        assert_eq!(EventType::from_code("m"), Some(EventType::Marker));
        assert_eq!(EventType::from_code("r"), Some(EventType::Resize));
        assert_eq!(EventType::from_code("x"), Some(EventType::Exit));
        assert_eq!(EventType::from_code("z"), None);
    }

    #[test]
    fn event_type_to_code() {
        assert_eq!(EventType::Output.to_code(), "o");
        assert_eq!(EventType::Input.to_code(), "i");
        assert_eq!(EventType::Marker.to_code(), "m");
        assert_eq!(EventType::Resize.to_code(), "r");
        assert_eq!(EventType::Exit.to_code(), "x");
    }

    #[test]
    fn markers_returns_only_markers() {
        let file = create_test_file();
        let markers = file.markers();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].data, "test marker");
    }

    #[test]
    fn outputs_returns_only_outputs() {
        let file = create_test_file();
        let outputs = file.outputs();
        assert_eq!(outputs.len(), 3);
    }

    #[test]
    fn cumulative_times_calculates_correctly() {
        let file = create_test_file();
        let times = file.cumulative_times();
        assert_eq!(times.len(), 4);
        assert!((times[0] - 0.1).abs() < 0.001);
        assert!((times[1] - 0.3).abs() < 0.001);
        assert!((times[2] - 0.4).abs() < 0.001);
        assert!((times[3] - 0.7).abs() < 0.001);
    }

    #[test]
    fn duration_returns_total_time() {
        let file = create_test_file();
        assert!((file.duration() - 0.7).abs() < 0.001);
    }

    #[test]
    fn output_at_returns_output_up_to_timestamp() {
        let file = create_test_file();

        // At 0.0 - nothing yet
        assert_eq!(file.output_at(0.0), "");

        // At 0.15 - just "hello"
        assert_eq!(file.output_at(0.15), "hello");

        // At 0.35 - "hello world"
        assert_eq!(file.output_at(0.35), "hello world");

        // At 1.0 - everything
        assert_eq!(file.output_at(1.0), "hello world!");
    }

    #[test]
    fn find_insertion_index_works() {
        let file = create_test_file();
        assert_eq!(file.find_insertion_index(0.0), 0);
        assert_eq!(file.find_insertion_index(0.15), 1);
        assert_eq!(file.find_insertion_index(1.0), 4);
    }

    #[test]
    fn marker_count_returns_correct_count() {
        let file = create_test_file();
        assert_eq!(file.marker_count(), 1);

        let empty = AsciicastFile::new(Header {
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
        });
        assert_eq!(empty.marker_count(), 0);
    }

    #[test]
    fn terminal_size_returns_defaults_when_missing() {
        let file = create_test_file();
        assert_eq!(file.terminal_size(), (80, 24));
    }

    #[test]
    fn terminal_size_returns_header_values() {
        let mut file = create_test_file();
        file.header.term = Some(TermInfo {
            cols: Some(120),
            rows: Some(40),
            term_type: None,
        });
        assert_eq!(file.terminal_size(), (120, 40));
    }

    #[test]
    fn parse_resize_returns_dimensions() {
        let event = Event::new(0.1, EventType::Resize, "100x50");
        assert_eq!(event.parse_resize(), Some((100, 50)));
    }

    #[test]
    fn parse_resize_returns_none_for_output() {
        let event = Event::output(0.1, "hello");
        assert_eq!(event.parse_resize(), None);
    }

    #[test]
    fn parse_resize_returns_none_for_malformed() {
        let event = Event::new(0.1, EventType::Resize, "invalid");
        assert_eq!(event.parse_resize(), None);
    }

    #[test]
    fn is_resize_returns_true_for_resize_events() {
        let event = Event::new(0.1, EventType::Resize, "80x24");
        assert!(event.is_resize());
    }

    #[test]
    fn is_resize_returns_false_for_output() {
        let event = Event::output(0.1, "hello");
        assert!(!event.is_resize());
    }
}
