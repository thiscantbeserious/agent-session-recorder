//! `agr sync delete` pattern selection, pure.

use agr::sync::{select_remote, SelectError};

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

fn remote() -> Vec<String> {
    paths(&[
        "claude/a.cast.gz",
        "claude/b.cast.gz",
        "codex/a1.cast.gz",
        "codex/ab.cast.gz",
        "codex/abc.cast.gz",
    ])
}

#[test]
fn star_and_question_mark_match_remote_sessions_only() {
    // `claude/c.cast.gz` exists only locally, it is not in the remote list and never matches
    let hits = select_remote(&paths(&["claude/*"]), false, &remote()).unwrap();
    assert_eq!(hits, vec!["claude/a.cast.gz", "claude/b.cast.gz"]);
    let hits = select_remote(&paths(&["codex/a?.cast.gz"]), false, &remote()).unwrap();
    assert_eq!(hits, vec!["codex/a1.cast.gz", "codex/ab.cast.gz"]);
}

#[test]
fn all_selects_every_live_remote_session() {
    let hits = select_remote(&[], true, &remote()).unwrap();
    assert_eq!(hits.len(), 5);
}

#[test]
fn pattern_without_match_is_an_error() {
    let err = select_remote(&paths(&["claude/*", "x*"]), false, &remote()).unwrap_err();
    assert_eq!(err, SelectError::NoMatch("x*".into()));
    let err = select_remote(&paths(&["claude/*", "y?"]), false, &remote()).unwrap_err();
    assert_eq!(err, SelectError::NoMatch("y?".into()));
}
