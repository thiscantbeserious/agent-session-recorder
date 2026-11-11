//! Compact `agr sync status`, one summary line per category.

use std::collections::BTreeMap;

use super::summary::{conflict_line, count_lines, detail, failed_line, fork_lines, planned_head};
use super::{backend_line, label, size, status_block};
use crate::sync::engine::plan::Decision;
use crate::sync::engine::run::{Item, Outcome, Report};

const HINT: &str = "Run agr sync status -v for the per-session list.";

/// Backend line, one line per category with count and size, the sessions that need action,
/// then the hint. `status_lines` is the full per-session list behind `-v`.
pub fn compact_status_lines(report: &Report, backend: &str) -> Vec<String> {
    let mut lines = vec![backend_line(report, backend)];
    lines.extend(super::queued_line(report));
    if report.items.is_empty() {
        lines.push("No sessions.".into());
        return lines;
    }
    lines.extend(category_lines(report));
    lines.extend(action_lines(report));
    lines.extend(super::packs::prune_hint(report));
    lines.push(HINT.into());
    lines
}

/// What a push or pull would do, compact: the head, skip reasons grouped, then the sessions
/// that need action. `preview_lines` and `problem_lines` are the per-session list behind `-v`.
pub fn compact_preview_lines(report: &Report, verb: &str) -> Vec<String> {
    let mut lines =
        vec![planned_head(report, verb).unwrap_or_else(|| format!("Nothing to {verb}."))];
    lines.extend(skipped_lines(report));
    lines.extend(action_lines(report));
    lines.push(hint(verb));
    lines
}

/// Result of a real run, compact: counts, skip reasons grouped, conflicts and failures listed.
pub fn compact_summary_lines(report: &Report, verb: &str) -> Vec<String> {
    let mut lines = count_lines(report);
    lines.extend(skipped_lines(report));
    lines.extend(detail("Conflicts", report.conflicts, report, conflict_line));
    lines.extend(detail("Failed", report.failed, report, failed_line));
    lines.extend(fork_lines(report));
    if lines.is_empty() {
        lines.push("Nothing to do.".into());
    } else if report.skipped > 0 {
        lines.push(hint(verb));
    }
    lines.extend(report.warning.iter().map(|w| format!("Warning: {w}")));
    lines
}

/// Remote copies `agr sync delete` would remove, compact, the paths are behind `-v`.
pub fn compact_delete_preview_lines(report: &Report) -> Vec<String> {
    let mut lines = super::delete_preview_lines(report);
    if lines.len() > 2 {
        let forks = report
            .items
            .iter()
            .filter(|i| i.on_remote && i.decision == Decision::Fork)
            .count();
        let mut mid = vec![hint("delete")];
        if forks > 0 {
            mid.insert(
                0,
                format!("{forks} fork(s), every version is deleted, see -v"),
            );
        }
        lines.splice(1..lines.len() - 1, mid);
    }
    lines
}

fn hint(verb: &str) -> String {
    format!("Run agr sync {verb} -v for the per-session list.")
}

fn category_lines(report: &Report) -> Vec<String> {
    let of = |names: &[&str]| -> Vec<&Item> {
        let hit = |i: &&Item| names.contains(&label(i));
        report.items.iter().filter(hit).collect()
    };
    let push = of(&["local ahead (push)", "local only (push)"]);
    let pull = of(&["remote ahead (pull)", "remote only (pull)"]);
    let synced = of(&["in sync"]);
    let conflicts = of(&["conflict"]).len();
    let mut lines = Vec::new();
    lines.extend(sized(push.len(), "to push", push.iter().map(|i| i.size)));
    lines.extend(sized(
        pull.len(),
        "to pull",
        pull.iter().map(|i| i.remote_size),
    ));
    lines.extend(sized(
        synced.len(),
        "in sync",
        synced.iter().map(|i| i.size),
    ));
    lines.extend(plural(conflicts, "conflict"));
    lines.extend(plural(of(&["duplicate"]).len(), "duplicate"));
    lines.extend(skipped_lines(report));
    lines
}

fn sized(n: usize, name: &str, bytes: impl Iterator<Item = u64>) -> Option<String> {
    (n > 0).then(|| format!("{n} {name} ({})", size(bytes.sum())))
}

fn plural(n: usize, name: &str) -> Option<String> {
    match n {
        0 => None,
        1 => Some(format!("1 {name}")),
        n => Some(format!("{n} {name}s")),
    }
}

/// One line per skip reason, biggest group first.
fn skipped_lines(report: &Report) -> Vec<String> {
    let mut groups: BTreeMap<&str, usize> = BTreeMap::new();
    for item in &report.items {
        if let (Outcome::Skipped(why), false) = (&item.outcome, item.decision == Decision::Conflict)
        {
            *groups.entry(why).or_default() += 1;
        }
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by_key(|(why, n)| (std::cmp::Reverse(*n), *why));
    groups
        .into_iter()
        .map(|(why, n)| format!("{n} skipped: {why}"))
        .collect()
}

/// Conflicts, failures, other-key entries and duplicates per session, then the index warning.
fn action_lines(report: &Report) -> Vec<String> {
    let mut lines = detail("Conflicts", report.conflicts, report, conflict_line);
    lines.extend(detail("Failed", report.failed, report, failed_line));
    lines.extend(super::packs::other_key_lines(report));
    let dups = report
        .items
        .iter()
        .filter(|i| i.decision == Decision::Duplicate);
    lines.extend(dups.flat_map(status_block));
    lines.extend(report.warning.iter().map(|w| format!("Warning: {w}")));
    lines
}
