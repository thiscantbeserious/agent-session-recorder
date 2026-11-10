use super::tests::{item, packs_of, repo, TARGET};
use super::{Outcome, PushError};
use crate::sync::crypto::Key;
use crate::sync::testing::MemoryStore;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn reload_failure_after_push_reports_pushed_and_check_failed() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let mut repo = repo(&store, &key, "w1");
    store.before_append(|m, _| m.fail_next_list());
    let report = repo.push_batch(&[item(dir.path(), "a", 1, b"x")]).unwrap();
    assert!(report.check_failed);
    let pack = packs_of(&store)[0].id.clone();
    assert_eq!(
        report.items,
        vec![("a".to_string(), 1, Outcome::Pushed { pack })]
    );
}

#[test]
fn failure_in_a_later_chunk_reports_the_earlier_chunks() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let fill = TARGET as usize - 10;
    let (first, second) = (
        item(dir.path(), "a", 1, &vec![1; fill]),
        item(dir.path(), "b", 1, b"yy"),
    );
    let file = second.file.clone();
    let calls = AtomicUsize::new(0);
    store.before_append(move |_, _| {
        if calls.fetch_add(1, Ordering::SeqCst) == 1 {
            std::fs::write(&file, b"zz").unwrap();
        }
    });
    let mut repo = repo(&store, &key, "w1");
    let err: PushError = repo.push_batch(&[first, second]).unwrap_err();
    let done: Vec<&str> = err
        .pushed
        .items
        .iter()
        .map(|(p, _, _)| p.as_str())
        .collect();
    assert_eq!(done, vec!["a"]);
}

#[test]
fn push_report_names_the_open_pack_and_its_length() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let mut repo = repo(&store, &key, "w1");
    let report = repo.push_batch(&[item(dir.path(), "a", 1, b"x")]).unwrap();
    let info = packs_of(&store).remove(0);
    let open = report.open.unwrap();
    assert_eq!((open.id, open.len), (info.id, info.len));
}
