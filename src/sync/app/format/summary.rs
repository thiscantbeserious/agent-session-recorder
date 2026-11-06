//! Lines for previews, notices, problems and run summaries.

use super::{short, size};
use crate::sync::engine::plan::Decision;
use crate::sync::engine::run::{Item, Outcome, Report};

/// What a push or pull would do (`verb` is `push` or `pull`), with sizes.
pub fn preview_lines(report: &Report, verb: &str) -> Vec<String> {
    let planned: Vec<&Item> = report
        .items
        .iter()
        .filter(|i| i.outcome == Outcome::Planned)
        .collect();
    if planned.is_empty() {
        return vec![format!("Nothing to {verb}.")];
    }
    let bytes = |i: &Item| {
        if verb == "pull" {
            i.remote_size
        } else {
            i.size
        }
    };
    let total: u64 = planned.iter().copied().map(bytes).sum();
    let head = format!("Would {verb} {} session(s), {}", planned.len(), size(total));
    let rows = planned
        .iter()
        .map(|i| format!("  {} ({})", i.path, size(bytes(i))));
    std::iter::once(head).chain(rows).collect()
}

/// First push notice, privacy and key backup.
pub fn privacy_lines() -> Vec<String> {
    [
        "Syncing uploads your sessions to secret GitHub gists.",
        "A secret gist is not listed, but anyone with the link can read it. Sessions can contain secrets typed in the terminal.",
        "Every session is encrypted on this machine first, gists only hold ciphertext and an opaque token.",
        "Back up your key with agr sync key export. Losing every copy of the key leaves synced sessions unrecoverable.",
    ]
    .map(String::from)
    .to_vec()
}

/// Skipped, conflicts with both hashes and sizes, failures and the index warning.
pub fn problem_lines(report: &Report) -> Vec<String> {
    let mut lines = detail("Skipped", report.skipped, report, skipped_line);
    lines.extend(detail("Conflicts", report.conflicts, report, conflict_line));
    lines.extend(detail("Failed", report.failed, report, failed_line));
    lines.extend(report.warning.iter().map(|w| format!("Warning: {w}")));
    lines
}

/// Remote copies `agr sync delete` would remove.
pub fn delete_preview_lines(report: &Report) -> Vec<String> {
    let remote: Vec<&Item> = report.items.iter().filter(|i| i.on_remote).collect();
    if remote.is_empty() {
        return vec!["Nothing to delete.".into()];
    }
    let mut lines = vec![format!("Would delete {} remote session(s)", remote.len())];
    lines.extend(remote.iter().map(|i| format!("  {}", i.path)));
    lines.push("A later `agr sync push` uploads the session again.".into());
    lines
}

/// Two lines when GitHub stopped the run, `left` sessions were not done.
pub fn rate_limit_lines(left: usize) -> [String; 2] {
    [
        "GitHub rate limit reached, sync stopped".into(),
        format!("{left} left, please run again later"),
    ]
}

/// Result of a real run, counts, skipped, conflicts with both hashes and sizes, failures.
pub fn summary_lines(report: &Report) -> Vec<String> {
    let mut lines = count_lines(report);
    lines.extend(
        problem_lines(report)
            .into_iter()
            .filter(|l| !l.starts_with("Warning: ")),
    );
    if lines.is_empty() {
        lines.push("Nothing to do.".into());
    }
    lines.extend(report.warning.iter().map(|w| format!("Warning: {w}")));
    lines
}

fn count_lines(report: &Report) -> Vec<String> {
    let counts = [
        ("Pushed", report.pushed),
        ("Pulled", report.pulled),
        ("Deleted", report.deleted),
    ];
    let mut lines: Vec<String> = counts
        .iter()
        .filter(|(_, n)| *n > 0)
        .map(|(name, n)| format!("{name}: {n}"))
        .collect();
    if report.bytes > 0 {
        lines.push(format!("Transferred: {}", size(report.bytes)));
    }
    lines
}

fn detail(
    name: &str,
    count: usize,
    report: &Report,
    line: fn(&Item) -> Option<String>,
) -> Vec<String> {
    if count == 0 {
        return Vec::new();
    }
    let mut lines = vec![format!("{name}: {count}")];
    lines.extend(report.items.iter().filter_map(line));
    lines
}

fn skipped_line(item: &Item) -> Option<String> {
    match (&item.outcome, item.decision) {
        (Outcome::Skipped(_), Decision::Conflict) => None,
        (Outcome::Skipped(why), _) => Some(format!("  {} ({why})", item.path)),
        _ => None,
    }
}

fn conflict_line(item: &Item) -> Option<String> {
    let (Outcome::Skipped(_), Decision::Conflict) = (&item.outcome, item.decision) else {
        return None;
    };
    Some(format!(
        "  {} local {} {}, remote {} {}",
        item.path,
        short(item.local_hash.as_deref()),
        size(item.size),
        short(item.remote_hash.as_deref()),
        size(item.remote_size)
    ))
}

fn failed_line(item: &Item) -> Option<String> {
    match &item.outcome {
        Outcome::Failed(why) => Some(format!("  {}: {why}", item.path)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::app::format::tests::{item, report, sync};
    use crate::sync::engine::plan::SkipReason;

    #[test]
    fn rate_limit_lines_name_the_stop_and_the_sessions_left() {
        assert_eq!(
            rate_limit_lines(3),
            [
                "GitHub rate limit reached, sync stopped",
                "3 left, please run again later"
            ]
        );
    }

    #[test]
    fn preview_lists_planned_with_sizes_only() {
        let r = report(vec![
            item("a/x.cast.gz", Decision::Push, Outcome::Planned),
            item("a/y.cast.gz", sync(), Outcome::Nothing),
        ]);
        let text = preview_lines(&r, "push").join("\n");
        assert!(text.contains("Would push 1 session(s), 2 KiB"), "{text}");
        assert!(text.contains("a/x.cast.gz (2 KiB)"), "{text}");
        assert!(!text.contains("a/y.cast.gz"), "{text}");
    }

    #[test]
    fn preview_without_candidates_says_nothing_to_do() {
        let r = report(vec![item("a/y.cast.gz", sync(), Outcome::Nothing)]);
        assert_eq!(preview_lines(&r, "pull"), ["Nothing to pull."]);
    }

    #[test]
    fn summary_counts_and_details() {
        let mut r = report(vec![
            item("a/ok.cast.gz", Decision::Push, Outcome::Done(10)),
            item(
                "a/c.cast.gz",
                Decision::Conflict,
                Outcome::Skipped("conflict".into()),
            ),
            item(
                "a/f.cast.gz",
                Decision::Push,
                Outcome::Failed("boom".into()),
            ),
            item(
                "a/s.cast.gz",
                Decision::Skip(SkipReason::Locked),
                Outcome::Skipped("still recording".into()),
            ),
        ]);
        r.pushed = 1;
        r.conflicts = 1;
        r.failed = 1;
        r.skipped = 1;
        r.bytes = 10;
        let text = summary_lines(&r).join("\n");
        for want in [
            "Pushed: 1",
            "Skipped: 1",
            "a/s.cast.gz (still recording)",
            "Conflicts: 1",
            "a/c.cast.gz local aaaaaaaaaaaa 2 KiB, remote bbbbbbbbbbbb 4 KiB",
            "Failed: 1",
            "a/f.cast.gz: boom",
            "Transferred: 10 B",
        ] {
            assert!(text.contains(want), "missing {want:?} in\n{text}");
        }
    }

    #[test]
    fn problem_lines_hold_only_skips_conflicts_failures_and_warning() {
        let mut r = report(vec![
            item(
                "a/c.cast.gz",
                Decision::Conflict,
                Outcome::Skipped("conflict".into()),
            ),
            item(
                "a/f.cast.gz",
                Decision::Push,
                Outcome::Failed("boom".into()),
            ),
        ]);
        r.conflicts = 1;
        r.failed = 1;
        r.pushed = 5;
        r.warning = Some("w".into());
        let text = problem_lines(&r).join("\n");
        assert!(text.contains("Conflicts: 1"), "{text}");
        assert!(text.contains("Failed: 1"), "{text}");
        assert!(text.contains("Warning: w"), "{text}");
        assert!(!text.contains("Pushed"), "{text}");
        assert!(problem_lines(&Report::default()).is_empty());
    }

    #[test]
    fn delete_preview_lists_only_remote_sessions() {
        let mut local_only = item("a/l.cast.gz", Decision::Push, Outcome::Planned);
        local_only.on_remote = false;
        let r = report(vec![
            item("a/r.cast.gz", sync(), Outcome::Nothing),
            local_only,
        ]);
        let text = delete_preview_lines(&r).join("\n");
        assert!(text.contains("Would delete 1 remote session(s)"), "{text}");
        assert!(text.contains("a/r.cast.gz"), "{text}");
        assert!(!text.contains("a/l.cast.gz"), "{text}");
        assert_eq!(
            delete_preview_lines(&report(vec![])),
            ["Nothing to delete."]
        );
    }

    #[test]
    fn privacy_notice_names_the_risks() {
        let text = privacy_lines().join("\n");
        assert!(text.contains("anyone with the link"), "{text}");
        assert!(text.contains("encrypted"), "{text}");
        assert!(text.contains("agr sync key export"), "{text}");
        assert!(text.contains("unrecoverable"), "{text}");
    }

    #[test]
    fn summary_includes_index_warning_and_pulled_and_deleted() {
        let mut r = Report {
            pulled: 2,
            deleted: 1,
            warning: Some("index rebuilt".into()),
            ..Report::default()
        };
        r.bytes = 0;
        let text = summary_lines(&r).join("\n");
        assert!(text.contains("Pulled: 2"), "{text}");
        assert!(text.contains("Deleted: 1"), "{text}");
        assert!(text.contains("Warning: index rebuilt"), "{text}");
    }
}
