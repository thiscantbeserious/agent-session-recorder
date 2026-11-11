//! The decision table and `--overwrite` resolution.

use agr::sync::testing::{decide, resolve, Command, Input, Local, Remote};
use agr::sync::{Decision, SkipReason};

fn h(s: &str) -> String {
    s.to_string()
}

fn input(local: Local, remote: Remote, last: Option<&str>) -> Input {
    Input {
        local,
        remote,
        last: last.map(h),
    }
}

fn both(l: &str, r: &str, last: Option<&str>) -> Decision {
    decide(&input(Local::Hash(h(l)), Remote::Hash(h(r)), last))
}

#[test]
fn equal_hashes_are_in_sync() {
    assert_eq!(
        both("a", "a", Some("a")),
        Decision::InSync {
            refresh_index: false
        }
    );
}

#[test]
fn equal_hashes_without_index_refresh_the_index() {
    let expected = Decision::InSync {
        refresh_index: true,
    };
    assert_eq!(both("a", "a", None), expected);
    assert_eq!(both("b", "b", Some("a")), expected);
}

#[test]
fn local_only_pushes() {
    let d = decide(&input(Local::Hash(h("a")), Remote::Absent, None));
    assert_eq!(d, Decision::Push);
}

#[test]
fn remote_only_pulls_also_when_index_remembers_it() {
    let d = decide(&input(Local::Absent, Remote::Hash(h("a")), None));
    assert_eq!(d, Decision::Pull);
    let d = decide(&input(Local::Absent, Remote::Hash(h("a")), Some("a")));
    assert_eq!(d, Decision::Pull);
}

#[test]
fn local_changed_pushes() {
    assert_eq!(both("new", "old", Some("old")), Decision::Push);
}

#[test]
fn remote_changed_pulls() {
    assert_eq!(both("old", "new", Some("old")), Decision::Pull);
}

#[test]
fn both_changed_is_conflict() {
    assert_eq!(both("l", "r", Some("old")), Decision::Conflict);
}

#[test]
fn no_index_and_different_hash_is_conflict() {
    assert_eq!(both("l", "r", None), Decision::Conflict);
}

#[test]
fn mtime_is_not_an_input() {
    // Input has no time field, a newer local file with a changed remote stays a conflict.
    assert_eq!(
        both("local-newer", "remote", Some("old")),
        Decision::Conflict
    );
}

#[test]
fn other_key_and_unknown_version_are_reported() {
    let l = Local::Hash(h("a"));
    assert_eq!(
        decide(&input(l.clone(), Remote::OtherKey, None)),
        Decision::OtherKey
    );
    assert_eq!(
        decide(&input(l, Remote::UnknownVersion, None)),
        Decision::UnknownVersion
    );
    assert_eq!(
        decide(&input(Local::Absent, Remote::OtherKey, None)),
        Decision::OtherKey
    );
}

#[test]
fn local_skips_win_with_their_reason() {
    for reason in [SkipReason::Plain, SkipReason::Locked, SkipReason::OverCap] {
        let d = decide(&input(Local::Skipped(reason), Remote::Hash(h("a")), None));
        assert_eq!(d, Decision::Skip(reason));
    }
}

#[test]
fn overwrite_turns_conflict_into_command_direction() {
    let c = Decision::Conflict;
    assert_eq!(resolve(c, Command::Push, true), Decision::Push);
    assert_eq!(resolve(c, Command::Pull, true), Decision::Pull);
    assert_eq!(resolve(c, Command::Push, false), Decision::Conflict);
}

#[test]
fn overwrite_leaves_other_decisions_alone() {
    for d in [
        Decision::Push,
        Decision::Pull,
        Decision::OtherKey,
        Decision::Skip(SkipReason::Locked),
        Decision::InSync {
            refresh_index: false,
        },
    ] {
        assert_eq!(resolve(d, Command::Pull, true), d);
    }
}
