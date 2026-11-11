//! Decision table rows of the tombstone (`Remote::Deleted`), the rest lives with the engine tests.

use agr::sync::testing::{decide, Input, Local, Remote};
use agr::sync::{Decision, SkipReason};

fn input(local: Local, last: Option<&str>) -> Input {
    Input {
        local,
        remote: Remote::Deleted { gen: 5 },
        last: last.map(String::from),
    }
}

fn hash(h: &str) -> Local {
    Local::Hash(h.into())
}

#[test]
fn deleted_remote_is_not_pushed_back() {
    for last in [Some("a"), None] {
        let d = decide(&input(hash("a"), last));
        assert_eq!(d, Decision::Skip(SkipReason::DeletedOnRemote), "{last:?}");
    }
}

#[test]
fn local_edit_after_remote_delete_is_a_fork() {
    let d = decide(&input(hash("edited"), Some("a")));
    assert_eq!(d, Decision::Fork);
}

#[test]
fn local_absent_remote_deleted_is_in_sync() {
    let d = decide(&input(Local::Absent, Some("a")));
    assert!(matches!(d, Decision::InSync { .. }), "{d:?}");
}
