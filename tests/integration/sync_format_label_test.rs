//! Labels, status lines and progress lines against hand built items.

use crate::helpers::format_items::{item, report, sync};
use agr::sync::format::{label, progress_line, status_lines};
use agr::sync::{Decision, Outcome, Report, SkipReason};

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
fn progress_line_covers_done_skipped_and_failed() {
    let p = "claude/x.cast.gz";
    let done = item(p, Decision::Push, Outcome::Done(12 * 1024 * 1024));
    let skip = item(
        p,
        Decision::Conflict,
        Outcome::Skipped("conflict, use --overwrite".into()),
    );
    let fail = item(p, Decision::Push, Outcome::Failed("upload broke".into()));
    let gone = item(p, Decision::Push, Outcome::Done(0));
    let got = [
        progress_line("pushed", &done, 3, 40),
        progress_line("pulled", &done, 1, 2),
        progress_line("deleted", &gone, 2, 2),
        progress_line("pushed", &skip, 4, 40),
        progress_line("pushed", &fail, 5, 40),
    ];
    assert_eq!(
        got,
        [
            "[3/40] pushed claude/x.cast.gz (12 MiB)",
            "[1/2] pulled claude/x.cast.gz (12 MiB)",
            "[2/2] deleted claude/x.cast.gz",
            "[4/40] skipped claude/x.cast.gz: conflict, use --overwrite",
            "[5/40] failed claude/x.cast.gz: upload broke",
        ]
    );
}
