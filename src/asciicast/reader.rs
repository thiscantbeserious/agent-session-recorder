//! Asciicast v3 file parser.
//!
//! This module provides parsing functionality for asciicast v3 files from
//! various sources: file paths, readers, and strings. The parser reads
//! NDJSON format where the first line is a JSON header and subsequent
//! lines are event arrays.
//!
//! # Format
//!
//! ```text
//! {"version":3,"term":{"cols":80,"rows":24}}  <- Header (JSON object)
//! [0.5,"o","Hello "]                          <- Event (JSON array)
//! [0.3,"o","world!"]                          <- Event (JSON array)
//! ```
//!
//! # Error Handling
//!
//! All parsing functions return `Result` and provide context for errors:
//! - File I/O errors include the file path
//! - JSON parsing errors include the line number
//! - Version mismatches report the found version
//!
//! # Example
//!
//! ```no_run
//! use agr::AsciicastFile;
//!
//! // Parse from file path
//! let file = AsciicastFile::parse("recording.cast")?;
//!
//! // Parse from string
//! let content = r#"{"version":3}
//! [0.1,"o","hello"]"#;
//! let file = AsciicastFile::parse_str(content)?;
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{bail, Context, Result};

use super::types::{AsciicastFile, Event, EventType, Header};

impl Event {
    /// Parse an event from a JSON line.
    ///
    /// Expects an array format: `[time, type_code, data]` where:
    /// - `time` is a number (seconds since previous event)
    /// - `type_code` is a string ("o", "i", "m", "r", or "x")
    /// - `data` is a string (event payload)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The line is not valid JSON
    /// - The JSON is not an array with at least 3 elements
    /// - The time is not a number
    /// - The type code is not a recognized string
    /// - The data is not a string
    pub fn from_json(line: &str) -> Result<Self> {
        let value: serde_json::Value =
            serde_json::from_str(line).context("Failed to parse event JSON")?;

        let arr = value.as_array().context("Event must be a JSON array")?;

        if arr.len() < 3 {
            bail!("Event array must have at least 3 elements");
        }

        let time = arr[0].as_f64().context("Event time must be a number")?;

        let code = arr[1].as_str().context("Event type must be a string")?;

        let event_type =
            EventType::from_code(code).with_context(|| format!("Unknown event type: {}", code))?;

        let data = arr[2]
            .as_str()
            .context("Event data must be a string")?
            .to_string();

        Ok(Event {
            time,
            event_type,
            data,
        })
    }
}

impl AsciicastFile {
    /// Parse an asciicast v3 file from a filesystem path.
    ///
    /// Opens the file and delegates to [`parse_reader`](Self::parse_reader).
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened or parsed.
    pub fn parse<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let file =
            fs::File::open(path).with_context(|| format!("Failed to open file: {:?}", path))?;
        let reader = BufReader::new(file);

        Self::parse_reader(reader)
    }

    /// Parse an asciicast v3 file from any buffered reader.
    ///
    /// Reads the first line as a JSON header, then parses each subsequent
    /// line as an event. Empty lines are skipped.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file is empty
    /// - The header is not valid JSON or missing `version` field
    /// - The version is not 3
    /// - Any event line fails to parse
    pub fn parse_reader<R: BufRead>(reader: R) -> Result<Self> {
        let mut lines = reader.lines();

        // First line is the header
        let header_line = lines
            .next()
            .context("File is empty")?
            .context("Failed to read header line")?;

        let header: Header =
            serde_json::from_str(&header_line).context("Failed to parse header")?;

        if header.version != 3 {
            bail!(
                "Only asciicast v3 format is supported (got version {})",
                header.version
            );
        }

        // Remaining lines are events
        let mut events = Vec::new();
        for (line_num, line_result) in lines.enumerate() {
            let line =
                line_result.with_context(|| format!("Failed to read line {}", line_num + 2))?;

            if line.trim().is_empty() {
                continue;
            }

            let event = Event::from_json(&line)
                .with_context(|| format!("Failed to parse event on line {}", line_num + 2))?;
            events.push(event);
        }

        Ok(AsciicastFile { header, events })
    }

    /// Parse an asciicast v3 file from a string.
    ///
    /// Convenience wrapper around [`parse_reader`](Self::parse_reader).
    ///
    /// # Errors
    ///
    /// Returns an error if parsing fails (see `parse_reader`).
    pub fn parse_str(content: &str) -> Result<Self> {
        let reader = BufReader::new(content.as_bytes());
        Self::parse_reader(reader)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_event_output() {
        let json = r#"[0.5, "o", "hello"]"#;
        let event = Event::from_json(json).unwrap();
        assert_eq!(event.time, 0.5);
        assert_eq!(event.event_type, EventType::Output);
        assert_eq!(event.data, "hello");
    }

    #[test]
    fn parse_event_marker() {
        let json = r#"[1.0, "m", "test marker"]"#;
        let event = Event::from_json(json).unwrap();
        assert_eq!(event.time, 1.0);
        assert_eq!(event.event_type, EventType::Marker);
        assert_eq!(event.data, "test marker");
    }

    #[test]
    fn parse_event_invalid_json() {
        let result = Event::from_json("not json");
        assert!(result.is_err());
    }

    #[test]
    fn parse_event_missing_elements() {
        let result = Event::from_json(r#"[0.5, "o"]"#);
        assert!(result.is_err());
    }

    #[test]
    fn parse_file_from_string() {
        let content = r#"{"version":3}
[0.1, "o", "hello"]
[0.2, "o", " world"]"#;

        let file = AsciicastFile::parse_str(content).unwrap();
        assert_eq!(file.header.version, 3);
        assert_eq!(file.events.len(), 2);
    }

    #[test]
    fn parse_file_wrong_version() {
        let content = r#"{"version":2}"#;
        let result = AsciicastFile::parse_str(content);
        assert!(result.is_err());
    }

    #[test]
    fn parse_file_skips_empty_lines() {
        let content = r#"{"version":3}
[0.1, "o", "hello"]

[0.2, "o", " world"]"#;

        let file = AsciicastFile::parse_str(content).unwrap();
        assert_eq!(file.events.len(), 2);
    }
}
