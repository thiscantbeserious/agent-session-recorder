//! Integration tests for the play command

use std::process::Command;
use tempfile::TempDir;

use crate::helpers::{fixtures_dir, load_fixture};

/// Helper to run agr CLI and capture output
fn run_agr(args: &[&str]) -> (String, String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_agr"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("Failed to execute agr");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    (stdout, stderr, exit_code)
}

// ============================================================================
// Help Output Tests
// ============================================================================

#[test]
fn play_help_shows_usage() {
    let (stdout, _stderr, exit_code) = run_agr(&["play", "--help"]);

    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Play an asciicast recording"));
    assert!(stdout.contains("<FILE>"));
    assert!(stdout.contains("PLAYER CONTROLS"));
    assert!(stdout.contains("Space"));
    assert!(stdout.contains("Pause/resume"));
}

#[test]
fn snapshot_cli_help_play() {
    let (stdout, stderr, exit_code) = run_agr(&["play", "--help"]);
    let output = format!(
        "=== agr play --help ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout, stderr
    );
    insta::assert_snapshot!("cli_help_play", output);
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn play_no_arguments_shows_error() {
    let (_stdout, stderr, exit_code) = run_agr(&["play"]);

    assert_eq!(exit_code, 2);
    assert!(stderr.contains("required arguments"));
    assert!(stderr.contains("<FILE>"));
}

#[test]
fn play_nonexistent_file_shows_error() {
    let (_stdout, stderr, exit_code) = run_agr(&["play", "nonexistent.cast"]);

    assert_eq!(exit_code, 1);
    assert!(stderr.contains("File not found"));
    assert!(stderr.contains("nonexistent.cast"));
    assert!(stderr.contains("agr list"));
}

#[test]
fn play_nonexistent_file_with_path_shows_error() {
    let (_stdout, stderr, exit_code) = run_agr(&["play", "/some/path/to/missing.cast"]);

    assert_eq!(exit_code, 1);
    assert!(stderr.contains("File not found"));
}

// ============================================================================
// Path Resolution Tests
// ============================================================================

#[test]
fn play_with_absolute_path_exists_check() {
    // Create a temp file
    let temp_dir = TempDir::new().unwrap();
    let cast_path = temp_dir.path().join("test.cast");
    std::fs::write(&cast_path, load_fixture("sample.cast")).unwrap();

    // Try to play it - we can't test actual playback without a TTY,
    // but we can verify it doesn't fail with "File not found"
    let (_stdout, stderr, _exit_code) = run_agr(&["play", cast_path.to_str().unwrap()]);

    // Should NOT show "File not found" error
    assert!(
        !stderr.contains("File not found"),
        "Should find file at absolute path"
    );

    // It might fail for other reasons (no TTY) but that's OK
    // The important thing is that the file was found
}

// Note: Short format resolution is tested at the unit level in src/commands/mod.rs
// via resolve_file_path_tests. Integration tests cannot easily override the storage
// directory without config file manipulation.

#[test]
fn play_warns_for_non_cast_extension() {
    // Create a temp file with wrong extension
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    std::fs::write(&file_path, load_fixture("sample.cast")).unwrap();

    // Try to play it
    let (_stdout, stderr, _exit_code) = run_agr(&["play", file_path.to_str().unwrap()]);

    // Should show warning about extension (written to stderr)
    assert!(
        stderr.contains("Warning") || stderr.contains(".cast"),
        "Should warn about non-.cast extension"
    );
}

// ============================================================================
// CLI Parsing Tests (in main.rs, but we verify here)
// ============================================================================

#[test]
fn play_accepts_file_argument() {
    // Just verify the command parses correctly
    let fixture_path = fixtures_dir().join("sample.cast");
    let (_stdout, stderr, _exit_code) = run_agr(&["play", fixture_path.to_str().unwrap()]);

    // Should NOT show parsing errors
    assert!(
        !stderr.contains("unexpected argument"),
        "Command should parse file argument correctly"
    );
}
