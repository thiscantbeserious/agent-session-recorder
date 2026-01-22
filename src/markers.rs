//! Marker injection for asciicast files

use crate::asciicast::{AsciicastFile, Event, EventType};
use anyhow::{bail, Result};
use std::path::Path;

/// Marker manager for adding/listing markers in asciicast files
pub struct MarkerManager;

impl MarkerManager {
    /// Add a marker to an asciicast file at the specified timestamp
    pub fn add_marker<P: AsRef<Path>>(path: P, timestamp: f64, label: &str) -> Result<()> {
        let path = path.as_ref();

        if timestamp < 0.0 {
            bail!("Timestamp cannot be negative");
        }

        if label.trim().is_empty() {
            bail!("Marker label cannot be empty");
        }

        let mut cast = AsciicastFile::parse(path)?;
        Self::add_marker_to_cast(&mut cast, timestamp, label)?;
        cast.write(path)?;

        Ok(())
    }

    /// Add a marker to an asciicast file in memory
    pub fn add_marker_to_cast(cast: &mut AsciicastFile, timestamp: f64, label: &str) -> Result<()> {
        let index = cast.find_insertion_index(timestamp);
        let relative_time = cast.calculate_relative_time(index, timestamp);

        let marker = Event::marker(relative_time, label);
        cast.events.insert(index, marker);

        // Adjust the next event's time if there is one
        if index + 1 < cast.events.len() {
            let cumulative_times = {
                let mut times = Vec::with_capacity(cast.events.len());
                let mut cumulative = 0.0;
                for event in &cast.events {
                    cumulative += event.time;
                    times.push(cumulative);
                }
                times
            };

            // The next event's cumulative time needs to remain the same
            // So we need to adjust its relative time
            if let Some(next_event) = cast.events.get_mut(index + 1) {
                let original_cumulative = cumulative_times.get(index + 1).copied().unwrap_or(0.0);
                let new_cumulative_for_marker = timestamp;
                let adjustment = original_cumulative - new_cumulative_for_marker - next_event.time;
                if adjustment > 0.0 {
                    next_event.time += adjustment;
                } else {
                    // If the marker is at exactly the same time or after, set next event time to 0
                    let marker_cumulative = cumulative_times.get(index).copied().unwrap_or(0.0);
                    if marker_cumulative >= original_cumulative {
                        next_event.time = 0.0;
                    }
                }
            }
        }

        Ok(())
    }

    /// List all markers in an asciicast file
    pub fn list_markers<P: AsRef<Path>>(path: P) -> Result<Vec<MarkerInfo>> {
        let cast = AsciicastFile::parse(path)?;
        Self::list_markers_from_cast(&cast)
    }

    /// List all markers from an asciicast file in memory
    pub fn list_markers_from_cast(cast: &AsciicastFile) -> Result<Vec<MarkerInfo>> {
        let cumulative_times = cast.cumulative_times();
        let mut markers = Vec::new();

        for (i, event) in cast.events.iter().enumerate() {
            if event.event_type == EventType::Marker {
                let timestamp = cumulative_times.get(i).copied().unwrap_or(0.0);
                markers.push(MarkerInfo {
                    timestamp,
                    label: event.data.clone(),
                });
            }
        }

        Ok(markers)
    }
}

/// Information about a marker
#[derive(Debug, Clone)]
pub struct MarkerInfo {
    pub timestamp: f64,
    pub label: String,
}

impl std::fmt::Display for MarkerInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.1}s: {}", self.timestamp, self.label)
    }
}
