//! Storage tests: listing sessions and fixture-based manager behavior

use super::helpers::storage::{
    create_dir_config, create_test_config, create_test_manager, create_test_session,
};
use agr::{Config, StorageManager};
use std::fs;
use std::io::Write;
use tempfile::TempDir;

// === Fixture-based tests (existing) ===

#[test]
fn list_all_sessions() {
    let (_temp_dir, manager) = create_test_manager();

    let sessions = manager.list_sessions(None).unwrap();
    assert_eq!(sessions.len(), 2);
}

#[test]
fn list_sessions_by_agent() {
    let (_temp_dir, manager) = create_test_manager();

    let claude_sessions = manager.list_sessions(Some("claude")).unwrap();
    assert_eq!(claude_sessions.len(), 1);
    assert_eq!(claude_sessions[0].agent, "claude");

    let codex_sessions = manager.list_sessions(Some("codex")).unwrap();
    assert_eq!(codex_sessions.len(), 1);
    assert_eq!(codex_sessions[0].agent, "codex");
}

#[test]
fn get_stats_counts_sessions() {
    let (_temp_dir, manager) = create_test_manager();

    let stats = manager.get_stats().unwrap();
    assert_eq!(stats.session_count, 2);
    assert_eq!(stats.sessions_by_agent.get("claude"), Some(&1));
    assert_eq!(stats.sessions_by_agent.get("codex"), Some(&1));
}

#[test]
fn get_stats_calculates_total_size() {
    let (_temp_dir, manager) = create_test_manager();

    let stats = manager.get_stats().unwrap();
    assert!(stats.total_size > 0);
}

#[test]
fn delete_sessions_removes_files() {
    let (_temp_dir, manager) = create_test_manager();

    let sessions = manager.list_sessions(None).unwrap();
    let to_delete = vec![sessions[0].clone()];

    let freed = manager.delete_sessions(&to_delete).unwrap();
    assert!(freed > 0);

    let remaining = manager.list_sessions(None).unwrap();
    assert_eq!(remaining.len(), 1);
}

#[test]
fn empty_storage_returns_empty_stats() {
    let temp_dir = TempDir::new().unwrap();
    let mut config = Config::default();
    config.storage.directory = temp_dir.path().to_string_lossy().to_string();

    let manager = StorageManager::new(config);
    let stats = manager.get_stats().unwrap();

    assert_eq!(stats.session_count, 0);
    assert_eq!(stats.total_size, 0);
}

#[test]
fn ensure_agent_dir_creates_directory() {
    let temp_dir = TempDir::new().unwrap();
    let mut config = Config::default();
    config.storage.directory = temp_dir.path().to_string_lossy().to_string();

    let manager = StorageManager::new(config);
    let agent_dir = manager.ensure_agent_dir("new-agent").unwrap();

    assert!(agent_dir.exists());
    assert!(agent_dir.ends_with("new-agent"));
}

#[test]
fn storage_stats_summary_is_human_readable() {
    let (_temp_dir, manager) = create_test_manager();

    let stats = manager.get_stats().unwrap();
    let summary = stats.summary();

    assert!(summary.contains("Agent Sessions"));
    assert!(summary.contains("Sessions:"));
    assert!(summary.contains("total"));
}

// === Inline tests (merged from src/storage.rs) ===

#[test]
fn list_sessions_returns_empty_for_new_storage() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    let sessions = manager.list_sessions(None).unwrap();
    assert!(sessions.is_empty());
}

#[test]
fn list_sessions_finds_cast_files() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    create_test_session(temp.path(), "claude", "test.cast", "test content");

    let sessions = manager.list_sessions(None).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].agent, "claude");
    assert_eq!(sessions[0].filename, "test.cast");
}

#[test]
fn list_sessions_filters_by_agent_inline() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    create_test_session(temp.path(), "claude", "session1.cast", "content");
    create_test_session(temp.path(), "codex", "session2.cast", "content");

    let claude_sessions = manager.list_sessions(Some("claude")).unwrap();
    assert_eq!(claude_sessions.len(), 1);
    assert_eq!(claude_sessions[0].agent, "claude");

    let codex_sessions = manager.list_sessions(Some("codex")).unwrap();
    assert_eq!(codex_sessions.len(), 1);
    assert_eq!(codex_sessions[0].agent, "codex");
}

#[test]
fn list_sessions_ignores_non_cast_files() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    create_test_session(temp.path(), "claude", "test.cast", "content");
    create_test_session(temp.path(), "claude", "test.txt", "content");
    create_test_session(temp.path(), "claude", "test.json", "content");

    let sessions = manager.list_sessions(None).unwrap();
    assert_eq!(sessions.len(), 1);
}

#[test]
fn get_stats_calculates_correctly() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    create_test_session(temp.path(), "claude", "session1.cast", "content1");
    create_test_session(temp.path(), "claude", "session2.cast", "content2");
    create_test_session(temp.path(), "codex", "session3.cast", "content3");

    let stats = manager.get_stats().unwrap();
    assert_eq!(stats.session_count, 3);
    assert_eq!(stats.sessions_by_agent.get("claude"), Some(&2));
    assert_eq!(stats.sessions_by_agent.get("codex"), Some(&1));
}

#[test]
fn delete_sessions_removes_files_inline() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    create_test_session(temp.path(), "claude", "session.cast", "content");

    let sessions = manager.list_sessions(None).unwrap();
    assert_eq!(sessions.len(), 1);

    manager.delete_sessions(&sessions).unwrap();

    let sessions_after = manager.list_sessions(None).unwrap();
    assert!(sessions_after.is_empty());
}

#[test]
fn ensure_storage_dir_creates_directory() {
    let temp = TempDir::new().unwrap();
    let mut config = create_test_config(&temp);
    config.storage.directory = temp.path().join("sessions").to_string_lossy().to_string();
    let manager = StorageManager::new(config);

    let dir = manager.ensure_storage_dir().unwrap();
    assert!(dir.exists());
}

#[test]
fn ensure_agent_dir_creates_directory_inline() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    let dir = manager.ensure_agent_dir("test-agent").unwrap();
    assert!(dir.exists());
    assert!(dir.ends_with("test-agent"));
}

// === Moved from src/storage/mod.rs ===

// ========================================================================
// Backup file filtering tests
// ========================================================================

/// Test that list_sessions excludes .bak files from the file list.
///
/// This is a critical regression test - .bak files are backups created
/// by the transform feature and should never appear in the session list.
#[test]
fn list_sessions_excludes_bak_files() {
    let dir = TempDir::new().unwrap();
    let storage_dir = dir.path();

    // Create agent directory
    let agent_dir = storage_dir.join("claude");
    fs::create_dir_all(&agent_dir).unwrap();

    // Create a valid .cast file
    let cast_file = agent_dir.join("session.cast");
    let mut f = fs::File::create(&cast_file).unwrap();
    writeln!(f, r#"{{"version":3}}"#).unwrap();
    writeln!(f, r#"[0.1,"o","hello"]"#).unwrap();

    // Create a .cast.bak file (backup)
    let bak_file = agent_dir.join("session.cast.bak");
    let mut f = fs::File::create(&bak_file).unwrap();
    writeln!(f, r#"{{"version":3}}"#).unwrap();
    writeln!(f, r#"[0.1,"o","original"]"#).unwrap();

    // Create another .bak file without .cast prefix
    let other_bak = agent_dir.join("other.bak");
    let mut f = fs::File::create(&other_bak).unwrap();
    writeln!(f, "some backup data").unwrap();

    // List sessions
    let config = create_dir_config(storage_dir);
    let manager = StorageManager::new(config);
    let sessions = manager.list_sessions(None).unwrap();

    // Should only find the .cast file, not any .bak files
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].filename, "session.cast");

    // Verify .bak files still exist on disk (possibly migrated to hidden format) but aren't listed.
    // The startup sweep in StorageManager::new() renames old-format `session.cast.bak` to
    // hidden `.session.cast.bak`, so check either location.
    let hidden_bak = agent_dir.join(".session.cast.bak");
    assert!(bak_file.exists() || hidden_bak.exists());
    assert!(other_bak.exists());
}

/// Test that list_sessions only includes files with .cast extension.
#[test]
fn list_sessions_only_includes_cast_extension() {
    let dir = TempDir::new().unwrap();
    let storage_dir = dir.path();

    // Create agent directory
    let agent_dir = storage_dir.join("claude");
    fs::create_dir_all(&agent_dir).unwrap();

    // Create files with various extensions
    let extensions = [
        "session.cast",     // Valid - should be included
        "session.cast.bak", // Backup - should be excluded
        "session.txt",      // Wrong extension - excluded
        "session.json",     // Wrong extension - excluded
        "session",          // No extension - excluded
    ];

    for filename in &extensions {
        let path = agent_dir.join(filename);
        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, "test content").unwrap();
    }

    // List sessions
    let config = create_dir_config(storage_dir);
    let manager = StorageManager::new(config);
    let sessions = manager.list_sessions(None).unwrap();

    // Should only find session.cast
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].filename, "session.cast");
}

/// Test that list_sessions works correctly with empty directory.
#[test]
fn list_sessions_empty_directory() {
    let dir = TempDir::new().unwrap();
    let storage_dir = dir.path();

    let config = create_dir_config(storage_dir);
    let manager = StorageManager::new(config);
    let sessions = manager.list_sessions(None).unwrap();

    assert!(sessions.is_empty());
}

/// Test that list_sessions filters by agent correctly.
#[test]
fn list_sessions_filters_by_agent() {
    let dir = TempDir::new().unwrap();
    let storage_dir = dir.path();

    // Create two agent directories
    let claude_dir = storage_dir.join("claude");
    let codex_dir = storage_dir.join("codex");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::create_dir_all(&codex_dir).unwrap();

    // Create files in each
    let mut f = fs::File::create(claude_dir.join("claude_session.cast")).unwrap();
    writeln!(f, "claude data").unwrap();

    let mut f = fs::File::create(codex_dir.join("codex_session.cast")).unwrap();
    writeln!(f, "codex data").unwrap();

    let config = create_dir_config(storage_dir);
    let manager = StorageManager::new(config);

    // List all sessions
    let all_sessions = manager.list_sessions(None).unwrap();
    assert_eq!(all_sessions.len(), 2);

    // List only claude sessions
    let claude_sessions = manager.list_sessions(Some("claude")).unwrap();
    assert_eq!(claude_sessions.len(), 1);
    assert_eq!(claude_sessions[0].agent, "claude");

    // List only codex sessions
    let codex_sessions = manager.list_sessions(Some("codex")).unwrap();
    assert_eq!(codex_sessions.len(), 1);
    assert_eq!(codex_sessions[0].agent, "codex");
}
