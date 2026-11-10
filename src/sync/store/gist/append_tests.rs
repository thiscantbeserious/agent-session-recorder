//! Append of the gist store over the in-memory gist remote, nothing here reaches GitHub.

use std::io::{self, Write};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::fake_remote::{FakeRemote, TickClock};
use super::{GistStore, PackStore, Segment, StoreError};

/// One store over its own fake remote, clock and cache dir.
pub(super) struct Rig {
    pub remote: Rc<FakeRemote>,
    pub clock: Arc<TickClock>,
    pub root: tempfile::TempDir,
    base: tempfile::TempDir,
}

impl Rig {
    pub(super) fn new() -> Self {
        Self {
            remote: FakeRemote::new(),
            clock: Arc::new(TickClock::new()),
            root: tempfile::tempdir().unwrap(),
            base: tempfile::tempdir().unwrap(),
        }
    }

    /// A store of its own (fresh pacer) on the shared remote, clock and cache dir.
    pub(super) fn store(&self) -> GistStore<Rc<FakeRemote>> {
        let runner = Rc::clone(&self.remote);
        GistStore::with_runner(runner, self.base.path().to_path_buf(), None)
            .with_cache_dir(self.root.path().to_path_buf())
            .with_clock(self.clock.clone())
    }

    pub(super) fn clone_dir(&self, id: &str) -> PathBuf {
        self.root.path().join("agr/packs").join(id)
    }

    pub(super) fn count(&self, word: &str) -> usize {
        let word = format!(" {word} ");
        self.remote
            .calls()
            .iter()
            .filter(|c| c.contains(&word))
            .count()
    }
}

pub(super) fn put(data: &'static [u8]) -> impl Fn(&mut dyn Write) -> io::Result<()> {
    move |w| w.write_all(data)
}

/// Appends `data` as one segment.
pub(super) fn append(
    store: &GistStore<Rc<FakeRemote>>,
    id: &str,
    (len, index): (u64, &str),
    (data, next): (&'static [u8], &str),
) -> Result<(), StoreError> {
    let w = put(data);
    let segs = [Segment {
        len: data.len() as u64,
        write: &w,
    }];
    store.append(id, len, index, &segs, next)
}

const PARTS: [(&str, &[u8]); 2] = [("data.000", b"abc"), ("data.001", b"def")];

#[test]
fn tree_len_not_expected_is_moved() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &PARTS);
    let res = append(&rig.store(), &id, (10, "i0"), (b"xyz", "i1"));
    assert_eq!(res, Err(StoreError::Moved("i0".into())));
}

#[test]
fn zero_byte_append_on_stale_index_is_moved() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &PARTS);
    let res = rig.store().append(&id, 6, "stale", &[], "i2");
    assert_eq!(res, Err(StoreError::Moved("i0".into())));
    assert!(!rig.remote.calls().iter().any(|c| c.contains("push")));
}

#[test]
fn rejected_push_is_moved_with_the_fetched_index() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &PARTS);
    rig.remote.race_on_push(|files| {
        files.insert("index".into(), b"r1".to_vec());
    });
    let res = append(&rig.store(), &id, (6, "i0"), (b"xyz", "i1"));
    assert_eq!(res, Err(StoreError::Moved("r1".into())));
    assert!(!rig.remote.calls().iter().any(|c| c.contains("--force")));
}

#[test]
fn rejected_push_with_a_failing_fetch_is_not_moved() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &PARTS);
    rig.remote.race_on_push(|files| {
        files.insert("index".into(), b"r1".to_vec());
    });
    rig.remote.fail_fetch(true);
    let res = append(&rig.store(), &id, (6, "i0"), (b"xyz", "i1"));
    assert!(matches!(res, Err(StoreError::Other(_))), "{res:?}");
}

#[test]
fn append_is_one_commit_with_parts_and_index() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &PARTS);
    append(&rig.store(), &id, (6, "i0"), (b"xyz", "i1")).unwrap();
    assert_eq!(rig.remote.commit_changes().len(), 1);
    assert_eq!(rig.remote.commit_changes()[0], vec!["data.002", "index"]);
    assert_eq!(rig.remote.push_changes(), vec![vec!["data.002", "index"]]);
}

#[test]
fn pushes_to_one_gist_are_paced_10s() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &[]);
    let store = rig.store();
    append(&store, &id, (0, "i0"), (b"abc", "i1")).unwrap();
    append(&store, &id, (3, "i1"), (b"def", "i2")).unwrap();
    assert_eq!(rig.clock.slept(), vec![Duration::from_secs(10)]);
}

#[test]
fn pacing_holds_across_processes() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &[]);
    append(&rig.store(), &id, (0, "i0"), (b"abc", "i1")).unwrap();
    rig.clock.advance(Duration::from_secs(3));
    append(&rig.store(), &id, (3, "i1"), (b"def", "i2")).unwrap();
    assert_eq!(rig.clock.slept(), vec![Duration::from_secs(7)]);
}

#[test]
fn progress_percent_belongs_to_the_append() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &[]);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let store = rig
        .store()
        .with_progress(Arc::new(move |p| sink.lock().unwrap().push(p)));
    append(&store, &id, (0, "i0"), (b"abc", "i1")).unwrap();
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.last(), Some(&100));
    assert_eq!(seen, vec![50, 100], "the clone reports no percent");
}
