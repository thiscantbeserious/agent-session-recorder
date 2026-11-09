//! Gist pack store: one secret gist per pack, `index` plus `data.NNN` parts of at most 8 MiB.
//! Reads go through `gh api` (listing, index) and `curl` with `Range` on `raw_url` (parts).

mod api;
mod index_cache;
mod layout;
mod read;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use self::api::Seen;
use super::{Listing, PackInfo, PackStore, Segment, StoreError};
use crate::sync::backend::gist::pace::{Pacer, SystemClock};
use crate::sync::backend::gist::runner::{ProcessRunner, Runner};

/// Secret gists as packs, the runner is the only way out to `gh` and `curl`.
pub struct GistStore<R: Runner = ProcessRunner> {
    runner: R,
    temp_base: PathBuf,
    /// Where the index cache lives, `None` means no cache.
    #[allow(dead_code)] // the index cache of the next commit reads it
    cache_file: Option<PathBuf>,
    pacer: Mutex<Pacer>,
    /// Files and `updated_at` of every pack this store listed or fetched.
    seen: Mutex<HashMap<String, Seen>>,
}

impl GistStore<ProcessRunner> {
    pub fn new() -> Self {
        let cache = dirs::cache_dir().map(|d| d.join("agr").join("index-cache.json"));
        Self::with_runner(ProcessRunner::default(), std::env::temp_dir(), cache)
    }
}

impl Default for GistStore<ProcessRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: Runner> GistStore<R> {
    pub fn with_runner(runner: R, temp_base: PathBuf, cache_file: Option<PathBuf>) -> Self {
        Self {
            runner,
            temp_base,
            cache_file,
            pacer: Mutex::new(Pacer::new(Arc::new(SystemClock::new()))),
            seen: Mutex::new(HashMap::new()),
        }
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
        _id: &str,
        _expected_len: u64,
        _expected_index: &str,
        _segments: &[Segment],
        _index: &str,
    ) -> Result<(), StoreError> {
        Err(StoreError::Other("append is not built yet".into()))
    }

    fn fits(&self, info: &PackInfo, lens: &[u64]) -> bool {
        self.fits_files(info, lens)
    }

    fn read(&self, id: &str, offset: u64, len: u64) -> Result<Box<dyn Read>, StoreError> {
        self.read_range(id, offset, len)
    }

    fn delete(&self, _id: &str) -> Result<(), StoreError> {
        Err(StoreError::Other("delete is not built yet".into()))
    }
}
