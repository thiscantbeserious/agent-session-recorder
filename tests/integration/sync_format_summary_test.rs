//! Summary, preview, problem and privacy lines.

use crate::helpers::format_items::{item, report, sync};
use agr::sync::format::{
    delete_preview_lines, preview_lines, privacy_lines, problem_lines, rate_limit_lines,
    summary_lines,
};
use agr::sync::{Decision, Outcome, Report, SkipReason};

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
    assert!(
        text.contains("Would delete 1 session from the remote"),
        "{text}"
    );
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
