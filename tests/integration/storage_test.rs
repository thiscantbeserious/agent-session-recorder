//! Unit tests for storage module

use super::helpers::setup_test_sessions;

use agr::storage::{validate_cast_header, ImportError, SessionInfo};
use agr::{Config, StorageManager};
use chrono::Local;
use std::fs;
use std::io::Write;
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

#[test]
fn list_cast_files_short_sorted_by_mtime_descending() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create sessions with different modification times
    // Oldest file first
    create_test_session(temp.path(), "claude", "oldest.cast", "oldest content");

    // Sleep to ensure different modification times
    // Use 1100ms to handle filesystems with 1-second timestamp granularity
    std::thread::sleep(std::time::Duration::from_millis(1100));

    create_test_session(temp.path(), "claude", "middle.cast", "middle content");

    std::thread::sleep(std::time::Duration::from_millis(1100));

    // Newest file last
    create_test_session(temp.path(), "codex", "newest.cast", "newest content");

    // Get files - should be sorted by mtime descending (newest first)
    let files = manager.list_cast_files_short(None).unwrap();
    assert_eq!(files.len(), 3);

    // Most recent first
    assert_eq!(files[0], "codex/newest.cast", "Newest file should be first");
    assert_eq!(
        files[1], "claude/middle.cast",
        "Middle file should be second"
    );
    assert_eq!(files[2], "claude/oldest.cast", "Oldest file should be last");
}

// ========================================================================
// Auxiliary file cleanup tests (Stage 4)
// ========================================================================

#[test]
fn delete_sessions_removes_auxiliary_files_both_formats() {
    use agr::files::backup::{backup_path_for, old_backup_path_for};
    use agr::files::lock::{lock_path_for, old_lock_path_for};

    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create the .cast file
    let cast_path = create_test_session(temp.path(), "claude", "session.cast", "content");

    // Create all 4 auxiliary files
    let new_lock = lock_path_for(&cast_path);
    let old_lock = old_lock_path_for(&cast_path);
    let new_backup = backup_path_for(&cast_path);
    let old_backup = old_backup_path_for(&cast_path);

    fs::write(&new_lock, "new lock").unwrap();
    fs::write(&old_lock, "old lock").unwrap();
    fs::write(&new_backup, "new backup").unwrap();
    fs::write(&old_backup, "old backup").unwrap();

    // Verify all files exist before deletion
    assert!(cast_path.exists(), "cast file should exist before delete");
    assert!(
        new_lock.exists(),
        "new-format lock should exist before delete"
    );
    assert!(
        old_lock.exists(),
        "old-format lock should exist before delete"
    );
    assert!(
        new_backup.exists(),
        "new-format backup should exist before delete"
    );
    assert!(
        old_backup.exists(),
        "old-format backup should exist before delete"
    );

    // Delete the session
    let sessions = manager.list_sessions(None).unwrap();
    assert_eq!(sessions.len(), 1);
    manager.delete_sessions(&sessions).unwrap();

    // Verify all files are gone
    assert!(!cast_path.exists(), "cast file should be removed");
    assert!(!new_lock.exists(), "new-format lock should be removed");
    assert!(!old_lock.exists(), "old-format lock should be removed");
    assert!(!new_backup.exists(), "new-format backup should be removed");
    assert!(!old_backup.exists(), "old-format backup should be removed");
}

// ========================================================================
// Import functionality tests (Stage 2)
// ========================================================================

/// Helper to create a valid v3 cast file for testing
fn create_valid_v3_cast(path: &Path) {
    let mut file = fs::File::create(path).unwrap();
    writeln!(file, r#"{{"version":3,"width":80,"height":24}}"#).unwrap();
    writeln!(file, r#"[0.1,"o","test output"]"#).unwrap();
}

/// Helper to create a valid v2 cast file for testing
fn create_valid_v2_cast(path: &Path) {
    let mut file = fs::File::create(path).unwrap();
    writeln!(file, r#"{{"version":2,"width":80,"height":24}}"#).unwrap();
    writeln!(file, r#"[0.1,"o","test output"]"#).unwrap();
}

#[test]
fn validate_cast_header_valid_v3() {
    let temp = TempDir::new().unwrap();
    let cast_file = temp.path().join("test.cast");
    create_valid_v3_cast(&cast_file);

    let result = validate_cast_header(&cast_file);
    assert!(result.is_ok(), "Valid v3 file should pass validation");
}

#[test]
fn validate_cast_header_valid_v2() {
    let temp = TempDir::new().unwrap();
    let cast_file = temp.path().join("test.cast");
    create_valid_v2_cast(&cast_file);

    let result = validate_cast_header(&cast_file);
    assert!(result.is_ok(), "Valid v2 file should pass validation");
}

#[test]
fn validate_cast_header_missing_file() {
    let temp = TempDir::new().unwrap();
    let missing_file = temp.path().join("nonexistent.cast");

    let result = validate_cast_header(&missing_file);
    assert!(result.is_err(), "Missing file should fail validation");
    assert!(
        matches!(result.unwrap_err(), ImportError::NotFound(_)),
        "Should return NotFound error"
    );
}

#[test]
fn validate_cast_header_wrong_extension() {
    let temp = TempDir::new().unwrap();
    let txt_file = temp.path().join("test.txt");
    let mut file = fs::File::create(&txt_file).unwrap();
    writeln!(file, r#"{{"version":3}}"#).unwrap();

    let result = validate_cast_header(&txt_file);
    assert!(result.is_err(), "Wrong extension should fail validation");
    assert!(
        matches!(result.unwrap_err(), ImportError::WrongExtension(_)),
        "Should return WrongExtension error"
    );
}

#[test]
fn validate_cast_header_invalid_json() {
    let temp = TempDir::new().unwrap();
    let cast_file = temp.path().join("invalid.cast");
    let mut file = fs::File::create(&cast_file).unwrap();
    writeln!(file, "not valid json").unwrap();

    let result = validate_cast_header(&cast_file);
    assert!(result.is_err(), "Invalid JSON should fail validation");
    assert!(
        matches!(result.unwrap_err(), ImportError::InvalidFormat(_)),
        "Should return InvalidFormat error"
    );
}

#[test]
fn validate_cast_header_no_version() {
    let temp = TempDir::new().unwrap();
    let cast_file = temp.path().join("no_version.cast");
    let mut file = fs::File::create(&cast_file).unwrap();
    writeln!(file, r#"{{"width":80,"height":24}}"#).unwrap();

    let result = validate_cast_header(&cast_file);
    assert!(
        result.is_err(),
        "JSON without version should fail validation"
    );
    assert!(
        matches!(result.unwrap_err(), ImportError::InvalidFormat(_)),
        "Should return InvalidFormat error"
    );
}

#[test]
fn import_cast_file_success() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create a valid cast file to import
    let source = temp.path().join("source.cast");
    create_valid_v3_cast(&source);

    // Import it
    let result = manager.import_cast_file(&source, "claude");
    assert!(result.is_ok(), "Import should succeed");

    let imported_path = result.unwrap();
    assert!(imported_path.exists(), "Imported file should exist");
    assert!(
        imported_path.to_string_lossy().contains("claude"),
        "Should be in claude directory"
    );
    assert!(
        imported_path.ends_with("source.cast"),
        "Should preserve filename"
    );
}

#[test]
fn import_cast_file_creates_agent_dir() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create a valid cast file
    let source = temp.path().join("test.cast");
    create_valid_v3_cast(&source);

    // Import into non-existing agent directory
    let result = manager.import_cast_file(&source, "new-agent");
    assert!(result.is_ok(), "Import should create agent directory");

    let imported_path = result.unwrap();
    assert!(imported_path.exists(), "Imported file should exist");

    let agent_dir = temp.path().join("new-agent");
    assert!(agent_dir.exists(), "Agent directory should be created");
}

#[test]
fn import_cast_file_conflict_resolution() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create source file
    let source = temp.path().join("session.cast");
    create_valid_v3_cast(&source);

    // First import
    let first_import = manager.import_cast_file(&source, "claude").unwrap();
    assert!(first_import.ends_with("session.cast"));

    // Second import of same file - should get -1 suffix
    let second_import = manager.import_cast_file(&source, "claude").unwrap();
    assert!(
        second_import.to_string_lossy().contains("session-1.cast"),
        "Second import should have -1 suffix, got: {}",
        second_import.display()
    );

    // Both files should exist
    assert!(first_import.exists());
    assert!(second_import.exists());
}

#[test]
fn import_cast_file_preserves_filename() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create source with specific filename
    let source = temp.path().join("my-special-session.cast");
    create_valid_v3_cast(&source);

    // Import it
    let imported = manager.import_cast_file(&source, "claude").unwrap();

    // Should preserve the original filename
    assert!(
        imported.ends_with("my-special-session.cast"),
        "Should preserve original filename"
    );
}

#[test]
fn resolve_filename_conflict_no_conflict() {
    let temp = TempDir::new().unwrap();

    // Create test using the internal helper via import
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    let source = temp.path().join("unique.cast");
    create_valid_v3_cast(&source);

    let imported = manager.import_cast_file(&source, "claude").unwrap();
    assert!(
        imported.ends_with("unique.cast"),
        "Should use original filename when no conflict"
    );
}

#[test]
fn resolve_filename_conflict_with_existing() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    let source = temp.path().join("conflict.cast");
    create_valid_v3_cast(&source);

    // First import
    let first = manager.import_cast_file(&source, "claude").unwrap();
    assert!(first.ends_with("conflict.cast"));

    // Second import - should get -1 suffix
    let second = manager.import_cast_file(&source, "claude").unwrap();
    assert!(second.to_string_lossy().contains("conflict-1.cast"));

    // Third import - should get -2 suffix
    let third = manager.import_cast_file(&source, "claude").unwrap();
    assert!(third.to_string_lossy().contains("conflict-2.cast"));
}

#[test]
fn stats_oldest_session_is_actually_oldest() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);

    // Create the "old" session first
    create_test_session(temp.path(), "claude", "old.cast", "old");

    // Sleep briefly so filesystem mtime differs
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Create the "new" session second (guaranteed newer mtime)
    create_test_session(temp.path(), "claude", "new.cast", "new");

    let manager = StorageManager::new(config);
    let stats = manager.get_stats().unwrap();
    let oldest = stats.oldest_session.expect("should have oldest session");

    assert_eq!(
        oldest.filename, "old.cast",
        "oldest should be old.cast, got {}",
        oldest.filename
    );
}
