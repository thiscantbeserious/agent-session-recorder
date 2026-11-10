//! Gist pack store: one secret gist per pack, `index` plus `data.NNN` parts of at most 8 MiB.
//! Reads go through `gh api` (listing, index) with `Range` on `raw_url` (parts).

mod api;
mod append;
mod cache;
mod index_cache;
mod last_push;
mod layout;
mod read;

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use self::api::Seen;
use self::append::Request;
#[doc(hidden)]
pub use self::cache::pack_dir;
use super::{Listing, PackInfo, PackStore, Segment, StoreError};
use crate::sync::backend::gist::pace::{Clock, Pacer, SystemClock};
use crate::sync::backend::gist::runner::{ProcessRunner, Runner};
use crate::sync::backend::gist::ProgressSink;

/// Secret gists as packs, the runner is the only way out to `gh` and `git`.
pub struct GistStore<R: Runner = ProcessRunner> {
    runner: R,
    temp_base: PathBuf,
    /// Where the index cache lives, `None` means no cache.
    cache_file: Option<PathBuf>,
    /// The user cache dir, cached clones and `pace.toml` live under `agr/` in it.
    cache_root: Option<PathBuf>,
    clock: Arc<dyn Clock>,
    /// Percent of the push of an append, `None` means nobody listens.
    progress: Option<ProgressSink>,
    pacer: Mutex<Pacer>,
    /// Files and `updated_at` of every pack this store listed or fetched.
    seen: Mutex<HashMap<String, Seen>>,
}

impl GistStore<ProcessRunner> {
    pub fn new() -> Self {
        let store = Self::with_runner(ProcessRunner::default(), std::env::temp_dir(), None);
        store.with_cache_dir(dirs::cache_dir())
    }
}

impl Default for GistStore<ProcessRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: Runner> GistStore<R> {
    pub fn with_runner(runner: R, temp_base: PathBuf, cache_file: Option<PathBuf>) -> Self {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock::new());
        Self {
            runner,
            temp_base,
            cache_file,
            cache_root: None,
            progress: None,
            pacer: Mutex::new(Pacer::new(clock.clone())),
            clock,
            seen: Mutex::new(HashMap::new()),
        }
    }

    /// Where the cached clones and `pace.toml` go, `<dir>/agr/...`, `None` means no append.
    /// Root for every cache file (the pack clones, `pace.toml`, `index-cache.json`).
    pub fn with_cache_dir(mut self, dir: impl Into<Option<PathBuf>>) -> Self {
        self.cache_root = dir.into();
        self.cache_file = self.cache_root.as_ref().map(|d| index_cache_path(d));
        self
    }

    /// Where the index cache file lives, `None` means no cache.
    #[doc(hidden)]
    pub fn cache_file(&self) -> Option<&std::path::Path> {
        self.cache_file.as_deref()
    }

    /// Replace the clock, pacing and commit dates both follow it.
    #[doc(hidden)]
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.pacer = Mutex::new(Pacer::new(clock.clone()));
        self.clock = clock;
        self
    }

    /// Report the percent of the push of every append to `sink`.
    #[doc(hidden)]
    pub fn with_progress(mut self, sink: ProgressSink) -> Self {
        self.progress = Some(sink);
        self
    }
}

impl<R: Runner> PackStore for GistStore<R> {
    fn name(&self) -> &'static str {
        "gist"
    }

    fn check(&self) -> Result<(), StoreError> {
        self.check_tools()
    }

    fn list(&self) -> Result<Listing, StoreError> {
        self.list_packs()
    }

    fn read_index(&self, id: &str) -> Result<String, StoreError> {
        self.index_of(id)
    }

    fn create(&self, tag: &str, index: &str) -> Result<String, StoreError> {
        self.create_pack(tag, index)
    }

    fn append(
        &self,
        id: &str,
        expected_len: u64,
        expected_index: &str,
        segments: &[Segment],
        index: &str,
    ) -> Result<(), StoreError> {
        let req = Request {
            expected_len,
            expected_index,
            segments,
            index,
        };
        self.append_pack(id, &req)
    }

    fn fits(&self, info: &PackInfo, lens: &[u64]) -> bool {
        self.fits_files(info, lens)
    }

    fn files_for(&self, lens: &[u64]) -> usize {
        layout::cut_parts(lens).len()
    }

    fn sealed(&self, id: &str) {
        self.forget_clone(id);
    }

    fn keep_only(&self, open: Option<&str>) {
        self.sweep_clones(open);
    }

    fn read(&self, id: &str, offset: u64, len: u64) -> Result<Box<dyn Read>, StoreError> {
        self.read_range(id, offset, len)
    }

    fn delete(&self, id: &str) -> Result<(), StoreError> {
        self.delete_pack(id)
    }
}

/// `<cache root>/agr/index-cache.json`.
fn index_cache_path(root: &std::path::Path) -> PathBuf {
    root.join("agr").join("index-cache.json")
}
