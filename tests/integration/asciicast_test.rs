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

// === Gzip round trips ===

use agr::files::codec;
use std::path::Path;

const CORRUPT: &str = "{\"version\":3}\n[0.1, \"o\", \"a\"]\nnot json\n[0.2, \"o\", \"b\"]\n";

fn seed(path: &Path, content: &str) {
    codec::atomic_write(path, |w| Ok(w.write_all(content.as_bytes())?)).unwrap();
}

fn is_gzip_file(path: &Path) -> bool {
    std::fs::read(path).unwrap().starts_with(&[0x1f, 0x8b])
}

#[test]
fn parse_gz_equals_plain_twin() {
    let dir = tempfile::TempDir::new().unwrap();
    let content = load_fixture("sample.cast");
    seed(&dir.path().join("x.cast"), &content);
    seed(&dir.path().join("x.cast.gz"), &content);

    let plain = AsciicastFile::parse(dir.path().join("x.cast")).unwrap();
    let gz = AsciicastFile::parse(dir.path().join("x.cast.gz")).ok();

    assert_eq!(gz.and_then(|c| c.to_string().ok()), plain.to_string().ok());
}

#[test]
fn write_keeps_format_by_suffix_and_leaves_no_temp() {
    let dir = tempfile::TempDir::new().unwrap();
    let cast = AsciicastFile::parse_str(&load_fixture("sample.cast")).unwrap();

    for (name, gzip) in [("x.cast", false), ("x.cast.gz", true)] {
        let path = dir.path().join(name);
        cast.write(&path).unwrap();
        assert_eq!(is_gzip_file(&path), gzip, "{name}");
        assert_eq!(
            AsciicastFile::parse(&path).unwrap().to_string().ok(),
            cast.to_string().ok()
        );
    }
    assert!(!dir.path().join("x.cast.cast.tmp").exists());
    assert!(!codec::temp_path_for(&dir.path().join("x.cast.gz")).exists());
}

#[test]
fn diagnose_and_repair_keep_format() {
    let dir = tempfile::TempDir::new().unwrap();

    for (name, gzip) in [("x.cast", false), ("x.cast.gz", true)] {
        let path = dir.path().join(name);
        seed(&path, CORRUPT);

        let found = agr::asciicast::diagnose(&path)
            .ok()
            .map(|d| d.bad_lines.len());
        assert_eq!(found, Some(1), "{name}");
        assert_eq!(agr::asciicast::repair(&path).ok(), Some(1), "{name}");
        assert_eq!(is_gzip_file(&path), gzip, "{name}");
        assert_eq!(AsciicastFile::parse(&path).unwrap().events.len(), 2);
    }
}

// === Integrity on gzip sessions ===

mod integrity_gzip {
    use agr::asciicast::{diagnose, repair, DiagnoseResult};
    use agr::files::codec;
    use std::path::Path;
    use std::sync::mpsc;
    use std::time::Duration;
    use tempfile::TempDir;

    fn write_session_gz(path: &Path, events: usize) {
        codec::atomic_write(path, |w| {
            writeln!(w, "{{\"version\":3}}")?;
            for i in 0..events {
                writeln!(w, "[0.1, \"o\", \"line {i} {}\"]", "x".repeat(40))?;
            }
            Ok(())
        })
        .unwrap();
    }

    fn truncate_to_half(path: &Path) {
        let bytes = std::fs::read(path).unwrap();
        std::fs::write(path, &bytes[..bytes.len() / 2]).unwrap();
    }

    fn diagnose_with_timeout(path: &Path) -> DiagnoseResult {
        let (tx, rx) = mpsc::channel();
        let path = path.to_path_buf();
        std::thread::spawn(move || {
            let _ = tx.send(diagnose(&path));
        });
        rx.recv_timeout(Duration::from_secs(10))
            .expect("diagnose did not return within 10s")
            .unwrap()
    }

    #[test]
    fn diagnose_truncated_gzip_returns_and_records_decode_error_once() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("t.cast.gz");
        write_session_gz(&path, 20_000);
        truncate_to_half(&path);

        let result = diagnose_with_timeout(&path);

        assert_eq!(result.bad_lines.len(), 1, "{:?}", result.bad_lines);
        assert!(result.valid_event_lines > 0);
    }

    #[test]
    fn repair_truncated_gzip_aborts_and_keeps_original() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("t.cast.gz");
        write_session_gz(&path, 20_000);
        truncate_to_half(&path);
        let before = std::fs::read(&path).unwrap();

        assert!(repair(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    fn write_raw(dir: &TempDir, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.path().join("b.cast");
        let mut data = b"{\"version\":3}\n[0.1, \"o\", \"ok\"]\n".to_vec();
        data.extend_from_slice(bytes);
        data.extend_from_slice(b"\n[0.2, \"o\", \"ok\"]\n");
        std::fs::write(&path, data).unwrap();
        path
    }

    #[test]
    fn repair_and_diagnose_agree_on_invalid_utf8_lines() {
        let dir = TempDir::new().unwrap();
        let path = write_raw(&dir, &[0xff, 0xfe, b'x']);

        let diagnosed = diagnose(&path).unwrap();
        let removed = repair(&path).ok();

        assert_eq!(diagnosed.bad_lines.len(), 1);
        assert_eq!(removed, Some(diagnosed.bad_lines.len()));
        assert_eq!(diagnose(&path).unwrap().bad_lines.len(), 0);
    }

    #[test]
    fn repair_drops_only_the_invalid_utf8_line_and_keeps_valid_bytes() {
        let dir = TempDir::new().unwrap();
        let path = write_raw(&dir, &[0xff, 0xfe, b'x']);

        assert_eq!(repair(&path).unwrap(), 1);

        let expected = b"{\"version\":3}\n[0.1, \"o\", \"ok\"]\n[0.2, \"o\", \"ok\"]\n";
        assert_eq!(std::fs::read(&path).unwrap(), expected);
    }

    #[test]
    fn repair_and_diagnose_agree_on_json_errors() {
        let dir = TempDir::new().unwrap();
        let path = write_raw(&dir, b"[not json");

        let diagnosed = diagnose(&path).unwrap();
        let removed = repair(&path).unwrap();

        assert_eq!(removed, diagnosed.bad_lines.len());
        assert_eq!(removed, 1);
    }
}
