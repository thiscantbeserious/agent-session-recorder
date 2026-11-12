//! Gist pack store: one secret gist per pack, `index` plus `data.NNN` parts of at most 8 MiB.
//! Reads go through `gh api` (listing, index) with `Range` on `raw_url` (parts).

mod api;
mod append;
mod cache;
mod index_cache;
mod last_push;
mod layout;
pub(crate) mod pace;
pub(crate) mod parse;
pub(crate) mod parts;
pub(crate) mod progress;
mod read;
pub mod runner;

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use self::api::Seen;
use self::append::Request;
#[doc(hidden)]
pub use self::cache::pack_dir;
use super::{Listing, PackInfo, PackStore, Segment, StoreError};
use crate::sync::store::gist::pace::{Clock, Pacer, SystemClock};
use crate::sync::store::gist::runner::{Output, ProcessRunner, Runner};

/// Receives the percent of the running upload or download, never a path or a name.
pub type ProgressSink = Arc<dyn Fn(u8) + Send + Sync>;

pub(crate) const GIT_IDENTITY: [&str; 6] = [
    "-c",
    "user.name=agr",
    "-c",
    "user.email=agr@localhost",
    "-c",
    "commit.gpgsign=false",
];
/// Env beats `-c user.*`, so a user's exported `GIT_AUTHOR_*` would leak without these.
pub(crate) const IDENTITY_ENV: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", "agr"),
    ("GIT_AUTHOR_EMAIL", "agr@localhost"),
    ("GIT_COMMITTER_NAME", "agr"),
    ("GIT_COMMITTER_EMAIL", "agr@localhost"),
];

pub(crate) fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(ToString::to_string).collect()
}

pub(crate) fn io_result(
    tool: &'static str,
    result: std::io::Result<Output>,
) -> Result<Output, StoreError> {
    match result {
        Ok(out) => Ok(out),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(StoreError::NotInstalled(tool)),
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Err(StoreError::Other(format!(
            "{tool} stalled, nothing came back in time, retry with `agr sync push`"
        ))),
        Err(e) => Err(StoreError::Other(format!("{tool}: {}", e.kind()))),
    }
}

/// One error line and an optional hint line per failure, the gist store knows its tools.
pub fn explain(error: &StoreError) -> (String, Option<String>) {
    match error {
        StoreError::NotInstalled("gh") => (
            "gh not found, the gist backend needs gh and git".into(),
            Some("Install gh from https://cli.github.com, then run `gh auth login`".into()),
        ),
        StoreError::NotInstalled(tool) => (
            format!("{tool} not found, the gist backend needs gh and git"),
            Some(format!(
                "Install {tool} and gh (https://cli.github.com), then run `gh auth login`"
            )),
        ),
        StoreError::NotLoggedIn => (
            "gh not logged in".into(),
            Some("Run `gh auth login`".into()),
        ),
        StoreError::MissingScope => (
            "the gh token has no gist scope".into(),
            Some("Run `gh auth refresh -s gist`".into()),
        ),
        StoreError::RateLimited => (
            "GitHub rate limit reached, sync stopped".into(),
            Some("Please run again later".into()),
        ),
        StoreError::NotFound => ("remote entry not found".into(), None),
        other => (other.to_string(), None),
    }
}

/// The old per-session gists, read by `agr sync migrate` only.
pub fn legacy_reader() -> Box<dyn super::legacy::LegacyRead> {
    Box::new(super::legacy::GistLegacy::new())
}

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
        self.list_packs(true)
    }

    fn list_for_migrate(&self) -> Result<Listing, StoreError> {
        self.list_packs(false)
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

    /// The clone of a pack that is not our open one is gone when this returns, success or error.
    fn append_transient(
        &self,
        id: &str,
        expected_len: u64,
        expected_index: &str,
        segments: &[Segment],
        index: &str,
    ) -> Result<(), StoreError> {
        let res = self.append(id, expected_len, expected_index, segments, index);
        self.forget_clone(id);
        res
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
