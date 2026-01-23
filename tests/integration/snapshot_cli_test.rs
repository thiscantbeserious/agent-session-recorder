//! CLI output snapshot tests
//!
//! Tests the actual CLI binary output for reproducibility.

use std::process::Command;

/// Helper to run agr CLI and capture output
fn run_agr(args: &[&str]) -> (String, String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_agr"))
        .args(args)
        .env("NO_COLOR", "1") // Disable colors for consistent snapshots
        .output()
        .expect("Failed to execute agr");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    (stdout, stderr, exit_code)
}

/// Helper to run agr CLI with colors enabled
fn run_agr_with_colors(args: &[&str]) -> (String, String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_agr"))
        .args(args)
        .env_remove("NO_COLOR") // Ensure colors are enabled
        .env("FORCE_COLOR", "1") // Force colors even without TTY
        .output()
        .expect("Failed to execute agr");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    (stdout, stderr, exit_code)
}

// ============================================================================
// Help Output Snapshots
// ============================================================================

#[test]
fn snapshot_cli_help_main() {
    let (stdout, stderr, exit_code) = run_agr(&["--help"]);
    let output = format!(
        "=== agr --help ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout, stderr
    );
    insta::assert_snapshot!("cli_help_main", output);
}

#[test]
fn snapshot_cli_help_record() {
    let (stdout, stderr, exit_code) = run_agr(&["record", "--help"]);
    let output = format!(
        "=== agr record --help ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout, stderr
    );
    insta::assert_snapshot!("cli_help_record", output);
}

#[test]
fn snapshot_cli_help_analyze() {
    let (stdout, stderr, exit_code) = run_agr(&["analyze", "--help"]);
    let output = format!(
        "=== agr analyze --help ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout, stderr
    );
    insta::assert_snapshot!("cli_help_analyze", output);
}

#[test]
fn snapshot_cli_help_agents() {
    let (stdout, stderr, exit_code) = run_agr(&["agents", "--help"]);
    let output = format!(
        "=== agr agents --help ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout, stderr
    );
    insta::assert_snapshot!("cli_help_agents", output);
}

#[test]
fn snapshot_cli_help_shell() {
    let (stdout, stderr, exit_code) = run_agr(&["shell", "--help"]);
    let output = format!(
        "=== agr shell --help ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout, stderr
    );
    insta::assert_snapshot!("cli_help_shell", output);
}

#[test]
fn snapshot_cli_help_config() {
    let (stdout, stderr, exit_code) = run_agr(&["config", "--help"]);
    let output = format!(
        "=== agr config --help ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout, stderr
    );
    insta::assert_snapshot!("cli_help_config", output);
}

#[test]
fn snapshot_cli_help_marker() {
    let (stdout, stderr, exit_code) = run_agr(&["marker", "--help"]);
    let output = format!(
        "=== agr marker --help ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout, stderr
    );
    insta::assert_snapshot!("cli_help_marker", output);
}

#[test]
fn snapshot_cli_help_list() {
    let (stdout, stderr, exit_code) = run_agr(&["list", "--help"]);
    let output = format!(
        "=== agr list --help ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout, stderr
    );
    insta::assert_snapshot!("cli_help_list", output);
}

#[test]
fn snapshot_cli_help_status() {
    let (stdout, stderr, exit_code) = run_agr(&["status", "--help"]);
    let output = format!(
        "=== agr status --help ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout, stderr
    );
    insta::assert_snapshot!("cli_help_status", output);
}

#[test]
fn snapshot_cli_help_cleanup() {
    let (stdout, stderr, exit_code) = run_agr(&["cleanup", "--help"]);
    let output = format!(
        "=== agr cleanup --help ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout, stderr
    );
    insta::assert_snapshot!("cli_help_cleanup", output);
}

// ============================================================================
// Error Output Snapshots
// ============================================================================

#[test]
fn snapshot_cli_no_subcommand() {
    let (stdout, stderr, exit_code) = run_agr(&[]);
    let output = format!(
        "=== agr (no args) ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout, stderr
    );
    insta::assert_snapshot!("cli_no_subcommand", output);
}

#[test]
fn snapshot_cli_invalid_subcommand() {
    let (stdout, stderr, exit_code) = run_agr(&["nonexistent"]);
    let output = format!(
        "=== agr nonexistent ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout, stderr
    );
    insta::assert_snapshot!("cli_invalid_subcommand", output);
}

// ============================================================================
// Version Output Snapshot
// ============================================================================

#[test]
fn snapshot_cli_version() {
    let (stdout, stderr, exit_code) = run_agr(&["--version"]);
    // Extract just the format, replace version number with placeholder
    let stdout_normalized = stdout
        .lines()
        .map(|line| {
            if line.starts_with("agr ") {
                "agr <VERSION>".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let output = format!(
        "=== agr --version ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout_normalized, stderr
    );
    insta::assert_snapshot!("cli_version", output);
}

// ============================================================================
// Colored Help Output Snapshots (with ANSI codes)
// ============================================================================

#[test]
fn snapshot_cli_help_main_colored() {
    let (stdout, stderr, exit_code) = run_agr_with_colors(&["--help"]);
    // Escape ANSI codes for readable snapshot
    let stdout_escaped = escape_ansi_for_snapshot(&stdout);
    let output = format!(
        "=== agr --help (colored) ===\nExit code: {}\n\n--- stdout (ANSI escaped) ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout_escaped, stderr
    );
    insta::assert_snapshot!("cli_help_main_colored", output);
}

/// Escape ANSI codes to make them visible in snapshots
fn escape_ansi_for_snapshot(s: &str) -> String {
    s.replace("\x1b[", "ESC[")
}
