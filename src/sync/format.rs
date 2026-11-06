//! Plain text lines for the sync commands, colour is added by the caller via theme helpers.

use humansize::{format_size, BINARY};

use super::plan::Decision;
use super::run::{Item, Outcome, Report};

const ORDER: [&str; 7] = [
    "in sync",
    "local ahead (push)",
    "local only (push)",
    "remote ahead (pull)",
    "remote only (pull)",
    "conflict",
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
        Decision::OtherKey | Decision::UnknownVersion | Decision::Skip(_) => ORDER[6],
    }
}

/// Backend line, one line per session and totals per state.
pub fn status_lines(report: &Report, backend: &str) -> Vec<String> {
    let remote = report.items.iter().filter(|i| i.on_remote).count();
    let mut lines = vec![format!("Backend: {backend}, {remote} remote session(s)")];
    if report.items.is_empty() {
        lines.push("No sessions.".into());
        return lines;
    }
    lines.extend(report.items.iter().map(status_line));
    lines.push(totals(report));
    lines
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
    let total: u64 = planned.iter().map(|i| bytes(i)).sum();
    let mut lines = vec![format!(
        "Would {verb} {} session(s), {}",
        planned.len(),
        size(total)
    )];
    lines.extend(
        planned
            .iter()
            .map(|i| format!("  {} ({})", i.path, size(bytes(i)))),
    );
    lines
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

/// Result of a real run, counts, skipped, conflicts with both hashes and sizes, failures.
pub fn summary_lines(report: &Report) -> Vec<String> {
    let mut lines = Vec::new();
    let counts = [
        ("Pushed", report.pushed),
        ("Pulled", report.pulled),
        ("Deleted", report.deleted),
    ];
    lines.extend(
        counts
            .iter()
            .filter(|(_, n)| *n > 0)
            .map(|(name, n)| format!("{name}: {n}")),
    );
    if report.bytes > 0 {
        lines.push(format!("Transferred: {}", size(report.bytes)));
    }
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

fn short(hash: Option<&str>) -> String {
    hash.unwrap_or("-").chars().take(12).collect()
}

fn size(bytes: u64) -> String {
    format_size(bytes, BINARY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::plan::SkipReason;

    fn item(path: &str, decision: Decision, outcome: Outcome) -> Item {
        Item {
            path: path.into(),
            decision,
            outcome,
            size: 2048,
            on_remote: true,
            on_local: true,
            local_hash: Some("a".repeat(64)),
            remote_hash: Some("b".repeat(64)),
            remote_size: 4096,
        }
    }

    fn sync() -> Decision {
        Decision::InSync {
            refresh_index: false,
        }
    }

    fn report(items: Vec<Item>) -> Report {
        Report {
            items,
            ..Report::default()
        }
    }

    #[test]
    fn labels_cover_every_state() {
        let mut local_only = item("a/1", Decision::Push, Outcome::Planned);
        local_only.on_remote = false;
        let mut remote_only = item("a/2", Decision::Pull, Outcome::Planned);
        remote_only.on_local = false;
        let cases = [
            (item("a/3", sync(), Outcome::Nothing), "in sync"),
            (
                item("a/4", Decision::Push, Outcome::Planned),
                "local ahead (push)",
            ),
            (local_only, "local only (push)"),
            (
                item("a/5", Decision::Pull, Outcome::Planned),
                "remote ahead (pull)",
            ),
            (remote_only, "remote only (pull)"),
            (
                item("a/6", Decision::Conflict, Outcome::Nothing),
                "conflict",
            ),
            (
                item("a/7", Decision::Skip(SkipReason::Plain), Outcome::Nothing),
                "skipped",
            ),
            (item("a/8", Decision::OtherKey, Outcome::Nothing), "skipped"),
        ];
        for (item, want) in cases {
            assert_eq!(label(&item), want, "{}", item.path);
        }
    }

    #[test]
    fn status_shows_backend_sessions_and_totals() {
        let r = report(vec![
            item("claude/a.cast.gz", sync(), Outcome::Nothing),
            item("claude/b.cast.gz", Decision::Push, Outcome::Planned),
            item(
                "claude/c.cast.gz",
                Decision::Skip(SkipReason::Plain),
                Outcome::Skipped("plain .cast, run `agr compress` first".into()),
            ),
        ]);
        let text = status_lines(&r, "gist").join("\n");
        assert!(
            text.contains("Backend: gist, 3 remote session(s)"),
            "{text}"
        );
        assert!(text.contains("in sync"), "{text}");
        assert!(text.contains("claude/b.cast.gz"), "{text}");
        assert!(text.contains("local ahead (push)"), "{text}");
        assert!(text.contains("2 KiB"), "{text}");
        assert!(
            text.contains("plain .cast, run `agr compress` first"),
            "{text}"
        );
        assert!(
            text.contains("Totals: 1 in sync, 1 local ahead (push), 1 skipped"),
            "{text}"
        );
    }

    #[test]
    fn status_of_nothing_says_so() {
        let text = status_lines(&Report::default(), "gist").join("\n");
        assert!(
            text.contains("Backend: gist, 0 remote session(s)"),
            "{text}"
        );
        assert!(text.contains("No sessions"), "{text}");
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
