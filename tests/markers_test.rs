//! Integration tests for markers module

mod helpers;

use agr::{AsciicastFile, MarkerManager};
use helpers::temp_fixture;

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
