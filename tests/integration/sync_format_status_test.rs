//! Compact and verbose status, preview and summary lines.

use crate::helpers::format_items::{item, report, sync};
use agr::sync::format::{
    compact_delete_preview_lines, compact_preview_lines, compact_status_lines,
    compact_summary_lines, delete_preview_lines, label, live_head, preview_lines, problem_lines,
    progress_line, status_lines, summary_lines, LiveLine, LoadLine,
};
use agr::sync::{Decision, Item, Outcome, Report, SkipReason};

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
    assert!(
        text.contains("Would delete 2 sessions from the remote"),
        "{text}"
    );
    assert!(text.contains("agr sync delete -v"), "{text}");
    assert!(!text.contains("a/x.cast.gz"), "{text}");
    assert!(delete_preview_lines(&r)
        .join("\n")
        .contains("  a/x.cast.gz"));
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

#[test]
fn head_names_index_path_and_size() {
    assert_eq!(
        live_head("claude/x.cast.gz", 49_700_000, 12, 721),
        "[12/721] claude/x.cast.gz 47.40 MiB"
    );
}

#[test]
fn start_shows_zero_and_the_percent_redraws_in_place() {
    let mut live = LiveLine::default();
    let head = "[12/721] claude/x.cast.gz 47.40 MiB";
    assert_eq!(
        live.start("uploading", head),
        format!("\r{head}, uploading 0%")
    );
    assert_eq!(
        live.percent(45).unwrap(),
        format!("\r{head}, uploading 45%")
    );
}

#[test]
fn an_unchanged_percent_draws_nothing() {
    let mut live = LiveLine::default();
    live.start("uploading", "[1/2] a 1 B");
    assert!(live.percent(0).is_none());
    assert!(live.percent(7).is_some());
    assert!(live.percent(7).is_none());
}

#[test]
fn a_percent_without_a_running_session_draws_nothing() {
    assert!(LiveLine::default().percent(50).is_none());
}

#[test]
fn finish_pads_over_the_live_text_and_ends_the_line() {
    let mut live = LiveLine::default();
    let head = "[1/2] claude/x.cast.gz 1 MiB";
    live.start("uploading", head);
    live.percent(100);
    let live_width = format!("{head}, uploading 100%").chars().count();
    let done = "[1/2] pushed claude/x.cast.gz (1 MiB)";
    let out = live.finish(done);
    assert_eq!(
        out,
        format!(
            "\r{done}{}\n",
            " ".repeat(live_width.saturating_sub(done.chars().count()))
        )
    );
    assert!(live.clear().is_none());
}

#[test]
fn finish_without_a_live_line_is_a_plain_line() {
    assert_eq!(
        LiveLine::default().finish("[1/1] skipped a"),
        "[1/1] skipped a\n"
    );
}

#[test]
fn clear_wipes_a_live_line_and_never_contains_escape_codes() {
    let mut live = LiveLine::default();
    let shown = live.start("downloading", "[1/1] a 1 B");
    let wipe = live.clear().unwrap();
    assert!(wipe.starts_with('\r') && wipe.ends_with('\r'), "{wipe:?}");
    assert_eq!(wipe.trim_matches('\r').trim(), "");
    assert!(!shown.contains('\u{1b}') && !wipe.contains('\u{1b}'));
    assert!(live.clear().is_none());
}

#[test]
fn load_line_names_listing_and_the_scan_count() {
    let mut load = LoadLine::default();
    assert_eq!(load.listing(), "\rListing remote sessions...");
    assert_eq!(load.scanning(312, 727), "\rScanning local sessions 312/727");
}

#[test]
fn load_line_pads_a_shorter_redraw_over_the_old_text() {
    let mut load = LoadLine::default();
    load.scanning(10, 727);
    let short = load.scanning(9, 9);
    assert_eq!(short.trim_start_matches('\r').chars().count(), 30);
}

#[test]
fn load_line_clear_returns_the_full_width_and_never_has_escape_codes() {
    let mut load = LoadLine::default();
    assert!(load.clear().is_none());
    let shown = load.scanning(312, 727);
    let wipe = load.clear().unwrap();
    let width = shown.trim_start_matches('\r').chars().count();
    assert_eq!(wipe, format!("\r{}\r", " ".repeat(width)));
    assert!(!shown.contains('\u{1b}') && !wipe.contains('\u{1b}'));
    assert!(load.clear().is_none());
}
