//! Shell completion output snapshot tests
//!
//! Tests the generated shell completion scripts for reproducibility.

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

// ============================================================================
// Shell Completion Script Snapshots
// ============================================================================

#[test]
fn snapshot_completions_zsh() {
    let (stdout, stderr, exit_code) = run_agr(&["completions", "--shell", "zsh"]);
    let output = format!(
        "=== agr completions --shell zsh ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout, stderr
    );
    insta::with_settings!({
        snapshot_path => "../integration/snapshots/completions"
    }, {
        insta::assert_snapshot!("completions_shell_zsh", output);
    });
}

#[test]
fn snapshot_completions_bash() {
    let (stdout, stderr, exit_code) = run_agr(&["completions", "--shell", "bash"]);
    let output = format!(
        "=== agr completions --shell bash ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout, stderr
    );
    insta::with_settings!({
        snapshot_path => "../integration/snapshots/completions"
    }, {
        insta::assert_snapshot!("completions_shell_bash", output);
    });
}

// ============================================================================
// Shell Init Script Snapshots (Dynamic Completions)
// ============================================================================

#[test]
fn snapshot_completions_shell_init_zsh() {
    let (stdout, stderr, exit_code) = run_agr(&["completions", "--shell-init", "zsh"]);
    let output = format!(
        "=== agr completions --shell-init zsh ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout, stderr
    );
    insta::with_settings!({
        snapshot_path => "../integration/snapshots/completions"
    }, {
        insta::assert_snapshot!("completions_shell_init_zsh", output);
    });
}

#[test]
fn snapshot_completions_shell_init_bash() {
    let (stdout, stderr, exit_code) = run_agr(&["completions", "--shell-init", "bash"]);
    let output = format!(
        "=== agr completions --shell-init bash ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout, stderr
    );
    insta::with_settings!({
        snapshot_path => "../integration/snapshots/completions"
    }, {
        insta::assert_snapshot!("completions_shell_init_bash", output);
    });
}

// ============================================================================
// Completions Help
// ============================================================================

#[test]
fn snapshot_completions_help() {
    let (stdout, stderr, exit_code) = run_agr(&["completions", "--help"]);
    let output = format!(
        "=== agr completions --help ===\nExit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
        exit_code, stdout, stderr
    );
    insta::with_settings!({
        snapshot_path => "../integration/snapshots/completions"
    }, {
        insta::assert_snapshot!("completions_help", output);
    });
}
