use std::path::Path;

use agr::files::lock;
use agr::files::lock::old_lock_path_for;
use tempfile::TempDir;

#[test]
fn lock_path_for_appends_lock_extension() {
    let path = Path::new("/tmp/sessions/recording.cast");
    let lock_path = lock::lock_path_for(path);
    assert_eq!(lock_path, Path::new("/tmp/sessions/.recording.cast.lock"));
}

#[test]
fn lock_path_for_hidden_path_structure() {
    // Dot should be on the filename, not on the directory
    let path = Path::new("/dir/session.cast");
    let lock_path = lock::lock_path_for(path);
    assert_eq!(lock_path, Path::new("/dir/.session.cast.lock"));
    // Parent must remain /dir, not /.dir
    assert_eq!(lock_path.parent().unwrap(), Path::new("/dir"));
}

#[test]
fn lock_path_for_no_parent_directory() {
    // Edge case: cast file with no parent directory component
    let path = Path::new("session.cast");
    let lock_path = lock::lock_path_for(path);
    assert_eq!(lock_path, Path::new(".session.cast.lock"));
}

#[test]
fn create_lock_and_read_lock_round_trip() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("test.cast");
    lock::create_lock(&cast_path).unwrap();
    let info = lock::read_lock(&cast_path);
    assert!(info.is_some());
    let info = info.unwrap();
    assert_eq!(info.pid, std::process::id());
}

#[test]
fn read_lock_returns_none_when_no_lock_file() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("nonexistent.cast");
    assert!(lock::read_lock(&cast_path).is_none());
}

#[test]
fn read_lock_returns_none_for_stale_pid() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("stale.cast");
    let lock_path = lock::lock_path_for(&cast_path);
    std::fs::write(
        &lock_path,
        r#"{"pid":999999999,"started":"2025-01-01T00:00:00Z"}"#,
    )
    .unwrap();
    assert!(lock::read_lock(&cast_path).is_none());
}

#[test]
fn remove_lock_deletes_lock_file() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("cleanup.cast");
    lock::create_lock(&cast_path).unwrap();
    let lock_path = lock::lock_path_for(&cast_path);
    assert!(lock_path.exists());
    lock::remove_lock(&cast_path);
    assert!(!lock_path.exists());
}

#[test]
fn check_not_locked_succeeds_when_no_lock() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("unlocked.cast");
    assert!(lock::check_not_locked(&cast_path).is_ok());
}

#[test]
fn check_not_locked_errors_when_active_lock() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("locked.cast");
    lock::create_lock(&cast_path).unwrap();
    let result = lock::check_not_locked(&cast_path);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("locked") || err_msg.contains("recording"));
}

#[test]
fn check_not_locked_cleans_stale_lock_and_succeeds() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("stale_clean.cast");
    let lock_path = lock::lock_path_for(&cast_path);
    std::fs::write(
        &lock_path,
        r#"{"pid":999999999,"started":"2025-01-01T00:00:00Z"}"#,
    )
    .unwrap();
    assert!(lock_path.exists());
    assert!(lock::check_not_locked(&cast_path).is_ok());
    assert!(!lock_path.exists());
}

// --- dual-format: read_lock ---

#[test]
fn read_lock_finds_new_format() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("newformat.cast");
    lock::create_lock(&cast_path).unwrap();
    let result = lock::read_lock(&cast_path);
    assert!(
        result.is_some(),
        "read_lock should find new hidden-format lock"
    );
    assert_eq!(result.unwrap().pid, std::process::id());
}

// --- dual-format: remove_lock ---

#[test]
fn remove_lock_removes_both_formats() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("both.cast");
    let new_path = lock::lock_path_for(&cast_path);
    let old_path = old_lock_path_for(&cast_path);
    let info = serde_json::json!({"pid": std::process::id(), "started": "2025-01-01T00:00:00Z"});
    std::fs::write(&new_path, info.to_string()).unwrap();
    std::fs::write(&old_path, info.to_string()).unwrap();
    lock::remove_lock(&cast_path);
    assert!(!new_path.exists(), "new-format lock should be removed");
    assert!(!old_path.exists(), "old-format lock should be removed");
}

#[cfg(unix)]
#[test]
fn find_by_inode_locates_renamed_file() {
    use std::os::unix::fs::MetadataExt;
    let dir = TempDir::new().unwrap();
    let original = dir.path().join("original.cast");
    std::fs::write(&original, r#"{"version":3,"term":{"cols":80,"rows":24}}"#).unwrap();
    let inode = std::fs::metadata(&original).unwrap().ino();
    let renamed = dir.path().join("renamed.cast");
    std::fs::rename(&original, &renamed).unwrap();
    let found = lock::find_by_inode(dir.path(), inode);
    assert_eq!(found, Some(renamed));
}

#[test]
fn find_by_header_locates_file_by_first_line() {
    let dir = TempDir::new().unwrap();
    let header = r#"{"version":3,"term":{"cols":80,"rows":24},"title":"unique-session-42"}"#;
    let content = format!("{}\n[0.5,\"o\",\"hello\"]\n", header);
    let target = dir.path().join("target.cast");
    std::fs::write(&target, &content).unwrap();
    let other = dir.path().join("other.cast");
    std::fs::write(
        &other,
        r#"{"version":3,"term":{"cols":80,"rows":24},"title":"different"}"#,
    )
    .unwrap();
    let found = lock::find_by_header(dir.path(), header);
    assert_eq!(found, Some(target));
}

#[test]
fn find_by_header_returns_none_when_no_match() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("unrelated.cast");
    std::fs::write(&file, r#"{"version":3,"term":{"cols":80,"rows":24}}"#).unwrap();
    let found = lock::find_by_header(dir.path(), "no-such-header");
    assert!(found.is_none());
}

#[cfg(unix)]
#[test]
fn find_by_inode_returns_none_in_empty_dir() {
    let dir = TempDir::new().unwrap();
    let found = lock::find_by_inode(dir.path(), 12345);
    assert!(found.is_none());
}

#[cfg(unix)]
#[test]
fn find_by_inode_ignores_non_cast_files() {
    use std::os::unix::fs::MetadataExt;
    let dir = TempDir::new().unwrap();
    let txt_file = dir.path().join("notes.txt");
    std::fs::write(&txt_file, "not a cast file").unwrap();
    let inode = std::fs::metadata(&txt_file).unwrap().ino();
    let found = lock::find_by_inode(dir.path(), inode);
    assert!(found.is_none());
}

#[test]
fn find_by_header_ignores_non_cast_files() {
    let dir = TempDir::new().unwrap();
    let header = r#"{"version":3,"term":{"cols":80,"rows":24}}"#;
    let txt_file = dir.path().join("sneaky.txt");
    std::fs::write(&txt_file, header).unwrap();
    let found = lock::find_by_header(dir.path(), header);
    assert!(found.is_none());
}
