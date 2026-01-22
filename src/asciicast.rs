//! asciicast v3 format parser and writer
//!
//! Reference: https://docs.asciinema.org/manual/asciicast/v3/

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

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
}
