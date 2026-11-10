//! One gist store over the in-memory remote, its own clock and cache dir.

use super::fake_remote::{FakeRemote, Handle};
use agr::sync::testing::{pack_dir, Clock, ClockError, GistStore, PackStore, Segment, StoreError};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// One store over its own fake remote, clock and cache dir.
pub struct Rig {
    pub remote: Handle,
    pub clock: Arc<TickClock>,
    pub root: tempfile::TempDir,
    base: tempfile::TempDir,
}

impl Rig {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            remote: FakeRemote::new(),
            clock: Arc::new(TickClock::new()),
            root: tempfile::tempdir().unwrap(),
            base: tempfile::tempdir().unwrap(),
        }
    }

    /// A store of its own (fresh pacer) on the shared remote, clock and cache dir.
    pub fn store(&self) -> GistStore<Handle> {
        let runner = self.remote.clone();
        GistStore::with_runner(runner, self.base.path().to_path_buf(), None)
            .with_cache_dir(self.root.path().to_path_buf())
            .with_clock(self.clock.clone())
    }

    pub fn clone_dir(&self, id: &str) -> PathBuf {
        pack_dir(self.root.path(), id).unwrap()
    }

    /// Calls that run git or gh with `word` as one of their arguments.
    pub fn count(&self, word: &str) -> usize {
        let calls = self.remote.calls();
        calls
            .iter()
            .filter(|c| format!("{c} ").contains(&format!(" {word} ")))
            .count()
    }
}

pub fn put(data: &'static [u8]) -> impl Fn(&mut dyn Write) -> io::Result<()> {
    move |w| w.write_all(data)
}

/// Appends `data` as one segment.
pub fn append(
    store: &GistStore<Handle>,
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

/// Test clock where monotonic and unix time are one value, `sleep` moves it and is recorded.
pub struct TickClock(Mutex<(Duration, Vec<Duration>)>);

impl TickClock {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self(Mutex::new((Duration::from_secs(1_000), Vec::new())))
    }

    pub fn advance(&self, by: Duration) {
        self.0.lock().unwrap().0 += by;
    }

    pub fn slept(&self) -> Vec<Duration> {
        self.0.lock().unwrap().1.clone()
    }
}

impl Clock for TickClock {
    fn mono(&self) -> Duration {
        self.0.lock().unwrap().0
    }

    fn unix_secs(&self) -> Result<u64, ClockError> {
        Ok(self.mono().as_secs())
    }

    fn sleep(&self, time: Duration) {
        let mut state = self.0.lock().unwrap();
        state.0 += time;
        state.1.push(time);
    }
}
