//! Storage tests: session info and stats

use super::helpers::setup_test_sessions;
use super::helpers::storage::{create_test_config, create_test_manager, create_test_session};
use agr::storage::SessionInfo;
use agr::{Config, StorageManager};
use chrono::Local;
use std::path::PathBuf;
use tempfile::TempDir;

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
fn stats_oldest_session_is_actually_oldest() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);

    // Create the "old" session first
    create_test_session(temp.path(), "claude", "old.cast", "old");

    // Sleep to ensure distinguishable mtime on filesystems with 1s granularity
    std::thread::sleep(std::time::Duration::from_millis(1100));

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

// === Size threshold (optional warning) ===

#[test]
fn exceeds_threshold_is_false_when_threshold_unset() {
    let (_temp_dir, manager) = create_test_manager();
    assert!(!manager.exceeds_threshold().unwrap());
}

#[test]
fn exceeds_threshold_is_true_when_size_above_configured_threshold() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_dir = setup_test_sessions(&temp_dir);
    let mut config = Config::default();
    config.storage.directory = sessions_dir.to_string_lossy().to_string();
    config.storage.size_threshold_gb = Some(0.0);
    let manager = StorageManager::new(config);
    assert!(manager.exceeds_threshold().unwrap());
}
