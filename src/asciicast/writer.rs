//! Asciicast v3 file writer
//!
//! Handles writing asciicast files to various destinations.

use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

use super::{AsciicastFile, Event};

impl Event {
    /// Convert event to JSON string
    pub fn to_json(&self) -> String {
        serde_json::to_string(&serde_json::json!([
            self.time,
            self.event_type.to_code(),
            self.data
        ]))
        .unwrap()
    }
}

impl AsciicastFile {
    /// Write the asciicast file to a path
    pub fn write<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        let mut file =
            fs::File::create(path).with_context(|| format!("Failed to create file: {:?}", path))?;

        self.write_to(&mut file)
    }

    /// Write the asciicast file to a writer
    pub fn write_to<W: Write>(&self, writer: &mut W) -> Result<()> {
        // Write header
        let header_json =
            serde_json::to_string(&self.header).context("Failed to serialize header")?;
        writeln!(writer, "{}", header_json)?;

        // Write events
        for event in &self.events {
            writeln!(writer, "{}", event.to_json())?;
        }

        Ok(())
    }

    /// Convert to string
    pub fn to_string(&self) -> Result<String> {
        let mut buffer = Vec::new();
        self.write_to(&mut buffer)?;
        Ok(String::from_utf8(buffer)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asciicast::{EventType, Header};

    #[test]
    fn event_to_json_output() {
        let event = Event::new(0.5, EventType::Output, "hello");
        let json = event.to_json();
        assert_eq!(json, r#"[0.5,"o","hello"]"#);
    }

    #[test]
    fn event_to_json_marker() {
        let event = Event::new(1.0, EventType::Marker, "test");
        let json = event.to_json();
        assert_eq!(json, r#"[1.0,"m","test"]"#);
    }

    #[test]
    fn file_to_string() {
        let mut file = AsciicastFile::new(Header {
            version: 3,
            width: None,
            height: None,
            term: None,
            timestamp: None,
            duration: None,
            title: None,
            command: None,
            env: None,
            idle_time_limit: None,
        });
        file.events.push(Event::output(0.1, "hello"));

        let output = file.to_string().unwrap();
        assert!(output.contains(r#""version":3"#));
        assert!(output.contains(r#"[0.1,"o","hello"]"#));
    }

    #[test]
    fn write_to_buffer() {
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
        file.events.push(Event::output(0.0, "test"));

        let mut buffer = Vec::new();
        file.write_to(&mut buffer).unwrap();

        let output = String::from_utf8(buffer).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2); // header + 1 event
    }
}
