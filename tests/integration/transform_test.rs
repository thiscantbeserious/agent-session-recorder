//! Integration tests for the transform command.
//!
//! Tests end-to-end CLI behavior for silence removal transform,
//! including file operations, error handling, and round-trip integrity.

use std::fs;
use std::process::Command;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use tempfile::TempDir;

use agr::AsciicastFile;

/// Helper to run agr CLI and capture output
fn run_agr(args: &[&str]) -> (String, String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_agr"))
        .args(args)
        .env("NO_COLOR", "1") // Disable colors for consistent output
        .output()
        .expect("Failed to execute agr");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    (stdout, stderr, exit_code)
}

/// Create a valid asciicast file with configurable events.
fn create_cast_file(dir: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, content).expect("Failed to write cast file");
    path
}

/// Basic valid asciicast content with long pauses
fn sample_cast_with_long_pauses() -> &'static str {
    r#"{"version":3,"term":{"cols":80,"rows":24}}
[0.5,"o","$ echo hello\r\n"]
[10.0,"o","hello\r\n"]
[0.2,"o","$ "]"#
}

/// Cast content with idle_time_limit in header
fn sample_cast_with_idle_time_limit() -> &'static str {
    r#"{"version":3,"term":{"cols":80,"rows":24},"idle_time_limit":1.5}
[0.5,"o","$ echo hello\r\n"]
[10.0,"o","hello\r\n"]
[0.2,"o","$ "]"#
}

/// Cast with multiple event types
fn sample_cast_with_all_event_types() -> &'static str {
    r#"{"version":3,"term":{"cols":80,"rows":24},"title":"Test Recording","timestamp":1706123456}
[0.5,"o","$ echo hello\r\n"]
[0.1,"i","echo hello"]
[5.0,"m","Thinking marker"]
[0.3,"o","hello\r\n"]
[0.1,"r","100x50"]"#
}

/// Cast with Unicode content
fn sample_cast_with_unicode() -> &'static str {
    r#"{"version":3,"term":{"cols":80,"rows":24}}
[0.5,"o","Hello \u4e2d\u6587 \ud83d\ude00\r\n"]
[10.0,"o","\u4e16\u754c\r\n"]
[0.2,"o","$ "]"#
}

// ============================================================================
// File Operation Tests
// ============================================================================

#[test]
fn transform_inplace_modification_works() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "test.cast", sample_cast_with_long_pauses());

    // Run transform without --output (in-place)
    let (stdout, stderr, exit_code) =
        run_agr(&["optimize", "--remove-silence", cast_path.to_str().unwrap()]);

    assert_eq!(exit_code, 0, "Exit code should be 0. stderr: {}", stderr);
    assert!(
        stdout.contains("in-place") || stdout.contains("modified"),
        "Should indicate in-place modification. stdout: {}",
        stdout
    );

    // Verify the file was modified
    let modified = AsciicastFile::parse(&cast_path).unwrap();
    // The 10.0s pause should now be 2.0s (default threshold)
    assert!(
        (modified.events[1].time - 2.0).abs() < 0.001,
        "Event time should be clamped to 2.0s, got {}",
        modified.events[1].time
    );
}

#[test]
fn transform_output_preserves_original_file() {
    let temp_dir = TempDir::new().unwrap();
    let original_content = sample_cast_with_long_pauses();
    let cast_path = create_cast_file(&temp_dir, "original.cast", original_content);
    let output_path = temp_dir.path().join("output.cast");

    // Run transform with --output
    let (_, stderr, exit_code) = run_agr(&[
        "optimize",
        "--remove-silence",
        "--output",
        output_path.to_str().unwrap(),
        cast_path.to_str().unwrap(),
    ]);

    assert_eq!(exit_code, 0, "Exit code should be 0. stderr: {}", stderr);

    // Verify original file is unchanged
    let original_read = fs::read_to_string(&cast_path).unwrap();
    assert_eq!(
        original_read, original_content,
        "Original file should be unchanged"
    );

    // Verify output file exists and has transformed content
    assert!(output_path.exists(), "Output file should exist");
    let output_cast = AsciicastFile::parse(&output_path).unwrap();
    assert!(
        (output_cast.events[1].time - 2.0).abs() < 0.001,
        "Output should have transformed content"
    );
}

#[test]
fn transform_output_creates_new_file() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "source.cast", sample_cast_with_long_pauses());
    let output_path = temp_dir.path().join("new_output.cast");

    assert!(!output_path.exists(), "Output file should not exist yet");

    let (_, stderr, exit_code) = run_agr(&[
        "optimize",
        "--remove-silence",
        "--output",
        output_path.to_str().unwrap(),
        cast_path.to_str().unwrap(),
    ]);

    assert_eq!(exit_code, 0, "Exit code should be 0. stderr: {}", stderr);
    assert!(output_path.exists(), "Output file should be created");

    let output_cast = AsciicastFile::parse(&output_path).unwrap();
    assert_eq!(output_cast.events.len(), 3);
}

#[test]
fn transform_output_to_same_path_as_input_works() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "same.cast", sample_cast_with_long_pauses());
    let path_str = cast_path.to_str().unwrap();

    // Using --output with same path as input should work (effectively in-place)
    let (_, stderr, exit_code) = run_agr(&[
        "optimize",
        "--remove-silence",
        "--output",
        path_str,
        path_str,
    ]);

    assert_eq!(exit_code, 0, "Exit code should be 0. stderr: {}", stderr);

    let modified = AsciicastFile::parse(&cast_path).unwrap();
    assert!(
        (modified.events[1].time - 2.0).abs() < 0.001,
        "File should be transformed"
    );
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn transform_corrupt_json_file_clear_error() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "corrupt.cast", "{ not valid json at all");

    let (stdout, stderr, exit_code) =
        run_agr(&["optimize", "--remove-silence", cast_path.to_str().unwrap()]);

    assert_ne!(
        exit_code, 0,
        "Exit code should be non-zero for corrupt file"
    );
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        combined.to_lowercase().contains("error")
            || combined.to_lowercase().contains("failed")
            || combined.to_lowercase().contains("invalid"),
        "Should show error message. Output: {}",
        combined
    );
}

#[test]
fn transform_truncated_file_clear_error() {
    let temp_dir = TempDir::new().unwrap();
    // Create a truly truncated file mid-JSON (incomplete event line)
    let truncated_path = create_cast_file(
        &temp_dir,
        "truncated.cast",
        r#"{"version":3,"term":{"cols":80,"rows":24}}
[0.5,"o","incomplete"#,
    );

    let (stdout, stderr, exit_code) = run_agr(&[
        "optimize",
        "--remove-silence",
        truncated_path.to_str().unwrap(),
    ]);

    assert_ne!(
        exit_code, 0,
        "Exit code should be non-zero for truncated file"
    );
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        combined.to_lowercase().contains("error")
            || combined.to_lowercase().contains("failed")
            || combined.to_lowercase().contains("invalid")
            || combined.to_lowercase().contains("parse"),
        "Should show error message. Output: {}",
        combined
    );

    // Verify file is not modified (still truncated)
    let content = fs::read_to_string(&truncated_path).unwrap();
    assert!(
        content.contains("incomplete"),
        "File should not be modified on error"
    );
}

#[test]
fn transform_missing_header_clear_error() {
    let temp_dir = TempDir::new().unwrap();
    // No header, just events
    let cast_path = create_cast_file(
        &temp_dir,
        "no_header.cast",
        r#"[0.5,"o","$ echo hello\r\n"]
[0.1,"o","hello\r\n"]"#,
    );

    let (stdout, stderr, exit_code) =
        run_agr(&["optimize", "--remove-silence", cast_path.to_str().unwrap()]);

    assert_ne!(
        exit_code, 0,
        "Exit code should be non-zero for missing header"
    );
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        combined.to_lowercase().contains("error")
            || combined.to_lowercase().contains("failed")
            || combined.to_lowercase().contains("header"),
        "Should show error about missing header. Output: {}",
        combined
    );
}

#[test]
fn transform_file_not_found_clear_error() {
    let (stdout, stderr, exit_code) = run_agr(&[
        "optimize",
        "--remove-silence",
        "/nonexistent/path/to/file.cast",
    ]);

    assert_ne!(
        exit_code, 0,
        "Exit code should be non-zero for missing file"
    );
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        combined.to_lowercase().contains("not found")
            || combined.to_lowercase().contains("error")
            || combined.to_lowercase().contains("no such"),
        "Should show file not found error. Output: {}",
        combined
    );
}

#[cfg(unix)]
#[test]
fn transform_permission_denied_clear_error() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "readonly.cast", sample_cast_with_long_pauses());

    // Make directory read-only (atomic write needs directory write permission
    // to create temp file and rename)
    let dir_path = temp_dir.path();
    let original_perms = fs::metadata(dir_path).unwrap().permissions();
    let mut readonly_perms = original_perms.clone();
    readonly_perms.set_mode(0o555);
    fs::set_permissions(dir_path, readonly_perms).unwrap();

    let (stdout, stderr, exit_code) =
        run_agr(&["optimize", "--remove-silence", cast_path.to_str().unwrap()]);

    // Restore permissions for cleanup
    fs::set_permissions(dir_path, original_perms).unwrap();

    assert_ne!(
        exit_code, 0,
        "Exit code should be non-zero for read-only directory"
    );
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        combined.to_lowercase().contains("permission")
            || combined.to_lowercase().contains("denied")
            || combined.to_lowercase().contains("error")
            || combined.to_lowercase().contains("failed"),
        "Should show permission error. Output: {}",
        combined
    );
}

#[test]
fn transform_invalid_threshold_clear_error_before_file_ops() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "test.cast", sample_cast_with_long_pauses());
    let original_content = fs::read_to_string(&cast_path).unwrap();

    // Test with negative threshold
    let (stdout, stderr, exit_code) = run_agr(&[
        "optimize",
        "--remove-silence=-1.0",
        cast_path.to_str().unwrap(),
    ]);

    assert_ne!(
        exit_code, 0,
        "Exit code should be non-zero for invalid threshold"
    );
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        combined.to_lowercase().contains("positive")
            || combined.to_lowercase().contains("invalid")
            || combined.to_lowercase().contains("error"),
        "Should show error about invalid threshold. Output: {}",
        combined
    );

    // Verify file is unchanged
    let current_content = fs::read_to_string(&cast_path).unwrap();
    assert_eq!(
        current_content, original_content,
        "File should not be modified on threshold validation error"
    );
}

#[test]
fn transform_invalid_threshold_zero() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "test.cast", sample_cast_with_long_pauses());

    let (stdout, stderr, exit_code) = run_agr(&[
        "optimize",
        "--remove-silence=0",
        cast_path.to_str().unwrap(),
    ]);

    assert_ne!(
        exit_code, 0,
        "Exit code should be non-zero for zero threshold"
    );
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        combined.to_lowercase().contains("positive") || combined.to_lowercase().contains("error"),
        "Should show error about zero threshold. Output: {}",
        combined
    );
}

#[test]
fn transform_invalid_threshold_nan() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "test.cast", sample_cast_with_long_pauses());

    let (stdout, stderr, exit_code) = run_agr(&[
        "optimize",
        "--remove-silence=notanumber",
        cast_path.to_str().unwrap(),
    ]);

    assert_ne!(
        exit_code, 0,
        "Exit code should be non-zero for NaN threshold"
    );
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        combined.to_lowercase().contains("invalid")
            || combined.to_lowercase().contains("number")
            || combined.to_lowercase().contains("error"),
        "Should show error about invalid number. Output: {}",
        combined
    );
}

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

// ============================================================================
// Lock Enforcement Tests
// ============================================================================

#[test]
fn transform_refuses_locked_file() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "locked.cast", sample_cast_with_long_pauses());
    agr::files::lock::create_lock(&cast_path).unwrap();

    let (_stdout, stderr, exit_code) =
        run_agr(&["optimize", "--remove-silence", cast_path.to_str().unwrap()]);

    assert_ne!(exit_code, 0, "Should refuse locked file");
    assert!(
        stderr.to_lowercase().contains("locked") || stderr.to_lowercase().contains("recording"),
        "Should mention lock. stderr: {}",
        stderr
    );
}

#[test]
fn transform_cleans_stale_lock_and_proceeds() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "stale.cast", sample_cast_with_long_pauses());
    let lock_path = agr::files::lock::lock_path_for(&cast_path);
    std::fs::write(
        &lock_path,
        r#"{"pid":999999999,"started":"2025-01-01T00:00:00Z"}"#,
    )
    .unwrap();

    let (_stdout, _stderr, exit_code) =
        run_agr(&["optimize", "--remove-silence", cast_path.to_str().unwrap()]);

    assert_eq!(exit_code, 0, "Should succeed after cleaning stale lock");
    assert!(!lock_path.exists(), "Stale lock should be cleaned up");
}

#[test]
fn analyze_refuses_locked_file() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(
        &temp_dir,
        "locked_analyze.cast",
        sample_cast_with_long_pauses(),
    );
    agr::files::lock::create_lock(&cast_path).unwrap();

    let (_stdout, stderr, exit_code) = run_agr(&["analyze", cast_path.to_str().unwrap()]);

    assert_ne!(exit_code, 0, "Should refuse locked file");
    assert!(
        stderr.to_lowercase().contains("locked") || stderr.to_lowercase().contains("recording"),
        "Should mention lock. stderr: {}",
        stderr
    );
}

#[test]
fn marker_add_refuses_locked_file() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(
        &temp_dir,
        "locked_marker.cast",
        sample_cast_with_long_pauses(),
    );
    agr::files::lock::create_lock(&cast_path).unwrap();

    let (_stdout, stderr, exit_code) = run_agr(&[
        "marker",
        "add",
        cast_path.to_str().unwrap(),
        "0.5",
        "test marker",
    ]);

    assert_ne!(exit_code, 0, "Should refuse locked file");
    assert!(
        stderr.to_lowercase().contains("locked") || stderr.to_lowercase().contains("recording"),
        "Should mention lock. stderr: {}",
        stderr
    );
}

#[test]
fn transform_requires_transform_flag() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "test.cast", sample_cast_with_long_pauses());

    // Run transform without any transform flag
    let (stdout, stderr, exit_code) = run_agr(&["optimize", cast_path.to_str().unwrap()]);

    assert_ne!(exit_code, 0, "Should fail without optimization flag");
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        combined.contains("remove-silence") || combined.contains("No optimization"),
        "Should indicate need for optimization flag. Output: {}",
        combined
    );
}
