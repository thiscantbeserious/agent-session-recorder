//! Tests for the analyze rename helpers.

use super::*;
use agr::files::backup::backup_path_for;
use tempfile::TempDir;

#[test]
fn current_stem_strips_full_session_suffix() {
    assert_eq!(current_stem(Path::new("/a/x.cast.gz")), "x");
    assert_eq!(current_stem(Path::new("/a/x.cast")), "x");
}

#[test]
fn apply_rename_keeps_gz_suffix_and_moves_backup() {
    let dir = TempDir::new().unwrap();
    let old = dir.path().join("x.cast.gz");
    std::fs::write(&old, "gz").unwrap();
    std::fs::write(backup_path_for(&old), "bak").unwrap();
    let new = apply_rename(&old, "y").unwrap();
    assert_eq!(new, dir.path().join("y.cast.gz"));
    assert!(new.exists() && !old.exists());
    assert!(backup_path_for(&new).exists());
    assert!(!backup_path_for(&old).exists());
}

#[test]
fn target_path_keeps_suffix() {
    let p = target_path(Path::new("/a/x.cast.gz"), "y");
    assert_eq!(p, Path::new("/a/y.cast.gz"));
}
