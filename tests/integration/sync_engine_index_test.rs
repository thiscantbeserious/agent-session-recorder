//! Index stamp and the changed-file scan, through the testing facade.

use agr::sync::testing::index::{self, Index};

#[test]
fn old_entries_load_without_a_stamp_and_changed_files_are_found() {
    let json = r#"{"version":1,"entries":{"a/x.cast.gz":{"sha256":"h","synced_at":"t"}}}"#;
    let mut index: Index = serde_json::from_str(json).unwrap();
    assert_eq!(
        (
            index.entries["a/x.cast.gz"].size,
            index.entries["a/x.cast.gz"].mtime_ns
        ),
        (0, 0)
    );
    let dir = tempfile::tempdir().unwrap();
    assert!(
        index::maybe_changed(dir.path(), &index).is_empty(),
        "missing file"
    );
    std::fs::create_dir_all(dir.path().join("a")).unwrap();
    let file = dir.path().join("a/x.cast.gz");
    std::fs::write(&file, "abc").unwrap();
    assert_eq!(index::maybe_changed(dir.path(), &index), ["a/x.cast.gz"]);
    let (size, mtime_ns) = index::stamp(&file);
    let entry = index.entries.get_mut("a/x.cast.gz").unwrap();
    (entry.size, entry.mtime_ns) = (size, mtime_ns);
    assert!(index::maybe_changed(dir.path(), &index).is_empty());
}
