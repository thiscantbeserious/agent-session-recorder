//! Performance tests for transform CLI with large files.
//!
//! These tests verify CLI behavior with large asciicast files,
//! including file operations, parsing, and round-trip integrity.
//!
//! Run with: `cargo test --test performance`

use tempfile::TempDir;

use agr::AsciicastFile;

use crate::helpers::{create_cast_file, generate_large_cast, run_agr};

// ========================================================================
// Large File CLI Tests
// ========================================================================

/// Test: Large file (1M events) transforms via CLI successfully
///
/// Performance requirement: 100MB file in < 30 seconds (including file I/O)
#[test]
fn transform_large_file_via_cli_completes() {
    let temp_dir = TempDir::new().unwrap();

    // Generate approximately 100MB worth of events
    // Each event is roughly 30-50 bytes, so ~2-3 million events for 100MB
    // For reasonable test time, use 1 million events
    let large_content = generate_large_cast(1_000_000);
    let cast_path = create_cast_file(&temp_dir, "large.cast", &large_content);

    let start = std::time::Instant::now();
    let (stdout, stderr, exit_code) = run_agr(&[
        "transform",
        "--remove-silence",
        cast_path.to_str().unwrap(),
    ]);
    let duration = start.elapsed();

    assert_eq!(
        exit_code, 0,
        "Large file transform should succeed. stderr: {}",
        stderr
    );
    println!(
        "Large file (1M events) CLI transform completed in {:.2}s",
        duration.as_secs_f64()
    );

    // Should complete in reasonable time (< 30s including file I/O)
    assert!(
        duration.as_secs() < 30,
        "Transform took too long: {:.2}s",
        duration.as_secs_f64()
    );

    // Verify some output indicating success
    assert!(
        stdout.contains("Duration") || stdout.contains("threshold") || stdout.contains("modified"),
        "Should show success message. stdout: {}",
        stdout
    );
}

/// Test: Large file (100K events) produces valid and playable output
///
/// Verifies that the transformed file is a valid asciicast file.
#[test]
fn transform_large_file_output_is_valid_and_playable() {
    let temp_dir = TempDir::new().unwrap();

    // Use smaller count for this test since we need to verify parsing
    let large_content = generate_large_cast(100_000);
    let cast_path = create_cast_file(&temp_dir, "large_valid.cast", &large_content);

    let (_, stderr, exit_code) = run_agr(&[
        "transform",
        "--remove-silence",
        cast_path.to_str().unwrap(),
    ]);

    assert_eq!(
        exit_code, 0,
        "Large file transform should succeed. stderr: {}",
        stderr
    );

    // Verify the output is valid asciicast
    let result = AsciicastFile::parse(&cast_path);
    assert!(
        result.is_ok(),
        "Transformed large file should be valid: {:?}",
        result.err()
    );

    let cast = result.unwrap();
    assert_eq!(cast.header.version, 3);
    assert_eq!(cast.events.len(), 100_000);

    // Verify some events were actually transformed (the ones with 5.0s intervals)
    let clamped_count = cast.events.iter().filter(|e| (e.time - 2.0).abs() < 0.001).count();
    assert!(
        clamped_count > 0,
        "Some events should have been clamped to 2.0s"
    );
}
