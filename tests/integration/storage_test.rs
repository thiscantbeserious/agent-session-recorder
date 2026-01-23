//! Unit tests for storage module

use super::helpers::setup_test_sessions;

use agr::storage::SessionInfo;
use agr::{Config, StorageManager};
use chrono::Local;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// === Fixture-based tests (existing) ===

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

// === Inline tests (merged from src/storage.rs) ===

fn create_test_config(temp_dir: &TempDir) -> Config {
    let mut config = Config::default();
    config.storage.directory = temp_dir.path().to_string_lossy().to_string();
    config
}

fn create_test_session(dir: &Path, agent: &str, filename: &str, content: &str) -> PathBuf {
    let agent_dir = dir.join(agent);
    fs::create_dir_all(&agent_dir).unwrap();
    let path = agent_dir.join(filename);
    fs::write(&path, content).unwrap();
    path
}

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

#[test]
fn session_info_size_human_formats_correctly() {
    let session = SessionInfo {
        path: PathBuf::from("/test"),
        agent: "test".to_string(),
        filename: "test.cast".to_string(),
        size: 1024 * 1024, // 1 MiB
        modified: Local::now(),
        age_days: 0,
        age_hours: 0,
        age_minutes: 0,
    };

    let human = session.size_human();
    assert!(human.contains("MiB") || human.contains("MB"));
}

#[test]
fn session_info_format_age_minutes_only() {
    // Less than 1 hour: show minutes only "  45m"
    let session = SessionInfo {
        path: PathBuf::from("/test"),
        agent: "test".to_string(),
        filename: "test.cast".to_string(),
        size: 1024,
        modified: Local::now(),
        age_days: 0,
        age_hours: 0,
        age_minutes: 45,
    };
    assert_eq!(session.format_age(), "  45m");
}

#[test]
fn session_info_format_age_same_day_shows_hours() {
    // Same day, more than 1 hour: show hours only
    let session = SessionInfo {
        path: PathBuf::from("/test"),
        agent: "test".to_string(),
        filename: "test.cast".to_string(),
        size: 1024,
        modified: Local::now(),
        age_days: 0,
        age_hours: 5,
        age_minutes: 300,
    };
    assert_eq!(session.format_age(), "   5h");
}

#[test]
fn session_info_format_age_older_shows_days_only() {
    // Older than 1 day: show days only
    let session = SessionInfo {
        path: PathBuf::from("/test"),
        agent: "test".to_string(),
        filename: "test.cast".to_string(),
        size: 1024,
        modified: Local::now(),
        age_days: 3,
        age_hours: 75,
        age_minutes: 4500,
    };
    assert_eq!(session.format_age(), "   3d");
}

#[test]
fn session_info_format_age_just_created() {
    // Just created (0 minutes)
    let session = SessionInfo {
        path: PathBuf::from("/test"),
        agent: "test".to_string(),
        filename: "test.cast".to_string(),
        size: 1024,
        modified: Local::now(),
        age_days: 0,
        age_hours: 0,
        age_minutes: 0,
    };
    assert_eq!(session.format_age(), "   0m");
}

#[test]
fn stats_summary_shows_agent_breakdown() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create sessions for multiple agents
    create_test_session(temp.path(), "claude", "s1.cast", "content");
    create_test_session(temp.path(), "claude", "s2.cast", "content");
    create_test_session(temp.path(), "codex", "s3.cast", "content");

    let stats = manager.get_stats().unwrap();
    let summary = stats.summary();

    // Should show breakdown by agent
    assert!(
        summary.contains("claude: 2"),
        "Summary should show claude: 2, got: {}",
        summary
    );
    assert!(
        summary.contains("codex: 1"),
        "Summary should show codex: 1, got: {}",
        summary
    );
}

#[test]
fn stats_summary_shows_disk_percentage() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    create_test_session(temp.path(), "claude", "test.cast", "content");

    let stats = manager.get_stats().unwrap();
    let summary = stats.summary();

    // Should show disk percentage (even if small/zero for test)
    assert!(
        summary.contains("% of disk"),
        "Summary should show disk percentage, got: {}",
        summary
    );
}

#[test]
fn stats_summary_shows_oldest_session_age() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    create_test_session(temp.path(), "claude", "test.cast", "content");

    let stats = manager.get_stats().unwrap();
    let summary = stats.summary();

    // Should show oldest session info
    assert!(
        summary.contains("Oldest:"),
        "Summary should show oldest session, got: {}",
        summary
    );
    assert!(
        summary.contains("days ago") || summary.contains("0 days"),
        "Summary should show age in days, got: {}",
        summary
    );
}

#[test]
fn stats_summary_uses_human_readable_sizes() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create a session with known content size
    let content = "x".repeat(1024); // 1 KiB
    create_test_session(temp.path(), "claude", "test.cast", &content);

    let stats = manager.get_stats().unwrap();
    let summary = stats.summary();

    // Should use human-readable size format (KiB, MiB, etc.)
    assert!(
        summary.contains("KiB") || summary.contains("KB") || summary.contains("B"),
        "Summary should use human-readable size, got: {}",
        summary
    );
}

#[test]
fn stats_shows_total_session_count() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    create_test_session(temp.path(), "claude", "s1.cast", "content");
    create_test_session(temp.path(), "claude", "s2.cast", "content");
    create_test_session(temp.path(), "codex", "s3.cast", "content");

    let stats = manager.get_stats().unwrap();
    let summary = stats.summary();

    // Should show total count
    assert!(
        summary.contains("3 total"),
        "Summary should show '3 total', got: {}",
        summary
    );
}

#[test]
fn disk_percentage_is_calculated() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create a session
    create_test_session(temp.path(), "claude", "test.cast", "content");

    let stats = manager.get_stats().unwrap();

    // Disk percentage should be >= 0 (might be 0 for tiny files on large disk)
    assert!(
        stats.disk_percentage >= 0.0,
        "Disk percentage should be non-negative"
    );
}

#[test]
fn resolve_cast_path_handles_short_format() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create a test session
    let created_path = create_test_session(temp.path(), "claude", "session.cast", "content");

    // Resolve using short format
    let resolved = manager.resolve_cast_path("claude/session.cast");
    assert!(resolved.is_some(), "Should resolve agent/file.cast format");
    assert_eq!(resolved.unwrap(), created_path);
}

#[test]
fn resolve_cast_path_handles_absolute_path() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create a test session
    let created_path = create_test_session(temp.path(), "claude", "session.cast", "content");

    // Resolve using absolute path
    let resolved = manager.resolve_cast_path(&created_path.to_string_lossy());
    assert!(resolved.is_some(), "Should resolve absolute path");
    assert_eq!(resolved.unwrap(), created_path);
}

#[test]
fn resolve_cast_path_returns_none_for_missing_file() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Try to resolve non-existent file
    let resolved = manager.resolve_cast_path("claude/nonexistent.cast");
    assert!(resolved.is_none(), "Should return None for missing file");

    // Also test absolute path that doesn't exist
    let resolved = manager.resolve_cast_path("/nonexistent/path/file.cast");
    assert!(
        resolved.is_none(),
        "Should return None for missing absolute path"
    );
}

#[test]
fn list_cast_files_short_returns_correct_format() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create sessions for multiple agents
    create_test_session(temp.path(), "claude", "session1.cast", "content");
    create_test_session(temp.path(), "codex", "session2.cast", "content");

    let files = manager.list_cast_files_short(None).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.contains(&"claude/session1.cast".to_string()));
    assert!(files.contains(&"codex/session2.cast".to_string()));
}

#[test]
fn list_cast_files_short_filters_by_prefix() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create sessions for multiple agents
    create_test_session(temp.path(), "claude", "session1.cast", "content");
    create_test_session(temp.path(), "claude", "session2.cast", "content");
    create_test_session(temp.path(), "codex", "session3.cast", "content");

    // Filter by claude prefix
    let files = manager.list_cast_files_short(Some("claude/")).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|f| f.starts_with("claude/")));

    // Filter by partial filename
    let files = manager
        .list_cast_files_short(Some("claude/session1"))
        .unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0], "claude/session1.cast");
}

#[test]
fn find_cast_file_by_name_returns_match() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create a test session
    let created_path = create_test_session(temp.path(), "claude", "unique.cast", "content");

    // Should find the file by name only
    let found = manager.find_cast_file_by_name("unique.cast");
    assert!(found.is_some(), "Should find file by name");
    assert_eq!(found.unwrap(), created_path);
}

#[test]
fn find_cast_file_by_name_returns_none_for_missing() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create a session with different name
    create_test_session(temp.path(), "claude", "existing.cast", "content");

    // Should not find non-existent file
    let found = manager.find_cast_file_by_name("nonexistent.cast");
    assert!(found.is_none(), "Should return None for missing file");
}

#[test]
fn find_cast_file_by_name_handles_duplicates_returns_newest() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create same filename in multiple agents
    let _older = create_test_session(temp.path(), "claude", "shared.cast", "claude content");

    // Sleep briefly to ensure different modification times (100ms for CI reliability)
    std::thread::sleep(std::time::Duration::from_millis(100));

    let newer = create_test_session(temp.path(), "codex", "shared.cast", "codex content");

    // Should return the newest (most recently modified) one
    let found = manager.find_cast_file_by_name("shared.cast");
    assert!(found.is_some(), "Should find file");
    assert_eq!(found.unwrap(), newer, "Should return the newest file");
}

#[test]
fn find_cast_file_by_name_empty_storage() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // No sessions created
    let found = manager.find_cast_file_by_name("any.cast");
    assert!(found.is_none(), "Should return None for empty storage");
}

#[test]
fn find_cast_file_by_name_partial_match_not_supported() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create a session
    create_test_session(temp.path(), "claude", "my-session.cast", "content");

    // Partial name should not match (exact match required)
    let found = manager.find_cast_file_by_name("session");
    assert!(found.is_none(), "Partial name should not match");

    let found = manager.find_cast_file_by_name("my-session");
    assert!(found.is_none(), "Missing extension should not match");
}
