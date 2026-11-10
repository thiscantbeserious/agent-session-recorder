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
