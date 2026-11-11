//! Plain text lines for the sync commands, colour is added by the caller via theme helpers.

use humansize::{format_size, BINARY};

use crate::sync::engine::plan::{Decision, SkipReason};
use crate::sync::engine::run::{Item, Outcome, Report};

mod live;
mod packs;
mod status;
mod summary;

pub use live::{live_head, Live, LiveLine, LoadLine};
pub use packs::migrate_lines;
pub use status::{
    compact_delete_preview_lines, compact_preview_lines, compact_status_lines,
    compact_summary_lines,
};
pub use summary::{
    delete_preview_lines, preview_lines, privacy_lines, problem_lines, progress_line,
    rate_limit_lines, summary_lines,
};

const ORDER: [&str; 10] = [
    "in sync",
    "local ahead (push)",
    "local only (push)",
    "remote ahead (pull)",
    "remote only (pull)",
    "conflict",
    "duplicate",
    "fork",
    "deleted on the remote",
    "skipped",
];

/// Status label of one session, from the decision and which sides have it.
pub fn label(item: &Item) -> &'static str {
    match item.decision {
        Decision::InSync { .. } => ORDER[0],
        Decision::Push if item.on_remote => ORDER[1],
        Decision::Push => ORDER[2],
        Decision::Pull if item.on_local => ORDER[3],
        Decision::Pull => ORDER[4],
        Decision::Conflict => ORDER[5],
        Decision::Duplicate => ORDER[6],
        Decision::Fork => ORDER[7],
        Decision::Skip(SkipReason::DeletedOnRemote) => ORDER[8],
        Decision::OtherKey | Decision::UnknownVersion | Decision::Skip(_) => ORDER[9],
    }
}

/// Backend line, one line per session and totals per state.
pub fn status_lines(report: &Report, backend: &str) -> Vec<String> {
    let mut lines = vec![backend_line(report, backend)];
    lines.extend(queued_line(report));
    if report.items.is_empty() {
        lines.push("No sessions.".into());
        return lines;
    }
    lines.extend(report.items.iter().flat_map(status_block));
    lines.push(totals(report));
    lines.extend(packs::prune_hint(report));
    lines
}

/// `N queued for auto push`, only when the queue is not empty.
pub(super) fn queued_line(report: &Report) -> Option<String> {
    (report.queued > 0).then(|| format!("{} queued for auto push", report.queued))
}

pub(super) fn backend_line(report: &Report, backend: &str) -> String {
    let remote = report
        .items
        .iter()
        .filter(|i| i.on_remote && !i.path.starts_with("remote:"))
        .count();
    format!("Backend: {backend}, {remote} remote session(s)")
}

pub(super) fn status_block(item: &Item) -> Vec<String> {
    vec![status_line(item)]
}

fn status_line(item: &Item) -> String {
    let mut line = format!("  {} {} ({})", item.path, label(item), size(item.size));
    if let Outcome::Skipped(why) | Outcome::Failed(why) = &item.outcome {
        line.push_str(&format!(": {why}"));
    }
    line
}

fn totals(report: &Report) -> String {
    let parts: Vec<String> = ORDER
        .iter()
        .filter_map(|name| {
            let n = report.items.iter().filter(|i| label(i) == *name).count();
            (n > 0).then(|| format!("{n} {name}"))
        })
        .collect();
    format!("Totals: {}", parts.join(", "))
}

fn short(hash: Option<&str>) -> String {
    hash.unwrap_or("-").chars().take(12).collect()
}

fn size(bytes: u64) -> String {
    format_size(bytes, BINARY)
}
