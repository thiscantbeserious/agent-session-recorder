//! Unit tests for asciicast module

use super::helpers::{load_fixture, temp_fixture};

use agr::{AsciicastFile, Event, EventType};

// === Fixture-based tests (existing) ===

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

// === Inline string tests (merged from src/asciicast.rs) ===

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

#[test]
fn parse_valid_asciicast() {
    let cast = AsciicastFile::parse_str(sample_cast()).unwrap();
    assert_eq!(cast.header.version, 3);
    assert_eq!(cast.events.len(), 3);
}

#[test]
fn parse_extracts_output_events() {
    let cast = AsciicastFile::parse_str(sample_cast()).unwrap();
    let outputs = cast.outputs();
    assert_eq!(outputs.len(), 3);
    assert!(outputs[0].data.contains("echo hello"));
}

#[test]
fn parse_extracts_marker_events() {
    let cast = AsciicastFile::parse_str(cast_with_markers()).unwrap();
    let markers = cast.markers();
    assert_eq!(markers.len(), 2);
    assert_eq!(markers[0].data, "Build started");
    assert_eq!(markers[1].data, "Build finished");
}

#[test]
fn roundtrip_preserves_data() {
    let original = sample_cast();
    let cast = AsciicastFile::parse_str(original).unwrap();
    let written = cast.to_string().unwrap();
    let reparsed = AsciicastFile::parse_str(&written).unwrap();

    assert_eq!(reparsed.header.version, cast.header.version);
    assert_eq!(reparsed.events.len(), cast.events.len());
    for (orig, reparsed) in cast.events.iter().zip(reparsed.events.iter()) {
        assert_eq!(orig.time, reparsed.time);
        assert_eq!(orig.event_type, reparsed.event_type);
        assert_eq!(orig.data, reparsed.data);
    }
}

#[test]
fn cumulative_times_calculated_correctly() {
    let cast = AsciicastFile::parse_str(sample_cast()).unwrap();
    let times = cast.cumulative_times();
    assert_eq!(times.len(), 3);
    assert!((times[0] - 0.5).abs() < 0.001);
    assert!((times[1] - 0.6).abs() < 0.001);
    assert!((times[2] - 0.8).abs() < 0.001);
}

#[test]
fn find_insertion_index_at_start() {
    let cast = AsciicastFile::parse_str(sample_cast()).unwrap();
    assert_eq!(cast.find_insertion_index(0.1), 0);
}

#[test]
fn find_insertion_index_in_middle() {
    let cast = AsciicastFile::parse_str(sample_cast()).unwrap();
    // Cumulative times: 0.5, 0.6, 0.8
    assert_eq!(cast.find_insertion_index(0.55), 1);
}

#[test]
fn find_insertion_index_at_end() {
    let cast = AsciicastFile::parse_str(sample_cast()).unwrap();
    assert_eq!(cast.find_insertion_index(10.0), 3);
}

#[test]
fn event_type_conversion() {
    assert_eq!(EventType::from_code("o"), Some(EventType::Output));
    assert_eq!(EventType::from_code("i"), Some(EventType::Input));
    assert_eq!(EventType::from_code("m"), Some(EventType::Marker));
    assert_eq!(EventType::from_code("r"), Some(EventType::Resize));
    assert_eq!(EventType::from_code("x"), Some(EventType::Exit));
    assert_eq!(EventType::from_code("z"), None);

    assert_eq!(EventType::Output.to_code(), "o");
    assert_eq!(EventType::Marker.to_code(), "m");
    assert_eq!(EventType::Exit.to_code(), "x");
}

#[test]
fn rejects_non_v3_files() {
    let v2_content = r#"{"version":2,"width":80,"height":24}"#;
    let result = AsciicastFile::parse_str(v2_content);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("v3"));
}
