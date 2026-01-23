//! Unit tests for markers module

use super::helpers::temp_fixture;

use agr::{AsciicastFile, MarkerInfo, MarkerManager};
use std::io::Write;
use tempfile::NamedTempFile;

// === Fixture-based tests (existing) ===

#[test]
fn add_marker_creates_marker_event() {
    let (_temp_dir, path) = temp_fixture("sample.cast");

    MarkerManager::add_marker(&path, 0.55, "Test marker").unwrap();

    let cast = AsciicastFile::parse(&path).unwrap();
    let markers = cast.markers();
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0].data, "Test marker");
}

#[test]
fn add_marker_preserves_existing_output() {
    let (_temp_dir, path) = temp_fixture("sample.cast");
    let original = AsciicastFile::parse(&path).unwrap();
    let original_output_count = original.outputs().len();

    MarkerManager::add_marker(&path, 0.55, "Test marker").unwrap();

    let modified = AsciicastFile::parse(&path).unwrap();
    assert_eq!(modified.outputs().len(), original_output_count);
}

#[test]
fn add_multiple_markers() {
    let (_temp_dir, path) = temp_fixture("sample.cast");

    MarkerManager::add_marker(&path, 0.3, "First marker").unwrap();
    MarkerManager::add_marker(&path, 0.7, "Second marker").unwrap();

    let markers = MarkerManager::list_markers(&path).unwrap();
    assert_eq!(markers.len(), 2);
}

#[test]
fn list_markers_returns_timestamps() {
    let (_temp_dir, path) = temp_fixture("with_markers.cast");

    let markers = MarkerManager::list_markers(&path).unwrap();
    assert_eq!(markers.len(), 2);

    // First marker at cumulative time 1.5 (0.5 + 1.0)
    assert!((markers[0].timestamp - 1.5).abs() < 0.1);
    assert_eq!(markers[0].label, "Build started");
}

#[test]
fn add_marker_at_start() {
    let (_temp_dir, path) = temp_fixture("sample.cast");

    MarkerManager::add_marker(&path, 0.1, "Start marker").unwrap();

    let cast = AsciicastFile::parse(&path).unwrap();
    assert!(cast.events[0].is_marker());
}

#[test]
fn add_marker_at_end() {
    let (_temp_dir, path) = temp_fixture("sample.cast");

    MarkerManager::add_marker(&path, 100.0, "End marker").unwrap();

    let cast = AsciicastFile::parse(&path).unwrap();
    assert!(cast.events.last().unwrap().is_marker());
}

#[test]
fn reject_negative_timestamp() {
    let (_temp_dir, path) = temp_fixture("sample.cast");

    let result = MarkerManager::add_marker(&path, -1.0, "Bad marker");
    assert!(result.is_err());
}

#[test]
fn reject_empty_label() {
    let (_temp_dir, path) = temp_fixture("sample.cast");

    let result = MarkerManager::add_marker(&path, 0.5, "");
    assert!(result.is_err());
}

// === Inline string tests (merged from src/markers.rs) ===

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
fn add_marker_at_start_inline() {
    let temp = create_temp_cast(sample_cast());
    MarkerManager::add_marker(temp.path(), 0.1, "Start marker").unwrap();

    let cast = AsciicastFile::parse(temp.path()).unwrap();
    assert_eq!(cast.events.len(), 4);
    assert!(cast.events[0].is_marker());
}

#[test]
fn add_marker_at_end_inline() {
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
    let outputs: Vec<_> = modified_cast
        .events
        .iter()
        .filter(|e| e.is_output())
        .collect();
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
