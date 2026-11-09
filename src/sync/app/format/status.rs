//! Compact `agr sync status`, one summary line per category.

use std::collections::BTreeMap;

use super::summary::{conflict_line, count_lines, detail, failed_line, planned_head};
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
        lines.splice(1..lines.len() - 1, [hint("delete")]);
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
    let other: Vec<&Item> = report
        .items
        .iter()
        .filter(|i| i.decision == Decision::OtherKey)
        .collect();
    if !other.is_empty() {
        lines.push(format!("Other key: {}", other.len()));
        lines.extend(other.iter().map(|i| format!("  {}", i.path)));
    }
    let dups = report
        .items
        .iter()
        .filter(|i| i.decision == Decision::Duplicate);
    lines.extend(dups.flat_map(status_block));
    lines.extend(report.warning.iter().map(|w| format!("Warning: {w}")));
    lines
}

#[cfg(test)]
mod tests {
    use super::{
        compact_delete_preview_lines, compact_preview_lines, compact_status_lines,
        compact_summary_lines,
    };
    use crate::sync::app::format::status_lines;
    use crate::sync::app::format::tests::{item, report, sync};
    use crate::sync::app::format::{
        delete_preview_lines, preview_lines, problem_lines, summary_lines,
    };
    use crate::sync::engine::plan::{Decision, SkipReason};
    use crate::sync::engine::run::{Item, Outcome, Report};

    const MIB: u64 = 1024 * 1024;

    fn many(n: usize, decision: Decision, outcome: Outcome, size: u64) -> Vec<Item> {
        (0..n)
            .map(|i| {
                let mut it = item(&format!("claude/{i}.cast.gz"), decision, outcome.clone());
                it.size = size;
                it.remote_size = size;
                it
            })
            .collect()
    }

    fn plain() -> Outcome {
        Outcome::Skipped("plain .cast, run `agr compress` first".into())
    }

    fn big_report() -> Report {
        let mut items = many(721, Decision::Push, Outcome::Planned, 190_000);
        items.extend(many(6, Decision::Skip(SkipReason::Plain), plain(), 10));
        items.extend(many(3, sync(), Outcome::Nothing, MIB));
        items.extend(many(2, Decision::Pull, Outcome::Nothing, 3 * MIB));
        let mut conflict = item(
            "claude/c.cast.gz",
            Decision::Conflict,
            Outcome::Skipped("conflict, use --overwrite".into()),
        );
        conflict.size = 2048;
        let mut other = item(
            "claude/o.cast.gz",
            Decision::OtherKey,
            Outcome::Skipped("sealed with another key".into()),
        );
        other.on_local = false;
        items.extend([conflict, other]);
        let mut r = report(items);
        r.skipped = 7;
        r.conflicts = 1;
        r
    }

    #[test]
    fn compact_status_has_one_summary_line_per_category() {
        let lines = compact_status_lines(&big_report(), "gist");
        assert_eq!(lines[0], "Backend: gist, 734 remote session(s)");
        assert_eq!(lines[1], "721 to push (130.64 MiB)");
        assert_eq!(lines[2], "2 to pull (6 MiB)");
        assert_eq!(lines[3], "3 in sync (3 MiB)");
        assert_eq!(lines[4], "1 conflict");
        assert_eq!(lines[5], "6 skipped: plain .cast, run `agr compress` first");
        assert_eq!(lines[6], "1 skipped: sealed with another key");
        assert_eq!(
            lines.last().unwrap(),
            "Run agr sync status -v for the per-session list."
        );
    }

    #[test]
    fn compact_status_has_no_line_per_ordinary_session() {
        let text = compact_status_lines(&big_report(), "gist").join("\n");
        assert!(!text.contains("claude/0.cast.gz"), "{text}");
        assert!(!text.contains("claude/719.cast.gz"), "{text}");
        assert!(text.lines().count() < 20, "{text}");
    }

    #[test]
    fn compact_status_always_lists_conflicts_failures_and_other_key_entries() {
        let mut r = big_report();
        r.items.push(item(
            "codex/f.cast.gz",
            Decision::Push,
            Outcome::Failed("boom".into()),
        ));
        r.failed = 1;
        let text = compact_status_lines(&r, "gist").join("\n");
        for want in [
            "Conflicts: 1",
            "  claude/c.cast.gz local aaaaaaaaaaaa 2 KiB, remote bbbbbbbbbbbb 4 KiB",
            "Failed: 1",
            "  codex/f.cast.gz: boom",
            "Other key: 1",
            "  claude/o.cast.gz",
        ] {
            assert!(text.contains(want), "missing {want:?} in\n{text}");
        }
    }

    #[test]
    fn compact_status_of_nothing_says_so_without_the_hint() {
        let text = compact_status_lines(&Report::default(), "gist").join("\n");
        assert!(
            text.contains("Backend: gist, 0 remote session(s)"),
            "{text}"
        );
        assert!(text.contains("No sessions."), "{text}");
        assert!(!text.contains("-v"), "{text}");
    }

    #[test]
    fn verbose_status_keeps_the_per_session_list() {
        let text = status_lines(&big_report(), "gist").join("\n");
        assert!(
            text.contains("  claude/0.cast.gz local ahead (push)"),
            "{text}"
        );
        assert!(text.contains("claude/719.cast.gz"), "{text}");
        assert!(text.contains("Totals: 3 in sync"), "{text}");
        assert!(!text.contains("per-session list"), "{text}");
    }

    #[test]
    fn compact_preview_is_one_head_a_grouped_skip_line_and_the_problems() {
        let text = compact_preview_lines(&big_report(), "push").join("\n");
        assert!(
            text.contains("Would push 721 session(s), 130.64 MiB"),
            "{text}"
        );
        assert!(
            text.contains("6 skipped: plain .cast, run `agr compress` first"),
            "{text}"
        );
        assert!(text.contains("Conflicts: 1"), "{text}");
        assert!(text.contains("claude/c.cast.gz local"), "{text}");
        assert!(text.contains("Other key: 1"), "{text}");
        assert!(
            text.contains("Run agr sync push -v for the per-session list."),
            "{text}"
        );
        assert!(!text.contains("claude/0.cast.gz"), "{text}");
        assert!(text.lines().count() < 14, "{text}");
    }

    #[test]
    fn verbose_preview_keeps_the_old_per_session_list() {
        let r = big_report();
        let text = [preview_lines(&r, "push"), problem_lines(&r)]
            .concat()
            .join("\n");
        assert!(text.contains("  claude/0.cast.gz (185.55 KiB)"), "{text}");
        assert!(text.contains("  claude/720.cast.gz"), "{text}");
    }

    #[test]
    fn compact_preview_of_nothing_says_so() {
        let text = compact_preview_lines(&Report::default(), "pull").join("\n");
        assert!(text.starts_with("Nothing to pull."), "{text}");
    }

    #[test]
    fn compact_summary_groups_skips_and_always_lists_failures() {
        let mut r = big_report();
        r.skipped = 6;
        r.failed = 1;
        r.items.push(item(
            "claude/f.cast.gz",
            Decision::Push,
            Outcome::Failed("boom".into()),
        ));
        let text = compact_summary_lines(&r, "push").join("\n");
        assert!(text.contains("6 skipped: plain .cast"), "{text}");
        assert!(text.contains("  claude/f.cast.gz: boom"), "{text}");
        assert!(!text.contains("claude/0.cast.gz"), "{text}");
        let full = summary_lines(&r).join("\n");
        assert!(full.contains("claude/0.cast.gz (plain"), "{full}");
    }

    #[test]
    fn compact_delete_preview_swaps_the_paths_for_the_hint() {
        let r = report(vec![
            item("a/x.cast.gz", Decision::Push, Outcome::Planned),
            item("a/y.cast.gz", sync(), Outcome::Nothing),
        ]);
        let text = compact_delete_preview_lines(&r).join("\n");
        assert!(text.contains("Would delete 2 remote session(s)"), "{text}");
        assert!(text.contains("agr sync delete -v"), "{text}");
        assert!(!text.contains("a/x.cast.gz"), "{text}");
        assert!(delete_preview_lines(&r)
            .join("\n")
            .contains("  a/x.cast.gz"));
    }
}
