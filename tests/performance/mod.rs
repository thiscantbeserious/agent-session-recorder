//! Performance test module.
//!
//! This module contains helpers shared across performance tests.

use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Helper to run agr CLI and capture output
pub fn run_agr(args: &[&str]) -> (String, String, i32) {
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
pub fn create_cast_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, content).expect("Failed to write cast file");
    path
}

/// Generate a large asciicast file with many events.
pub fn generate_large_cast(event_count: usize) -> String {
    let mut content = String::from(r#"{"version":3,"term":{"cols":80,"rows":24}}"#);
    content.push('\n');

    for i in 0..event_count {
        // Alternate between short and long intervals
        let time = if i % 100 == 0 { 5.0 } else { 0.1 };
        content.push_str(&format!(r#"[{},"o","output line {}\r\n"]"#, time, i));
        content.push('\n');
    }

    content
}
