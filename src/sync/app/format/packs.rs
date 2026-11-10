//! Per pack lines of `agr sync status`: other key packs, forks and the prune hint.

use crate::sync::engine::plan::Decision;
use crate::sync::engine::run::{Item, Report};

/// Dead share of the remote, in percent, above which status points at `agr sync prune`.
const PRUNE_ABOVE_PERCENT: u64 = 20;

/// `Other key: N` and one line per pack, `remote:<id> (other key)`.
pub(super) fn other_key_lines(report: &Report) -> Vec<String> {
    let other: Vec<&Item> = report
        .items
        .iter()
        .filter(|i| i.decision == Decision::OtherKey)
        .collect();
    if other.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![format!("Other key: {}", other.len())];
    lines.extend(other.iter().map(|i| format!("  {} (other key)", i.path)));
    lines
}

/// The prune hint, only when more than 20 percent of the remote bytes are dead.
pub(super) fn prune_hint(report: &Report) -> Option<String> {
    let dead = u128::from(report.dead_bytes) * 100;
    let over = dead > u128::from(report.pack_bytes) * u128::from(PRUNE_ABOVE_PERCENT);
    over.then(|| {
        let percent = dead / u128::from(report.pack_bytes.max(1));
        format!("{percent}% of the remote is dead data, run `agr sync prune` to reclaim it.")
    })
}

/// Result lines of `agr sync migrate`, the engine already worded the kept gists.
pub fn migrate_lines(report: &crate::sync::pack::migrate::Report) -> Vec<String> {
    let mut lines = vec![format!(
        "Moved {} sessions, deleted {} old gists.",
        report.moved, report.deleted
    )];
    lines.extend(report.lines.iter().cloned());
    lines
}
