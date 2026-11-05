//! Backup, restore and rename keep the full session suffix (`.cast` and `.cast.gz`)

use super::helpers::storage::create_codec_session;
use agr::files::backup::{backup_path_for, create_backup, restore_from_backup};
use agr::files::filename::{rename_file, RenameError, MAX_FILENAME_LENGTH};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

const CAST: &str = "{\"version\":3,\"width\":80,\"height\":24}\n[0.1,\"o\",\"x\"]\n";
const SUFFIXES: [&str; 2] = [".cast", ".cast.gz"];

fn session(temp: &TempDir, stem: &str, suffix: &str) -> PathBuf {
    create_codec_session(temp.path(), "a", &format!("{stem}{suffix}"), CAST)
}

#[test]
fn create_backup_is_hidden_byte_copy() {
    for suffix in SUFFIXES {
        let temp = TempDir::new().unwrap();
        let path = session(&temp, "x", suffix);
        assert!(create_backup(&path).unwrap());
        let backup = backup_path_for(&path);
        assert!(backup.ends_with(format!("a/.x{suffix}.bak")), "{backup:?}");
        assert_eq!(fs::read(&backup).unwrap(), fs::read(&path).unwrap());
    }
}

#[test]
fn restore_restores_exact_bytes_and_removes_backup() {
    for suffix in SUFFIXES {
        let temp = TempDir::new().unwrap();
        let path = session(&temp, "x", suffix);
        let original = fs::read(&path).unwrap();
        create_backup(&path).unwrap();
        fs::write(&path, b"changed").unwrap();
        restore_from_backup(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(!backup_path_for(&path).exists());
    }
}

#[test]
fn restore_does_not_touch_cast_cast_tmp_sibling() {
    for suffix in SUFFIXES {
        let temp = TempDir::new().unwrap();
        let path = session(&temp, "x", suffix);
        let sibling = path.with_file_name("x.cast.cast.tmp");
        fs::write(&sibling, b"keep").unwrap();
        create_backup(&path).unwrap();
        restore_from_backup(&path).unwrap();
        assert!(sibling.exists(), "sibling consumed, suffix {suffix}");
        assert_eq!(fs::read(&sibling).unwrap(), b"keep");
    }
}

#[test]
fn rename_keeps_suffix_and_moves_backup() {
    for suffix in SUFFIXES {
        let temp = TempDir::new().unwrap();
        let path = session(&temp, "x", suffix);
        create_backup(&path).unwrap();
        let new_path = rename_file(&path, "y").unwrap();
        assert!(new_path.ends_with(format!("a/y{suffix}")), "{new_path:?}");
        assert!(new_path.exists() && !path.exists());
        assert!(backup_path_for(&new_path).ends_with(format!("a/.y{suffix}.bak")));
        assert!(backup_path_for(&new_path).exists());
        assert!(!backup_path_for(&path).exists());
    }
}

#[test]
fn rename_to_same_stem_is_unchanged() {
    for suffix in SUFFIXES {
        let temp = TempDir::new().unwrap();
        let path = session(&temp, "x", suffix);
        assert_eq!(rename_file(&path, "x").unwrap(), path);
    }
}

#[test]
fn rename_length_counts_full_suffix() {
    for suffix in SUFFIXES {
        let temp = TempDir::new().unwrap();
        let path = session(&temp, "x", suffix);
        let fits = "n".repeat(MAX_FILENAME_LENGTH - suffix.len());
        let too_long = "n".repeat(MAX_FILENAME_LENGTH - suffix.len() + 1);
        assert!(matches!(
            rename_file(&path, &too_long),
            Err(RenameError::TooLong)
        ));
        assert!(rename_file(&path, &fits).is_ok(), "suffix {suffix}");
    }
}

#[test]
fn apply_transforms_uses_shared_backup_path() {
    for suffix in SUFFIXES {
        let temp = TempDir::new().unwrap();
        let path = session(&temp, "x", suffix);
        let original = fs::read(&path).unwrap();
        let result = agr::asciicast::apply_transforms(&path).unwrap();
        assert!(result.backup_created, "suffix {suffix}");
        assert_eq!(result.backup_path, Some(backup_path_for(&path)));
        assert_eq!(fs::read(backup_path_for(&path)).unwrap(), original);
    }
}
