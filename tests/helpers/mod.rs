//! Test helper utilities

#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Get the path to the fixtures directory
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Load a fixture file's contents
pub fn load_fixture(name: &str) -> String {
    let path = fixtures_dir().join(name);
    fs::read_to_string(&path).unwrap_or_else(|_| panic!("Failed to load fixture: {}", name))
}

/// Create a temporary directory with a copy of a fixture
pub fn temp_fixture(name: &str) -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let fixture_content = load_fixture(name);
    let temp_path = temp_dir.path().join(name);
    fs::write(&temp_path, fixture_content).expect("Failed to write temp fixture");
    (temp_dir, temp_path)
}

/// Create a test config directory
pub fn setup_test_config(temp_dir: &TempDir) -> PathBuf {
    let config_dir = temp_dir.path().join(".config").join("asr");
    fs::create_dir_all(&config_dir).expect("Failed to create config dir");
    config_dir
}

/// Create a test session directory with sample files
pub fn setup_test_sessions(temp_dir: &TempDir) -> PathBuf {
    let sessions_dir = temp_dir.path().join("recorded_agent_sessions");

    // Create agent directories with sample sessions
    let claude_dir = sessions_dir.join("claude");
    fs::create_dir_all(&claude_dir).expect("Failed to create claude dir");
    fs::write(
        claude_dir.join("20250119-100000.cast"),
        load_fixture("sample.cast"),
    )
    .expect("Failed to write sample session");

    let codex_dir = sessions_dir.join("codex");
    fs::create_dir_all(&codex_dir).expect("Failed to create codex dir");
    fs::write(
        codex_dir.join("20250118-120000.cast"),
        load_fixture("with_markers.cast"),
    )
    .expect("Failed to write marked session");

    sessions_dir
}
