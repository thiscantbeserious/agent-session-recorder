//! Storage tests: path resolution, lookup and auxiliary file cleanup

use super::helpers::storage::{create_codec_session, create_test_config, create_test_session};
use agr::StorageManager;
use std::fs;
use tempfile::TempDir;

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

// === Gzip sessions (Stage 7) ===

const CAST: &str = "{\"version\":3}\n[0.1,\"o\",\"x\"]\n";

fn manager_with(agent: &str, file: &str) -> (TempDir, StorageManager, std::path::PathBuf) {
    let temp = TempDir::new().unwrap();
    let manager = StorageManager::new(create_test_config(&temp));
    let path = create_codec_session(temp.path(), agent, file, CAST);
    (temp, manager, path)
}

#[test]
fn resolve_cast_path_finds_gzip_for_plain_name() {
    let (_t, manager, gz) = manager_with("claude", "x.cast.gz");
    assert_eq!(manager.resolve_cast_path("claude/x.cast"), Some(gz));
}

#[test]
fn resolve_cast_path_finds_plain_for_gzip_name() {
    let (_t, manager, plain) = manager_with("claude", "x.cast");
    assert_eq!(manager.resolve_cast_path("claude/x.cast.gz"), Some(plain));
}

#[test]
fn resolve_cast_path_absolute_falls_back_to_other_variant() {
    let (_t, manager, gz) = manager_with("claude", "x.cast.gz");
    let asked = gz.with_file_name("x.cast");
    assert_eq!(manager.resolve_cast_path(asked.to_str().unwrap()), Some(gz));
}

#[test]
fn find_cast_file_by_name_matches_either_variant() {
    let (_t, manager, gz) = manager_with("claude", "x.cast.gz");
    assert_eq!(manager.find_cast_file_by_name("x.cast"), Some(gz));
    let (_t2, manager2, plain) = manager_with("claude", "y.cast");
    assert_eq!(manager2.find_cast_file_by_name("y.cast.gz"), Some(plain));
}

#[test]
fn list_cast_files_short_includes_gzip_sessions() {
    let (_t, manager, _) = manager_with("claude", "x.cast.gz");
    assert_eq!(
        manager.list_cast_files_short(None).unwrap(),
        vec!["claude/x.cast.gz"]
    );
}
