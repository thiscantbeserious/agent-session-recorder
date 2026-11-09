//! Compact `agr sync status`, one summary line per category.

use crate::sync::engine::run::Report;

/// Backend line, one line per category with count and size, then what needs action.
pub fn compact_status_lines(_report: &Report, _backend: &str) -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::compact_status_lines;
    use crate::sync::app::format::status_lines;
    use crate::sync::app::format::tests::{item, report, sync};
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
        assert_eq!(lines[0], "Backend: gist, 735 remote session(s)");
        assert_eq!(lines[1], "721 to push (130.64 MiB)");
        assert_eq!(lines[2], "2 to pull (6 MiB)");
        assert_eq!(lines[3], "3 in sync (3 MiB)");
        assert_eq!(lines[4], "1 conflict");
        assert_eq!(lines[5], "1 skipped: sealed with another key");
        assert_eq!(lines[6], "6 skipped: plain .cast, run `agr compress` first");
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
            text.contains("  claude/0.cast.gz local only (push)"),
            "{text}"
        );
        assert!(text.contains("claude/719.cast.gz"), "{text}");
        assert!(text.contains("Totals: 3 in sync"), "{text}");
        assert!(!text.contains("per-session list"), "{text}");
    }
}
