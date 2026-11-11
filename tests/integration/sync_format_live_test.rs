//! The live line, the load line and the line head, byte for byte.

use agr::sync::format::{live_head, LiveLine, LoadLine};

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
