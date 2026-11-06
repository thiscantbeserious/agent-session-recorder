//! Tests for `apply_transforms`, backup path helpers and `TransformResult`

use agr::asciicast::{
    apply_transforms, remove_silence, AsciicastFile, Event, Header, TransformResult,
};
use agr::files::backup::{backup_path_for, has_backup};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// ========================================================================
// Helper Functions
// ========================================================================

pub fn create_test_cast_file(dir: &TempDir, name: &str, events: Vec<Event>) -> PathBuf {
    let path = dir.path().join(name);
    let mut file = AsciicastFile::new(Header {
        version: 3,
        width: Some(80),
        height: Some(24),
        term: None,
        timestamp: None,
        duration: None,
        title: None,
        command: None,
        env: None,
        idle_time_limit: None,
    });
    file.events = events;
    file.write(&path).unwrap();
    path
}

fn create_test_cast_with_idle_limit(
    dir: &TempDir,
    name: &str,
    idle_time_limit: f64,
    events: Vec<Event>,
) -> PathBuf {
    let path = dir.path().join(name);
    let mut file = AsciicastFile::new(Header {
        version: 3,
        width: Some(80),
        height: Some(24),
        term: None,
        timestamp: None,
        duration: None,
        title: None,
        command: None,
        env: None,
        idle_time_limit: Some(idle_time_limit),
    });
    file.events = events;
    file.write(&path).unwrap();
    path
}

// ========================================================================
// backup_path_for tests
// ========================================================================

#[test]
fn backup_path_appends_bak_extension() {
    let path = Path::new("/some/path/recording.cast");
    let backup = backup_path_for(path);
    assert_eq!(backup, PathBuf::from("/some/path/.recording.cast.bak"));
}

#[test]
fn backup_path_handles_relative_path() {
    let path = Path::new("recording.cast");
    let backup = backup_path_for(path);
    assert_eq!(backup, PathBuf::from(".recording.cast.bak"));
}

#[test]
fn backup_path_already_dotted_filename() {
    let path = Path::new("/dir/.hidden.cast");
    let backup = backup_path_for(path);
    // No double dot — already hidden
    assert_eq!(backup, PathBuf::from("/dir/.hidden.cast.bak"));
}

// ========================================================================
// has_backup tests
// ========================================================================

#[test]
fn has_backup_returns_false_when_no_backup() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(&dir, "test.cast", vec![Event::output(0.1, "hello")]);

    assert!(!has_backup(&path));
}

#[test]
fn has_backup_returns_true_when_backup_exists() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(&dir, "test.cast", vec![Event::output(0.1, "hello")]);

    // Create backup manually
    let backup = backup_path_for(&path);
    fs::copy(&path, &backup).unwrap();

    assert!(has_backup(&path));
}

// ========================================================================
// apply_transforms tests
// ========================================================================

#[test]
fn apply_transforms_creates_backup_when_none_exists() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(
        &dir,
        "test.cast",
        vec![
            Event::output(0.1, "hello"),
            Event::output(5.0, "world"), // 5s gap will be reduced
        ],
    );

    let result = apply_transforms(&path).unwrap();

    assert!(result.backup_created);
    assert!(result.backup_path.is_some());
    assert!(has_backup(&path));
}

#[test]
fn apply_transforms_does_not_overwrite_existing_backup() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(
        &dir,
        "test.cast",
        vec![
            Event::output(0.1, "original"),
            Event::output(5.0, "content"),
        ],
    );

    // Create initial backup with known content
    let backup = backup_path_for(&path);
    let mut backup_file = fs::File::create(&backup).unwrap();
    backup_file.write_all(b"ORIGINAL_BACKUP").unwrap();

    let result = apply_transforms(&path).unwrap();

    // Should not have created new backup
    assert!(!result.backup_created);

    // Backup should still have original content
    let backup_content = fs::read_to_string(&backup).unwrap();
    assert_eq!(backup_content, "ORIGINAL_BACKUP");
}

#[test]
fn apply_transforms_reduces_duration_with_silence() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(
        &dir,
        "test.cast",
        vec![
            Event::output(0.1, "hello"),
            Event::output(10.0, "after pause"), // 10s gap exceeds 2.0s threshold
            Event::output(0.2, "end"),
        ],
    );

    let result = apply_transforms(&path).unwrap();

    // Original: 0.1 + 10.0 + 0.2 = 10.3s
    assert!((result.original_duration - 10.3).abs() < 0.001);

    // New: 0.1 + 2.0 + 0.2 = 2.3s (10s capped to 2s)
    assert!((result.new_duration - 2.3).abs() < 0.001);

    // Time saved: 8.0s
    assert!((result.time_saved() - 8.0).abs() < 0.001);
}

#[test]
fn apply_transforms_uses_header_idle_time_limit() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_with_idle_limit(
        &dir,
        "test.cast",
        1.0, // Use 1.0s threshold from header
        vec![
            Event::output(0.1, "hello"),
            Event::output(3.0, "after pause"), // 3s gap exceeds 1.0s threshold
            Event::output(0.2, "end"),
        ],
    );

    let result = apply_transforms(&path).unwrap();

    // Original: 0.1 + 3.0 + 0.2 = 3.3s
    assert!((result.original_duration - 3.3).abs() < 0.001);

    // New: 0.1 + 1.0 + 0.2 = 1.3s (3s capped to 1s from header)
    assert!((result.new_duration - 1.3).abs() < 0.001);
}

#[test]
fn apply_transforms_no_change_when_no_silence() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(
        &dir,
        "test.cast",
        vec![
            Event::output(0.1, "fast"),
            Event::output(0.2, "typing"),
            Event::output(0.1, "here"),
        ],
    );

    let result = apply_transforms(&path).unwrap();

    // No gaps exceed threshold, so duration unchanged
    assert!((result.original_duration - result.new_duration).abs() < 0.001);
    assert!((result.time_saved()).abs() < 0.001);
}

#[test]
fn apply_transforms_modifies_file_in_place() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(
        &dir,
        "test.cast",
        vec![
            Event::output(0.1, "hello"),
            Event::output(10.0, "world"), // Will be capped to 2.0
        ],
    );

    apply_transforms(&path).unwrap();

    // Re-read the file and check duration
    let modified = AsciicastFile::parse(&path).unwrap();
    assert!((modified.duration() - 2.1).abs() < 0.001); // 0.1 + 2.0
}

// ========================================================================
// TransformResult tests
// ========================================================================

#[test]
fn transform_result_time_saved() {
    let result = TransformResult {
        original_duration: 100.0,
        new_duration: 30.0,
        backup_path: None,
        backup_created: false,
    };

    assert!((result.time_saved() - 70.0).abs() < 0.001);
}

#[test]
fn transform_result_percent_saved() {
    let result = TransformResult {
        original_duration: 100.0,
        new_duration: 30.0,
        backup_path: None,
        backup_created: false,
    };

    assert!((result.percent_saved() - 70.0).abs() < 0.001);
}

#[test]
fn transform_result_percent_saved_zero_duration() {
    let result = TransformResult {
        original_duration: 0.0,
        new_duration: 0.0,
        backup_path: None,
        backup_created: false,
    };

    assert!((result.percent_saved()).abs() < 0.001);
}

// ========================================================================
// Dual-format has_backup tests (Stage 3)
// ========================================================================

#[test]
fn has_backup_finds_new_format() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(&dir, "test.cast", vec![Event::output(0.1, "hello")]);
    let hidden_backup = backup_path_for(&path);
    fs::copy(&path, &hidden_backup).unwrap();

    assert!(has_backup(&path));
}

// ========================================================================
// remove_silence (no backup) and threshold guard
// ========================================================================

fn gap_events() -> Vec<Event> {
    vec![Event::output(0.1, "hello"), Event::output(5.0, "world")]
}

fn bak_files(dir: &TempDir) -> Vec<String> {
    fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".bak"))
        .collect()
}

#[test]
fn remove_silence_clamps_gap_to_default_threshold() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(&dir, "x.cast", gap_events());

    let result = remove_silence(&path).unwrap();

    assert!((result.new_duration - 2.1).abs() < 0.001);
    assert!((AsciicastFile::parse(&path).unwrap().duration() - 2.1).abs() < 0.001);
}

#[test]
fn remove_silence_uses_header_idle_time_limit() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_with_idle_limit(&dir, "x.cast", 1.0, gap_events());

    let result = remove_silence(&path).unwrap();

    assert!((result.new_duration - 1.1).abs() < 0.001);
}

#[test]
fn remove_silence_keeps_gzip_format() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(&dir, "x.cast.gz", gap_events());

    remove_silence(&path).unwrap();

    let bytes = fs::read(&path).unwrap();
    assert_eq!(&bytes[..2], &[0x1f, 0x8b]);
    assert!((AsciicastFile::parse(&path).unwrap().duration() - 2.1).abs() < 0.001);
}

#[test]
fn remove_silence_creates_no_backup_for_cast() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(&dir, "x.cast", gap_events());

    let result = remove_silence(&path).unwrap();

    assert!(bak_files(&dir).is_empty());
    assert!(result.backup_path.is_none());
    assert!(!result.backup_created);
}

#[test]
fn remove_silence_creates_no_backup_for_gz() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(&dir, "x.cast.gz", gap_events());

    let result = remove_silence(&path).unwrap();

    assert!(bak_files(&dir).is_empty());
    assert!(result.backup_path.is_none());
}

#[test]
fn remove_silence_leaves_orphaned_backup_untouched() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(&dir, "x.cast", gap_events());
    let backup = backup_path_for(&path);
    fs::write(&backup, b"ORPHAN").unwrap();

    remove_silence(&path).unwrap();

    assert_eq!(fs::read(&backup).unwrap(), b"ORPHAN");
}

#[test]
fn apply_transforms_still_creates_backup_for_gz() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(&dir, "x.cast.gz", gap_events());

    let result = apply_transforms(&path).unwrap();

    assert!(result.backup_created);
    assert_eq!(bak_files(&dir), vec![".x.cast.gz.bak".to_string()]);
}

#[test]
fn zero_and_negative_thresholds_error_and_leave_file_untouched() {
    // NaN/inf cannot be written as JSON numbers (serialised as null), so only 0 and negative.
    for bad in [0.0, -1.0] {
        for use_backup in [true, false] {
            let dir = TempDir::new().unwrap();
            let path = create_test_cast_with_idle_limit(&dir, "x.cast", bad, gap_events());
            let before = fs::read(&path).unwrap();

            let result = if use_backup {
                apply_transforms(&path)
            } else {
                remove_silence(&path)
            };

            assert!(result.is_err(), "threshold {bad} backup {use_backup}");
            assert_eq!(fs::read(&path).unwrap(), before);
            assert!(bak_files(&dir).is_empty());
        }
    }
}

#[test]
fn unrepresentable_thresholds_error_and_leave_file_untouched() {
    for bad in ["1e999", "-1e999"] {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("x.cast");
        let body = format!(
            "{{\"version\":3,\"term\":{{\"cols\":80,\"rows\":24}},\"idle_time_limit\":{bad}}}\n[0.1,\"o\",\"a\"]\n"
        );
        fs::write(&path, &body).unwrap();

        assert!(remove_silence(&path).is_err(), "threshold {bad}");
        assert!(apply_transforms(&path).is_err(), "threshold {bad}");
        assert_eq!(fs::read_to_string(&path).unwrap(), body);
        assert!(bak_files(&dir).is_empty());
    }
}
