//! Integration tests for SessionPreview functionality

use super::helpers::{fixtures_dir, temp_fixture};
use agr::tui::widgets::SessionPreview;

#[test]
fn session_preview_loads_from_fixture() {
    let (temp_dir, path) = temp_fixture("sample.cast");
    let preview = SessionPreview::load(&path);

    assert!(preview.is_some(), "Should load preview from valid file");
    let preview = preview.unwrap();

    // sample.cast has 3 output events with times: 0.5, 0.1, 0.2
    // Total duration: 0.8 seconds
    assert!(preview.duration_secs > 0.7 && preview.duration_secs < 0.9);
    assert_eq!(preview.marker_count, 0); // No markers in sample.cast

    drop(temp_dir); // Cleanup
}

#[test]
fn session_preview_counts_markers() {
    let (temp_dir, path) = temp_fixture("with_markers.cast");
    let preview = SessionPreview::load(&path).expect("Should load preview");

    // with_markers.cast has 2 marker events
    assert_eq!(preview.marker_count, 2);

    drop(temp_dir);
}

#[test]
fn session_preview_returns_none_for_invalid_file() {
    // Non-existent file
    let preview = SessionPreview::load("/nonexistent/path/file.cast");
    assert!(preview.is_none());
}

#[test]
fn session_preview_format_duration_formats_correctly() {
    let preview = SessionPreview {
        duration_secs: 3661.5, // 1h 1m 1.5s
        marker_count: 0,
        styled_preview: Vec::new(),
        uncompressed_size: None,
    };

    // Should format as "1h 1m 1s"
    let formatted = preview.format_duration();
    assert!(
        formatted.contains("1h"),
        "Should include hours: {}",
        formatted
    );
    assert!(
        formatted.contains("1m"),
        "Should include minutes: {}",
        formatted
    );
}

#[test]
fn session_preview_generates_styled_preview() {
    let (temp_dir, path) = temp_fixture("sample.cast");
    let preview = SessionPreview::load(&path).expect("Should load preview");

    // At 10% of 0.8 seconds = 0.08 seconds, which is before the first event (0.5s)
    // So the preview should be empty or contain only blank lines at this point.
    // The important thing is that styled_preview is a valid Vec that we can inspect.
    // For sample.cast, which has 3 lines of terminal output, we expect 24 lines
    // (default terminal height) but they may all be empty at 10%.
    assert!(
        !preview.styled_preview.is_empty(),
        "styled_preview should have at least one line (terminal height)"
    );

    // Each styled line should have cells
    for line in &preview.styled_preview {
        // Lines should have width matching terminal dimensions
        // (default is 80 cols, but styled_lines trims trailing spaces)
        assert!(
            line.cells.len() <= 80,
            "Line width should not exceed terminal width"
        );
    }

    drop(temp_dir);
}

#[test]
fn session_preview_loads_from_fixtures_dir() {
    // Test loading directly from fixtures directory
    let sample_path = fixtures_dir().join("sample.cast");
    let preview = SessionPreview::load(&sample_path);

    assert!(
        preview.is_some(),
        "Should load preview from fixtures/sample.cast"
    );
}

#[test]
fn session_preview_loads_gzip_like_plain_twin() {
    let (temp_dir, plain_path) = temp_fixture("with_markers.cast");
    let gz_path = temp_dir.path().join("with_markers.cast.gz");
    agr::files::codec::transcode(&plain_path, &gz_path).unwrap();

    let plain = SessionPreview::load(&plain_path).expect("plain loads");
    let gz = SessionPreview::load(&gz_path);

    assert!(gz.is_some(), "gzip session should load a preview");
    let gz = gz.unwrap();
    assert_eq!(gz.duration_secs, plain.duration_secs);
    assert_eq!(gz.marker_count, plain.marker_count);
    assert_eq!(gz.styled_preview.len(), plain.styled_preview.len());
    assert!(gz.uncompressed_size.is_some());
    assert!(plain.uncompressed_size.is_none());
}
