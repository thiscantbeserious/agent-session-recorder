//! Transform round-trip, threshold resolution and help tests.

use tempfile::TempDir;

use agr::AsciicastFile;

use super::transform_test::{
    create_cast_file, run_agr, sample_cast_with_all_event_types, sample_cast_with_idle_time_limit,
    sample_cast_with_long_pauses, sample_cast_with_unicode,
};

// ============================================================================
// Round-Trip Tests
// ============================================================================

#[test]
fn transform_then_parse_produces_valid_asciicast() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "roundtrip.cast", sample_cast_with_long_pauses());

    let (_, stderr, exit_code) =
        run_agr(&["optimize", "--remove-silence", cast_path.to_str().unwrap()]);

    assert_eq!(exit_code, 0, "Transform should succeed. stderr: {}", stderr);

    // Parse the transformed file
    let result = AsciicastFile::parse(&cast_path);
    assert!(
        result.is_ok(),
        "Transformed file should be valid asciicast: {:?}",
        result.err()
    );

    let cast = result.unwrap();
    assert_eq!(cast.header.version, 3);
    assert_eq!(cast.events.len(), 3);
}

#[test]
fn transform_preserves_header_fields() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "header.cast", sample_cast_with_all_event_types());

    // Parse before transform
    let original = AsciicastFile::parse(&cast_path).unwrap();

    let (_, stderr, exit_code) =
        run_agr(&["optimize", "--remove-silence", cast_path.to_str().unwrap()]);

    assert_eq!(exit_code, 0, "Transform should succeed. stderr: {}", stderr);

    // Parse after transform
    let transformed = AsciicastFile::parse(&cast_path).unwrap();

    // Verify header fields are preserved
    assert_eq!(transformed.header.version, original.header.version);
    // Compare term fields individually since TermInfo doesn't implement PartialEq
    let orig_term = original.header.term.as_ref().unwrap();
    let trans_term = transformed.header.term.as_ref().unwrap();
    assert_eq!(trans_term.cols, orig_term.cols);
    assert_eq!(trans_term.rows, orig_term.rows);
    assert_eq!(transformed.header.title, original.header.title);
    assert_eq!(transformed.header.timestamp, original.header.timestamp);
}

#[test]
fn transform_preserves_all_event_data_fields() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "events.cast", sample_cast_with_all_event_types());

    // Parse before transform
    let original = AsciicastFile::parse(&cast_path).unwrap();
    let original_data: Vec<_> = original.events.iter().map(|e| e.data.clone()).collect();
    let original_types: Vec<_> = original.events.iter().map(|e| e.event_type).collect();

    let (_, stderr, exit_code) =
        run_agr(&["optimize", "--remove-silence", cast_path.to_str().unwrap()]);

    assert_eq!(exit_code, 0, "Transform should succeed. stderr: {}", stderr);

    // Parse after transform
    let transformed = AsciicastFile::parse(&cast_path).unwrap();

    // Verify event count is preserved
    assert_eq!(
        transformed.events.len(),
        original.events.len(),
        "Event count should be preserved"
    );

    // Verify event data and types are preserved
    for (i, event) in transformed.events.iter().enumerate() {
        assert_eq!(
            event.data, original_data[i],
            "Event {} data should be preserved",
            i
        );
        assert_eq!(
            event.event_type, original_types[i],
            "Event {} type should be preserved",
            i
        );
    }
}

#[test]
fn transform_preserves_unicode_content() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "unicode.cast", sample_cast_with_unicode());

    // Parse before transform
    let original = AsciicastFile::parse(&cast_path).unwrap();
    let original_data: Vec<_> = original.events.iter().map(|e| e.data.clone()).collect();

    let (_, stderr, exit_code) =
        run_agr(&["optimize", "--remove-silence", cast_path.to_str().unwrap()]);

    assert_eq!(exit_code, 0, "Transform should succeed. stderr: {}", stderr);

    // Parse after transform
    let transformed = AsciicastFile::parse(&cast_path).unwrap();

    // Verify Unicode content is preserved
    for (i, event) in transformed.events.iter().enumerate() {
        assert_eq!(
            event.data, original_data[i],
            "Event {} Unicode data should be preserved",
            i
        );
    }
}

#[test]
fn transform_multiple_times_cumulative_effect() {
    let temp_dir = TempDir::new().unwrap();
    // Start with very long pauses
    let content = r#"{"version":3,"term":{"cols":80,"rows":24}}
[0.5,"o","first\r\n"]
[100.0,"o","second\r\n"]
[50.0,"o","third\r\n"]"#;
    let cast_path = create_cast_file(&temp_dir, "multi.cast", content);

    // First transform with 10s threshold
    let (_, stderr, exit_code) = run_agr(&[
        "optimize",
        "--remove-silence=10.0",
        cast_path.to_str().unwrap(),
    ]);
    assert_eq!(
        exit_code, 0,
        "First transform should succeed. stderr: {}",
        stderr
    );

    // After first: [0.5, 10.0, 10.0]
    let after_first = AsciicastFile::parse(&cast_path).unwrap();
    assert!((after_first.events[1].time - 10.0).abs() < 0.001);
    assert!((after_first.events[2].time - 10.0).abs() < 0.001);

    // Second transform with 2s threshold
    let (_, stderr, exit_code) = run_agr(&[
        "optimize",
        "--remove-silence=2.0",
        cast_path.to_str().unwrap(),
    ]);
    assert_eq!(
        exit_code, 0,
        "Second transform should succeed. stderr: {}",
        stderr
    );

    // After second: [0.5, 2.0, 2.0]
    let after_second = AsciicastFile::parse(&cast_path).unwrap();
    assert!(
        (after_second.events[0].time - 0.5).abs() < 0.001,
        "First event should still be 0.5"
    );
    assert!(
        (after_second.events[1].time - 2.0).abs() < 0.001,
        "Second event should be clamped to 2.0"
    );
    assert!(
        (after_second.events[2].time - 2.0).abs() < 0.001,
        "Third event should be clamped to 2.0"
    );
}

// ============================================================================
// Threshold Resolution Tests
// ============================================================================

#[test]
fn transform_uses_header_idle_time_limit_when_no_cli_threshold() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(
        &temp_dir,
        "header_threshold.cast",
        sample_cast_with_idle_time_limit(),
    );

    let (stdout, stderr, exit_code) =
        run_agr(&["optimize", "--remove-silence", cast_path.to_str().unwrap()]);

    assert_eq!(exit_code, 0, "Transform should succeed. stderr: {}", stderr);
    assert!(
        stdout.contains("1.5") || stdout.contains("header"),
        "Should use header idle_time_limit. stdout: {}",
        stdout
    );

    let transformed = AsciicastFile::parse(&cast_path).unwrap();
    // The 10.0s pause should be clamped to 1.5s (from header)
    assert!(
        (transformed.events[1].time - 1.5).abs() < 0.001,
        "Should use header idle_time_limit of 1.5s, got {}",
        transformed.events[1].time
    );
}

#[test]
fn transform_cli_threshold_overrides_header() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(
        &temp_dir,
        "override.cast",
        sample_cast_with_idle_time_limit(),
    );

    let (stdout, stderr, exit_code) = run_agr(&[
        "optimize",
        "--remove-silence=3.0",
        cast_path.to_str().unwrap(),
    ]);

    assert_eq!(exit_code, 0, "Transform should succeed. stderr: {}", stderr);
    assert!(
        stdout.contains("3.0") || stdout.contains("CLI"),
        "Should use CLI threshold. stdout: {}",
        stdout
    );

    let transformed = AsciicastFile::parse(&cast_path).unwrap();
    // Should use 3.0 from CLI, not 1.5 from header
    assert!(
        (transformed.events[1].time - 3.0).abs() < 0.001,
        "Should use CLI threshold of 3.0s, got {}",
        transformed.events[1].time
    );
}

// ============================================================================
// CLI Help Tests
// ============================================================================

#[test]
fn transform_help_shows_remove_silence_option() {
    let (stdout, stderr, exit_code) = run_agr(&["optimize", "--help"]);

    assert_eq!(exit_code, 0, "Help should succeed. stderr: {}", stderr);
    assert!(
        stdout.contains("remove-silence"),
        "Help should mention --remove-silence. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("output") || stdout.contains("OUTPUT"),
        "Help should mention --output. stdout: {}",
        stdout
    );
}
