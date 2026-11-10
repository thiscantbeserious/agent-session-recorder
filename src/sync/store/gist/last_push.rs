//! Wall clock time of the last push per gist in `<user cache dir>/agr/pace.toml`, so separate
//! processes keep the 10 s gap. Read and written only while the caller holds the local sync lock
//! (the `SyncLock` of the drain, wired in Stage 55), as such there is no file lock here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::index_cache::write_atomic;
use super::GistStore;
use crate::sync::backend::gist::runner::Runner;

/// GitHub recommends at most 6 pushes a minute per repository.
const GAP: Duration = Duration::from_secs(10);

#[derive(Default, Serialize, Deserialize)]
struct Times {
    #[serde(default)]
    last_push: BTreeMap<String, u64>,
}

pub(super) struct LastPush(PathBuf);

impl LastPush {
    pub(super) fn new(root: &Path) -> Self {
        Self(root.join("agr").join("pace.toml"))
    }

    fn load(&self) -> Times {
        let text = std::fs::read_to_string(&self.0).unwrap_or_default();
        toml::from_str(&text).unwrap_or_default()
    }

    /// What is left of the gap at `now` (unix seconds), nothing for a gist never pushed to.
    pub(super) fn wait_for(&self, id: &str, now: u64) -> Duration {
        let last = self.load().last_push.get(id).copied();
        last.map_or(Duration::ZERO, |t| {
            GAP.saturating_sub(Duration::from_secs(now.saturating_sub(t)))
        })
    }

    /// Best effort, a write that fails only costs a missed wait.
    pub(super) fn put(&self, id: &str, now: u64) {
        let mut all = self.load();
        all.last_push.insert(id.to_string(), now);
        if let Ok(text) = toml::to_string(&all) {
            let _ = write_atomic(&self.0, &text);
        }
    }
}

impl<R: Runner> GistStore<R> {
    pub(super) fn last_push(&self) -> Option<LastPush> {
        self.cache_root.as_deref().map(LastPush::new)
    }
}
