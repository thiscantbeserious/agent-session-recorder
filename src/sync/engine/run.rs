//! Run types of the sync engine: options, outcomes, items and the report. Never prints.

use thiserror::Error;

use super::plan::Decision;
use crate::sync::pack::StoreError;

/// Whole-run failures, per-session failures live in the report.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SyncError {
    /// Holds the lock file, a stale one that is empty or garbage is removed by hand.
    #[error("another sync is running, if none is, remove {}", .0.display())]
    Busy(std::path::PathBuf),
    #[error(transparent)]
    Backend(#[from] StoreError),
    #[error("run `agr sync migrate` first")]
    NeedsMigrate,
    #[error("{0}")]
    Failed(String),
    /// GitHub reported a rate limit, `left` sessions were not done.
    #[error("GitHub rate limit reached, sync stopped, {left} left, please run again later")]
    RateLimited { left: usize },
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub overwrite: bool,
    pub dry_run: bool,
    /// Empty means every session.
    pub sessions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Dry run, would act.
    Planned,
    /// Acted, carries the bytes moved.
    Done(u64),
    /// In sync or the other direction, nothing to do for this command.
    Nothing,
    Skipped(String),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub path: String,
    pub decision: Decision,
    pub outcome: Outcome,
    /// Local file size, else the remote plaintext size, 0 when unknown.
    pub size: u64,
    /// A remote entry exists for this path.
    pub on_remote: bool,
    /// The file exists on this machine.
    pub on_local: bool,
    pub local_hash: Option<String>,
    pub remote_hash: Option<String>,
    /// Plaintext size of the remote copy, 0 when unknown.
    pub remote_size: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub items: Vec<Item>,
    pub pushed: usize,
    pub pulled: usize,
    pub deleted: usize,
    pub skipped: usize,
    pub conflicts: usize,
    pub failed: usize,
    pub bytes: u64,
    /// Index was unparseable and rebuilt.
    pub warning: Option<String>,
    /// A push landed but the check for forks afterwards failed.
    pub unchecked: bool,
    /// Sessions waiting in the auto push queue, only set by status.
    pub queued: usize,
    /// Sessions with a fork in the merged view that this run pushed to.
    pub forks: Vec<String>,
    /// Bytes in packs that no session needs any more, and the bytes of all readable packs.
    pub dead_bytes: u64,
    pub pack_bytes: u64,
    /// Sessions whose merged gen is below the last synced one, never pulled.
    pub refused: Vec<String>,
}

impl Report {
    /// The run must exit non-zero.
    pub fn run_failed(&self) -> bool {
        self.failed > 0 || self.unchecked
    }
}
