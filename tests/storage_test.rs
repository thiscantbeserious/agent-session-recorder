//! Integration tests for storage module

mod helpers;

use asr::{Config, StorageManager};
use helpers::setup_test_sessions;
use tempfile::TempDir;

fn create_test_manager() -> (TempDir, StorageManager) {
    let temp_dir = TempDir::new().unwrap();
    let sessions_dir = setup_test_sessions(&temp_dir);

    let mut config = Config::default();
    config.storage.directory = sessions_dir.to_string_lossy().to_string();

    let manager = StorageManager::new(config);
    (temp_dir, manager)
}

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
