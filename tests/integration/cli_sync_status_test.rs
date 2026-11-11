//! `agr sync status` and auto push output against the fake backend.

use crate::helpers::cli_sync::*;
use std::fs;

#[test]
fn status_lists_states_totals_without_colour_and_shows_the_auto_push_error() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "one");
    assert_eq!(m.run(&["push", "--yes"]).code, 0);
    m.session("claude", "b.cast.gz", "two");
    m.session("claude", "plain.cast", "plain");
    let state = m.home.path().join(".config/agr/state.toml");
    fs::write(
        &state,
        "sync_privacy_ack = true\nlast_auto_push_error = \"offline\"\n",
    )
    .unwrap();
    let out = m.run(&["status", "-v"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(!out.all().contains('\u{1b}'));
    assert!(
        out.stdout
            .contains("Backend: directory, 1 remote session(s)"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("claude/a.cast.gz in sync"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("claude/b.cast.gz local only (push)"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("agr compress"), "{}", out.stdout);
    assert!(out.stdout.contains("Totals:"), "{}", out.stdout);
    assert!(
        out.all().contains("Last automatic push failed: offline"),
        "{}",
        out.all()
    );
    let again = m.run(&["status"]);
    assert!(
        !again.all().contains("Last automatic push failed"),
        "{}",
        again.all()
    );
    assert!(!m.state().contains("offline"));
}

#[test]
fn default_status_is_a_compact_summary_and_verbose_lists_sessions() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "one");
    m.session("claude", "plain.cast", "plain");
    let compact = m.run(&["status"]);
    assert_eq!(compact.code, 0, "{}", compact.all());
    for want in [
        "1 to push (",
        "Run agr sync status -v for the per-session list.",
    ] {
        assert!(compact.stdout.contains(want), "{}", compact.stdout);
    }
    assert!(
        !compact.stdout.contains("claude/a.cast.gz"),
        "{}",
        compact.stdout
    );
    let verbose = m.run(&["status", "--verbose"]);
    assert!(verbose
        .stdout
        .contains("claude/a.cast.gz local only (push)"));
    assert!(!verbose.stdout.contains("per-session list"));
}

#[cfg(unix)]
#[test]
fn auto_push_failure_is_stored_one_line_and_shown_once_by_status() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "one");
    assert_eq!(m.run(&["push", "--yes"]).code, 0);
    m.session("claude", "b.cast.gz", "two");
    let bin = fake_bin(&[("git", "exit 0"), ("gh", "exit 1")]);
    let mut cmd = m.base(&["push", "--auto", "claude/b.cast.gz"]);
    cmd.env("PATH", bin.path())
        .env_remove("AGR_SYNC_TEST_BACKEND_DIR");
    let out = m.exec(cmd, None);
    assert_eq!(out.code, 0, "{}", out.all());
    let state = m.state();
    let line = state
        .lines()
        .find(|l| l.starts_with("last_auto_push_error"))
        .unwrap();
    assert!(line.contains("gh"), "{line}");
    let status = m.run(&["status"]);
    assert!(
        status.all().contains("Last automatic push failed"),
        "{}",
        status.all()
    );
    assert!(!m.state().contains("last_auto_push_error"));
}

#[test]
fn auto_push_pushes_one_session_quietly_without_prompt_or_privacy_notice() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "one");
    m.session("claude", "b.cast.gz", "two");
    assert_eq!(m.run(&["push", "--yes", "claude/a.cast.gz"]).code, 0);
    let out = m.run(&["push", "--auto", "claude/b.cast.gz"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert_eq!(out.all(), "", "auto push prints nothing");
    assert_eq!(remote_files(remote.path(), "part.000001").len(), 1);
    assert!(!m.state().contains("last_auto_push_error"), "{}", m.state());
}

#[test]
fn auto_push_without_privacy_ack_pushes_nothing_and_stays_quiet() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "one");
    let out = m.run(&["push", "--auto", "claude/a.cast.gz"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(remote_files(remote.path(), "manifest").is_empty());
    assert!(!m.key_path().exists());
    assert_eq!(out.all(), "");
    assert_eq!(m.state(), "");
}

#[test]
fn status_on_v1_index_hints_migrate() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "one");
    let index = agr::sync::testing::index::path_for(&m.storage);
    fs::write(index, r#"{"version":1,"entries":{}}"#).unwrap();
    let out = m.run(&["status"]);
    assert_ne!(out.code, 0, "{}", out.all());
    assert!(out.all().contains("agr sync migrate"), "{}", out.all());
}
