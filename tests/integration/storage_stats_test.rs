//! Storage tests: session info and stats

use super::helpers::setup_test_sessions;
use super::helpers::storage::{create_test_config, create_test_manager, create_test_session};
use agr::files::codec::UncompressedSize;
use agr::storage::{format_uncompressed, SessionInfo};
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

// === Uncompressed size (header only, no decompress) ===

fn gz_claiming(claimed: u64, body: &[u8]) -> Vec<u8> {
    let mut extra = b"AG".to_vec();
    extra.extend_from_slice(&8u16.to_le_bytes());
    extra.extend_from_slice(&claimed.to_le_bytes());
    let mut enc = flate2::GzBuilder::new()
        .extra(extra)
        .write(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut enc, body).unwrap();
    enc.finish().unwrap()
}

fn put_gz(dir: &std::path::Path, agent: &str, name: &str, bytes: &[u8]) {
    std::fs::create_dir_all(dir.join(agent)).unwrap();
    std::fs::write(dir.join(agent).join(name), bytes).unwrap();
}

fn only_session(manager: &StorageManager) -> SessionInfo {
    manager.list_sessions(None).unwrap().remove(0)
}

#[test]
fn session_uncompressed_size_reads_header_without_decompressing() {
    let temp = TempDir::new().unwrap();
    let mut bytes = gz_claiming(4242, b"tiny");
    let len = bytes.len();
    bytes[len - 12..len - 8].fill(0xff); // corrupt the deflate body tail and CRC
    put_gz(temp.path(), "claude", "a.cast.gz", &bytes);
    let manager = StorageManager::new(create_test_config(&temp));
    let size = only_session(&manager).uncompressed_size().unwrap();
    assert_eq!(size, UncompressedSize::Exact(4242));
}

#[test]
fn stats_total_is_on_disk_and_uncompressed_total_is_summed() {
    let temp = TempDir::new().unwrap();
    let a = gz_claiming(1000, b"a");
    let b = gz_claiming(2000, b"b");
    put_gz(temp.path(), "claude", "a.cast.gz", &a);
    put_gz(temp.path(), "codex", "b.cast.gz", &b);
    create_test_session(temp.path(), "codex", "c.cast", "12345");
    let stats = StorageManager::new(create_test_config(&temp))
        .get_stats()
        .unwrap();
    assert_eq!(stats.total_size, (a.len() + b.len() + 5) as u64);
    assert_eq!(stats.uncompressed_total, UncompressedSize::Exact(3005));
}

#[test]
fn stats_uncompressed_total_flags_over_4gib() {
    let temp = TempDir::new().unwrap();
    put_gz(temp.path(), "claude", "a.cast.gz", &gz_claiming(10, b"a"));
    let mut foreign = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3];
    foreign.resize(10 + 4, 0);
    foreign.extend_from_slice(&[0; 4]);
    foreign.extend_from_slice(&1u32.to_le_bytes()); // wrapped ISIZE below on-disk size
    put_gz(temp.path(), "claude", "b.cast.gz", &foreign);
    let stats = StorageManager::new(create_test_config(&temp))
        .get_stats()
        .unwrap();
    assert_eq!(stats.uncompressed_total, UncompressedSize::Over4GiB);
    assert!(stats.summary().contains("4GiB+"), "{}", stats.summary());
}

#[test]
fn summary_shows_on_disk_and_uncompressed() {
    let temp = TempDir::new().unwrap();
    put_gz(temp.path(), "claude", "a.cast.gz", &gz_claiming(2048, b"a"));
    let summary = StorageManager::new(create_test_config(&temp))
        .get_stats()
        .unwrap()
        .summary();
    assert!(
        summary.contains("2 KiB uncompressed"),
        "expected '2 KiB uncompressed' in: {summary}"
    );
    assert!(
        summary.contains("on disk"),
        "expected 'on disk' in: {summary}"
    );
    assert!(
        summary.contains("1 total"),
        "expected '1 total' in: {summary}"
    );
}

#[test]
fn exceeds_threshold_compares_on_disk_size() {
    let temp = TempDir::new().unwrap();
    put_gz(
        temp.path(),
        "claude",
        "a.cast.gz",
        &gz_claiming(10 << 30, b"a"),
    );
    let mut config = create_test_config(&temp);
    config.storage.size_threshold_gb = Some(1.0);
    assert!(!StorageManager::new(config).exceeds_threshold().unwrap());
}

#[test]
fn stats_over_700_sessions_stay_header_only() {
    let temp = TempDir::new().unwrap();
    for i in 0..700 {
        let name = format!("s{i}.cast.gz");
        put_gz(temp.path(), "claude", &name, &gz_claiming(100, b"x"));
    }
    let start = std::time::Instant::now();
    let stats = StorageManager::new(create_test_config(&temp))
        .get_stats()
        .unwrap();
    eprintln!("get_stats over 700 files: {:?}", start.elapsed());
    assert_eq!(stats.session_count, 700);
    assert_eq!(stats.uncompressed_total, UncompressedSize::Exact(70_000));
}

#[test]
fn format_uncompressed_shows_human_or_4gib_plus() {
    assert_eq!(format_uncompressed(UncompressedSize::Exact(1024)), "1 KiB");
    assert_eq!(format_uncompressed(UncompressedSize::Over4GiB), "4GiB+");
}
