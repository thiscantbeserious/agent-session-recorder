//! Asciicast v3 file reader/parser
//!
//! Handles parsing asciicast files from various sources.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{bail, Context, Result};

use super::{AsciicastFile, Event, EventType, Header};

impl Event {
    /// Parse an event from a JSON line
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
    /// Parse an asciicast v3 file from a path
    pub fn parse<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let file =
            fs::File::open(path).with_context(|| format!("Failed to open file: {:?}", path))?;
        let reader = BufReader::new(file);

        Self::parse_reader(reader)
    }

    /// Parse an asciicast v3 file from a reader
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

    /// Parse from a string
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
