//! `--seal-orphans`: idle open packs of other writers, ADR open risk 4.

use crate::sync::pack::merge::View;
use crate::sync::store::PackInfo;

/// Open packs of other writers idle for longer than this are sealed.
pub const IDLE_SECS: i64 = 30 * 24 * 60 * 60;
/// A run downloads at most this much for seals, the rest waits for the next run.
pub const BUDGET: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SealPlan {
    pub seal: Vec<String>,
    pub download: u64,
    pub left: usize,
}

pub fn seal_plan(
    _view: &View,
    _packs: &[PackInfo],
    _own: &str,
    _now: i64,
    _budget: u64,
) -> SealPlan {
    SealPlan::default()
}
