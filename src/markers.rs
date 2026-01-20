//! Marker injection for asciicast files

use crate::asciicast::{AsciicastFile, Event, EventType};
use anyhow::{bail, Result};
use std::path::Path;

/// Marker manager for adding/listing markers in asciicast files
pub struct MarkerManager;

impl MarkerManager {
    /// Add a marker to an asciicast file at the specified timestamp
    pub fn add_marker<P: AsRef<Path>>(
        path: P,
        timestamp: f64,
        label: &str,
    ) -> Result<()> {
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
    pub fn add_marker_to_cast(
        cast: &mut AsciicastFile,
        timestamp: f64,
        label: &str,
    ) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    fn sample_cast() -> &'static str {
        r#"{"version":3,"term":{"cols":80,"rows":24}}
[0.5,"o","$ echo hello\r\n"]
[0.1,"o","hello\r\n"]
[0.2,"o","$ "]"#
    }

    fn cast_with_markers() -> &'static str {
        r#"{"version":3,"term":{"cols":80,"rows":24}}
[0.5,"o","$ make build\r\n"]
[1.0,"m","Build started"]
[2.5,"o","Build complete\r\n"]
[0.1,"m","Build finished"]"#
    }

    fn create_temp_cast(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn add_marker_inserts_at_correct_position() {
        let temp = create_temp_cast(sample_cast());

        // Cumulative times: 0.5, 0.6, 0.8
        // Insert marker at 0.55 (between first two events)
        MarkerManager::add_marker(temp.path(), 0.55, "Test marker").unwrap();

        let cast = AsciicastFile::parse(temp.path()).unwrap();
        assert_eq!(cast.events.len(), 4);
        assert!(cast.events[1].is_marker());
        assert_eq!(cast.events[1].data, "Test marker");
    }

    #[test]
    fn add_marker_at_start() {
        let temp = create_temp_cast(sample_cast());
        MarkerManager::add_marker(temp.path(), 0.1, "Start marker").unwrap();

        let cast = AsciicastFile::parse(temp.path()).unwrap();
        assert_eq!(cast.events.len(), 4);
        assert!(cast.events[0].is_marker());
    }

    #[test]
    fn add_marker_at_end() {
        let temp = create_temp_cast(sample_cast());
        MarkerManager::add_marker(temp.path(), 10.0, "End marker").unwrap();

        let cast = AsciicastFile::parse(temp.path()).unwrap();
        assert_eq!(cast.events.len(), 4);
        assert!(cast.events.last().unwrap().is_marker());
    }

    #[test]
    fn add_marker_preserves_existing_events() {
        let temp = create_temp_cast(sample_cast());
        let original_cast = AsciicastFile::parse(temp.path()).unwrap();
        let original_event_count = original_cast.events.len();

        MarkerManager::add_marker(temp.path(), 0.55, "Test marker").unwrap();

        let modified_cast = AsciicastFile::parse(temp.path()).unwrap();
        assert_eq!(modified_cast.events.len(), original_event_count + 1);

        // Check that original output events are still present
        let outputs: Vec<_> = modified_cast.events.iter().filter(|e| e.is_output()).collect();
        assert_eq!(outputs.len(), 3);
    }

    #[test]
    fn list_markers_returns_all_markers() {
        let temp = create_temp_cast(cast_with_markers());
        let markers = MarkerManager::list_markers(temp.path()).unwrap();

        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].label, "Build started");
        assert_eq!(markers[1].label, "Build finished");
    }

    #[test]
    fn list_markers_returns_empty_for_no_markers() {
        let temp = create_temp_cast(sample_cast());
        let markers = MarkerManager::list_markers(temp.path()).unwrap();
        assert!(markers.is_empty());
    }

    #[test]
    fn add_marker_rejects_negative_timestamp() {
        let temp = create_temp_cast(sample_cast());
        let result = MarkerManager::add_marker(temp.path(), -1.0, "Bad marker");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("negative"));
    }

    #[test]
    fn add_marker_rejects_empty_label() {
        let temp = create_temp_cast(sample_cast());
        let result = MarkerManager::add_marker(temp.path(), 0.5, "  ");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn marker_display_format() {
        let marker = MarkerInfo {
            timestamp: 45.2,
            label: "Build error".to_string(),
        };
        assert_eq!(format!("{}", marker), "45.2s: Build error");
    }
}
