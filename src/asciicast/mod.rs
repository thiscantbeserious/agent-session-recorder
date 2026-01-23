//! asciicast v3 format parser and writer
//!
//! This module provides types and utilities for working with asciicast v3 files,
//! the format used by asciinema for terminal recordings.
//!
//! Reference: https://docs.asciinema.org/manual/asciicast/v3/
//!
//! # Structure
//!
//! - `reader` - Parsing asciicast files from various sources
//! - `writer` - Writing asciicast files to various destinations
//! - `marker` - Adding and listing markers in recordings

pub mod marker;
mod reader;
mod writer;

pub use marker::{MarkerInfo, MarkerManager};

use serde::{Deserialize, Serialize};

// ============================================================================
// Header Types
// ============================================================================

/// asciicast v3 header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    pub version: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub term: Option<TermInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<EnvInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_time_limit: Option<f64>,
}

/// Terminal information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cols: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<u32>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub term_type: Option<String>,
}

/// Environment information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvInfo {
    #[serde(rename = "SHELL", skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    #[serde(rename = "TERM", skip_serializing_if = "Option::is_none")]
    pub term: Option<String>,
}

// ============================================================================
// Event Types
// ============================================================================

/// Event type codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    /// Output (data written to terminal)
    Output, // "o"
    /// Input (data read from terminal)
    Input, // "i"
    /// Marker (annotation)
    Marker, // "m"
    /// Resize (terminal resize)
    Resize, // "r"
    /// Exit (process exit code)
    Exit, // "x"
}

impl EventType {
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

/// An event in the asciicast file
#[derive(Debug, Clone)]
pub struct Event {
    /// Time offset from previous event (in seconds)
    pub time: f64,
    /// Event type
    pub event_type: EventType,
    /// Event data (output text, marker label, etc.)
    pub data: String,
}

impl Event {
    pub fn new(time: f64, event_type: EventType, data: impl Into<String>) -> Self {
        Self {
            time,
            event_type,
            data: data.into(),
        }
    }

    pub fn output(time: f64, data: impl Into<String>) -> Self {
        Self::new(time, EventType::Output, data)
    }

    pub fn marker(time: f64, label: impl Into<String>) -> Self {
        Self::new(time, EventType::Marker, label)
    }

    pub fn is_output(&self) -> bool {
        self.event_type == EventType::Output
    }

    pub fn is_marker(&self) -> bool {
        self.event_type == EventType::Marker
    }
}

// ============================================================================
// AsciicastFile
// ============================================================================

/// Complete asciicast file representation
#[derive(Debug, Clone)]
pub struct AsciicastFile {
    pub header: Header,
    pub events: Vec<Event>,
}

impl AsciicastFile {
    /// Create a new asciicast file with the given header
    pub fn new(header: Header) -> Self {
        Self {
            header,
            events: Vec::new(),
        }
    }

    /// Get all marker events
    pub fn markers(&self) -> Vec<&Event> {
        self.events.iter().filter(|e| e.is_marker()).collect()
    }

    /// Get all output events
    pub fn outputs(&self) -> Vec<&Event> {
        self.events.iter().filter(|e| e.is_output()).collect()
    }

    /// Calculate cumulative time for each event
    pub fn cumulative_times(&self) -> Vec<f64> {
        let mut times = Vec::with_capacity(self.events.len());
        let mut cumulative = 0.0;
        for event in &self.events {
            cumulative += event.time;
            times.push(cumulative);
        }
        times
    }

    /// Find the insertion index for a marker at the given absolute timestamp
    pub fn find_insertion_index(&self, timestamp: f64) -> usize {
        let cumulative_times = self.cumulative_times();
        for (i, &time) in cumulative_times.iter().enumerate() {
            if time > timestamp {
                return i;
            }
        }
        self.events.len()
    }

    /// Calculate the relative time for insertion at a given index
    pub fn calculate_relative_time(&self, index: usize, absolute_timestamp: f64) -> f64 {
        if index == 0 {
            return absolute_timestamp;
        }

        let cumulative_times = self.cumulative_times();
        let prev_cumulative = cumulative_times.get(index - 1).copied().unwrap_or(0.0);
        absolute_timestamp - prev_cumulative
    }

    /// Get the total duration of the recording
    pub fn duration(&self) -> f64 {
        self.cumulative_times().last().copied().unwrap_or(0.0)
    }

    /// Get output text up to a specific timestamp
    ///
    /// Returns all output data concatenated up to (and including) the given timestamp.
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
}
