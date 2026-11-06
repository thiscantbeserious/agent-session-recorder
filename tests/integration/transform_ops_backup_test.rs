//! Tests for `remove_silence` (no backup), threshold guard and dual-format `has_backup`

use super::transform_ops_test::{create_test_cast_file, create_test_cast_with_idle_limit};
use agr::asciicast::{apply_transforms, remove_silence, AsciicastFile, Event};
use agr::files::backup::{backup_path_for, has_backup};
use std::fs;
use tempfile::TempDir;

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
