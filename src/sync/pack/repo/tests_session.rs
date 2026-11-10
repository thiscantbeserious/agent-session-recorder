use super::tests::{item, packs_of, repo, tamper, TARGET};
use super::{ReadError, Repo, RepoError};
use crate::sync::crypto::{sha256_hex, Key};
use crate::sync::store::PackStore;
use crate::sync::testing::MemoryStore;
use std::path::Path;

/// Names of the hidden files in `dir`.
fn hidden(dir: &Path) -> Vec<String> {
    let names = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name());
    names
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| n.starts_with('.'))
        .collect()
}

#[test]
fn read_session_tries_next_place_on_not_found() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let file = item(dir.path(), "a", 1, b"same bytes");
    repo(&store, &key, "w1").push_batch(&[file]).unwrap();
    let again = item(dir.path(), "a", 1, b"same bytes");
    repo(&store, &key, "w2").push_batch(&[again]).unwrap();
    let reader = repo(&store, &key, "w3");
    let keeper = reader.view().get("a").unwrap().keeper().pack.clone();
    store.delete(&keeper).unwrap();
    let target = dir.path().join("out.cast");
    let _ = reader.read_session("a", &target);
    let got = std::fs::read(&target)
        .map(|b| sha256_hex(&b))
        .unwrap_or_default();
    assert_eq!(got, sha256_hex(b"same bytes"));
}

#[test]
fn read_session_writes_to_temp_and_renames_only_on_success() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    repo(&store, &key, "w1")
        .push_batch(&[item(dir.path(), "a", 1, b"x")])
        .unwrap();
    let reader = repo(&store, &key, "w1");
    store.delete(&packs_of(&store)[0].id).unwrap();
    let out = tempfile::tempdir().unwrap();
    let target = out.path().join("out.cast");
    let res = reader.read_session("a", &target);
    assert!(res.is_err(), "{res:?}");
    assert!(!target.exists());
    assert_eq!(hidden(out.path()), Vec::<String>::new());
}

#[test]
fn read_session_checks_sha256_after_open() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    repo(&store, &key, "w1")
        .push_batch(&[item(dir.path(), "a", 1, b"x")])
        .unwrap();
    tamper(&store, &key, &packs_of(&store)[0], |i| {
        i.entries[0].sha256 = "00".repeat(32)
    });
    let out = tempfile::tempdir().unwrap();
    let target = out.path().join("out.cast");
    let err = repo(&store, &key, "w1").read_session("a", &target).err();
    assert!(matches!(err, Some(ReadError::HashMismatch)), "{err:?}");
    assert!(!target.exists());
    assert_eq!(hidden(out.path()), Vec::<String>::new());
}

#[test]
fn tombstone_appends_deleted_at_gen_plus_one() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let mut repo = repo(&store, &key, "w1");
    repo.push_batch(&[item(dir.path(), "s", 3, b"x")]).unwrap();
    repo.tombstone("s").unwrap();
    let merged = repo.view().get("s").unwrap();
    assert_eq!((merged.gen, merged.is_deleted()), (4, true));
}

#[test]
fn fork_created_by_push_is_reported_by_the_re_read() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let theirs = item(dir.path(), "x", 1, b"theirs");
    repo(&store, &key, "w2").push_batch(&[theirs]).unwrap();
    let mine = item(dir.path(), "x", 1, b"mine!");
    let report = repo(&store, &key, "w1").push_batch(&[mine]).unwrap();
    assert_eq!(report.forks, vec!["x"]);
}

#[test]
fn open_fails_when_any_index_read_fails() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    for (w, n) in [("w1", "a"), ("w2", "b"), ("w3", "c")] {
        repo(&store, &key, w)
            .push_batch(&[item(dir.path(), n, 1, b"x")])
            .unwrap();
    }
    store.fail_read_index(&packs_of(&store)[1].id);
    let res = Repo::open(&store, &key, "w1", TARGET, None).err();
    assert!(matches!(res, Some(RepoError::PartialView(_))), "{res:?}");
}

/// `name` pushed by two writers, so its session has two places, `w3` reads it.
fn two_places<'a>(store: &'a MemoryStore, key: &'a Key, dir: &Path) -> (Repo<'a>, Vec<u8>) {
    let original = vec![9u8; 70_000];
    for w in ["w1", "w2"] {
        let file = item(dir, "a", 1, &original);
        repo(store, key, w).push_batch(&[file]).unwrap();
    }
    (repo(store, key, "w3"), original)
}

#[test]
fn read_session_after_a_place_fails_mid_stream_has_no_leftover_bytes() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let (reader, original) = two_places(&store, &key, dir.path());
    let first = reader.view().get("a").unwrap().keeper().pack.clone();
    store.fail_read_after(&first, 66_000);
    let target = dir.path().join("out.cast");
    let _ = reader.read_session("a", &target);
    assert_eq!(std::fs::read(&target).unwrap_or_default(), original);
}

#[test]
fn mismatch_at_first_place_reads_the_second() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let (reader, original) = two_places(&store, &key, dir.path());
    let first = reader.view().get("a").unwrap().keeper().pack.clone();
    let info = packs_of(&store)
        .into_iter()
        .find(|p| p.id == first)
        .unwrap();
    tamper(&store, &key, &info, |i| {
        i.entries[0].sha256 = "00".repeat(32)
    });
    let target = dir.path().join("out.cast");
    let _ = repo(&store, &key, "w3").read_session("a", &target);
    assert_eq!(std::fs::read(&target).unwrap_or_default(), original);
}

#[test]
fn mismatch_at_every_place_is_a_hash_mismatch() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let (_, _) = two_places(&store, &key, dir.path());
    for info in packs_of(&store) {
        tamper(&store, &key, &info, |i| {
            i.entries[0].sha256 = "00".repeat(32)
        });
    }
    let target = dir.path().join("out.cast");
    let err = repo(&store, &key, "w3").read_session("a", &target).err();
    assert!(matches!(err, Some(ReadError::HashMismatch)), "{err:?}");
    assert!(!target.exists());
}

#[test]
fn auth_failure_at_first_place_reads_the_second() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let original = b"same bytes".to_vec();
    for writer in ["w1", "w2"] {
        let file = item(dir.path(), "a", 1, &original);
        repo(&store, &key, writer).push_batch(&[file]).unwrap();
    }
    let reader = repo(&store, &key, "w3");
    store.corrupt(&reader.view().get("a").unwrap().keeper().pack);
    let target = dir.path().join("out.cast");
    reader.read_session("a", &target).unwrap();
    assert_eq!(std::fs::read(target).unwrap(), original);
}
