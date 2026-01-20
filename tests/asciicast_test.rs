//! Integration tests for asciicast module

mod helpers;

use agr::{AsciicastFile, Event};
use helpers::{load_fixture, temp_fixture};

#[test]
fn parse_sample_cast_file() {
    let content = load_fixture("sample.cast");
    let cast = AsciicastFile::parse_str(&content).unwrap();

    assert_eq!(cast.header.version, 3);
    assert_eq!(cast.events.len(), 3);
}

#[test]
fn parse_extracts_all_output_events() {
    let content = load_fixture("sample.cast");
    let cast = AsciicastFile::parse_str(&content).unwrap();

    let outputs = cast.outputs();
    assert_eq!(outputs.len(), 3);
    assert!(outputs[0].data.contains("echo hello"));
    assert!(outputs[1].data.contains("hello"));
}

#[test]
fn parse_extracts_all_marker_events() {
    let content = load_fixture("with_markers.cast");
    let cast = AsciicastFile::parse_str(&content).unwrap();

    let markers = cast.markers();
    assert_eq!(markers.len(), 2);
    assert_eq!(markers[0].data, "Build started");
    assert_eq!(markers[1].data, "Build finished");
}

#[test]
fn roundtrip_preserves_all_data() {
    let content = load_fixture("sample.cast");
    let cast = AsciicastFile::parse_str(&content).unwrap();
    let written = cast.to_string().unwrap();
    let reparsed = AsciicastFile::parse_str(&written).unwrap();

    assert_eq!(reparsed.header.version, cast.header.version);
    assert_eq!(reparsed.events.len(), cast.events.len());
}

#[test]
fn parse_file_from_path() {
    let (_temp_dir, path) = temp_fixture("sample.cast");
    let cast = AsciicastFile::parse(&path).unwrap();

    assert_eq!(cast.header.version, 3);
    assert_eq!(cast.events.len(), 3);
}

#[test]
fn write_file_to_path() {
    let (_temp_dir, path) = temp_fixture("sample.cast");
    let mut cast = AsciicastFile::parse(&path).unwrap();

    // Add a new event
    cast.events.push(Event::output(0.1, "new output"));

    // Write back
    cast.write(&path).unwrap();

    // Verify
    let reloaded = AsciicastFile::parse(&path).unwrap();
    assert_eq!(reloaded.events.len(), 4);
}

#[test]
fn cumulative_times_sum_correctly() {
    let content = load_fixture("sample.cast");
    let cast = AsciicastFile::parse_str(&content).unwrap();

    let times = cast.cumulative_times();
    // Events have times: 0.5, 0.1, 0.2
    // Cumulative: 0.5, 0.6, 0.8
    assert!((times[0] - 0.5).abs() < 0.001);
    assert!((times[1] - 0.6).abs() < 0.001);
    assert!((times[2] - 0.8).abs() < 0.001);
}
