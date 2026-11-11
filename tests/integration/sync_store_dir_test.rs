//! `DirStore` crash recovery and listing.

use agr::sync::testing::{DirStore, PackStore, Segment};
use std::fs;
use std::io::Write;

/// `exists` follows the link and says no, `create_dir` says AlreadyExists, as a racing writer would.
#[cfg(unix)]
#[test]
fn create_never_reuses_an_existing_pack_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::os::unix::fs::symlink(dir.path().join("nowhere"), dir.path().join("pack-1")).unwrap();
    let store = DirStore::new(dir.path());
    assert_eq!(store.create("t", "i0").unwrap(), "pack-2");
}

#[test]
fn dir_list_ignores_a_stray_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join(".DS_Store"), b"x").unwrap();
    let store = DirStore::new(dir.path());
    assert!(store.list().is_ok());
}

#[test]
fn dir_store_crash_after_parts_before_manifest_recovers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = DirStore::new(dir.path());
    let id = store.create("t", "i0").unwrap();
    let w = |w: &mut dyn Write| w.write_all(b"abc");
    let seg = || Segment { len: 3, write: &w };
    store.append(&id, 0, "i0", &[seg()], "i1").unwrap();
    let old_len = store.list().unwrap().packs[0].len;
    store.crash_before_manifest();
    assert!(store.append(&id, old_len, "i1", &[seg()], "i2").is_err());
    assert_eq!(store.list().unwrap().packs[0].len, old_len);
    let ok = store.append(&id, old_len, "i1", &[seg()], "i2");
    assert!(ok.is_ok(), "{ok:?}");
}

#[test]
fn dir_store_next_append_deletes_unlisted_parts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = DirStore::new(dir.path());
    let id = store.create("t", "i0").unwrap();
    let w = |w: &mut dyn Write| w.write_all(b"abc");
    let seg = Segment { len: 3, write: &w };
    store.crash_before_manifest();
    assert!(store.append(&id, 0, "i0", &[seg], "i1").is_err());
    let orphan = dir.path().join(&id).join("part.000000");
    assert!(orphan.exists());
    store.append(&id, 0, "i0", &[], "i1").unwrap();
    assert!(!orphan.exists());
}

#[test]
fn dir_store_crash_before_the_rename_keeps_the_old_manifest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = DirStore::new(dir.path());
    let id = store.create("t", "i0").unwrap();
    let w = |w: &mut dyn Write| w.write_all(b"abc");
    let seg = || Segment { len: 3, write: &w };
    store.crash_before_rename();
    assert!(store.append(&id, 0, "i0", &[seg()], "i1").is_err());
    assert_eq!(store.list().unwrap().packs[0].len, 0);
    assert_eq!(store.read_index(&id).unwrap(), "i0");
    store.append(&id, 0, "i0", &[seg()], "i1").unwrap();
    assert_eq!(store.list().unwrap().packs[0].len, 3);
}

#[test]
fn dir_store_list_reports_a_corrupt_manifest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = DirStore::new(dir.path());
    let id = store.create("t", "i0").unwrap();
    fs::write(dir.path().join(id).join("manifest"), "{not json").unwrap();
    assert!(store.list().is_err());
}
