//! `agr sync migrate`: what the report names when gists are kept or an append fails.

use crate::helpers::legacy_fake::{Listed, Rig};
use agr::sync::testing::{migrate_preview, migrate_run, Listing, PackStore, Segment, StoreError};
use std::io::Read;

const A: &str = "claude/a.cast.gz";

#[test]
fn a_v1_gist_of_our_key_with_an_extra_file_is_named_and_never_deleted() {
    let rig = Rig::new();
    let id = rig.legacy.v1_with_extra(&rig.key, (A, 3), b"body");
    let store = Listed(&rig.mem, &rig.legacy);
    let report = migrate_run(&rig.env(&store)).unwrap();
    let named = |l: &String| l.contains(&format!("gh gist delete {id}"));
    assert!(report.lines.iter().any(named), "{:?}", report.lines);
    assert_eq!((report.deleted, rig.legacy.deletes().len()), (0, 0));
    assert!(report.kept.contains(&id), "{report:?}");
}

#[test]
fn preview_counts_an_own_v1_gist_with_an_extra_file_as_blocked_not_as_not_ours() {
    let rig = Rig::new();
    rig.legacy.v1_with_extra(&rig.key, (A, 3), b"body");
    let store = Listed(&rig.mem, &rig.legacy);
    let report = migrate_preview(&rig.env(&store)).unwrap();
    let line = report.lines.join("\n");
    let blocked = "1 blocked (own gist with extra files, fix by hand, sync stays blocked)";
    assert!(line.contains(blocked), "{line}");
    assert!(line.contains("0 not ours (kept)"), "{line}");
}

#[test]
fn a_v1_gist_with_an_invalid_path_is_kept_named_and_not_moved() {
    let rig = Rig::new();
    let id = rig.legacy.v1(&rig.key, ("claude/a.cast", 3), b"body", None);
    let store = Listed(&rig.mem, &rig.legacy);
    let report = migrate_run(&rig.env(&store)).unwrap();
    let named = |l: &String| l.contains(&format!("gh gist delete {id}"));
    assert!(report.lines.iter().any(named), "{:?}", report.lines);
    assert_eq!((rig.mem.appends(), rig.legacy.deletes().len()), (0, 0));
}

/// Every write of the pack store fails, the reads work.
struct NoAppend<'a>(Listed<'a>);

impl PackStore for NoAppend<'_> {
    fn name(&self) -> &'static str {
        "no-append"
    }
    fn check(&self) -> Result<(), StoreError> {
        self.0.check()
    }
    fn list(&self) -> Result<Listing, StoreError> {
        self.0.list()
    }
    fn read_index(&self, id: &str) -> Result<String, StoreError> {
        self.0.read_index(id)
    }
    fn create(&self, tag: &str, index: &str) -> Result<String, StoreError> {
        self.0.create(tag, index)
    }
    fn append(&self, _: &str, _: u64, _: &str, _: &[Segment], _: &str) -> Result<(), StoreError> {
        Err(StoreError::Other("HTTP 500".into()))
    }
    fn read(&self, id: &str, offset: u64, len: u64) -> Result<Box<dyn Read>, StoreError> {
        self.0.read(id, offset, len)
    }
    fn delete(&self, id: &str) -> Result<(), StoreError> {
        self.0.delete(id)
    }
}

#[test]
fn an_append_error_still_returns_the_report_with_the_kept_lines() {
    let rig = Rig::new();
    let bad = rig
        .legacy
        .v1(&rig.key, (A, 3), b"body", Some(&"ab".repeat(32)));
    rig.legacy.v1(&rig.key, ("claude/b.cast.gz", 1), b"b", None);
    let store = NoAppend(Listed(&rig.mem, &rig.legacy));
    let report = migrate_run(&rig.env(&store));
    let report = report.expect("the report with the kept lines, not a bare error");
    assert!(report.failed.is_some(), "{report:?}");
    let named = |l: &String| l.contains(&format!("gh gist delete {bad}"));
    assert!(report.lines.iter().any(named), "{:?}", report.lines);
    assert_eq!(rig.legacy.deletes().len(), 0);
}

#[test]
fn an_unreachable_pull_stops_the_run_with_work_left_and_never_advises_a_delete() {
    let rig = Rig::new();
    let id = rig.legacy.v1(&rig.key, (A, 3), b"body", None);
    rig.legacy.pull_unreachable(true);
    let store = Listed(&rig.mem, &rig.legacy);
    let report = migrate_run(&rig.env(&store)).unwrap();
    let advised = |l: &String| l.contains(&format!("gh gist delete {id}"));
    assert!(!report.lines.iter().any(advised), "{:?}", report.lines);
    assert!(report.left > 0, "{report:?}");
    assert!(!report.kept.contains(&id), "{report:?}");
    assert_eq!((report.deleted, rig.legacy.deletes().len()), (0, 0));
}

/// The `fail_at`th append (counted from 1) fails with `error`, the others and all reads work.
struct FailNth<'a> {
    inner: Listed<'a>,
    fail_at: usize,
    error: StoreError,
    calls: std::sync::atomic::AtomicUsize,
}

impl<'a> FailNth<'a> {
    fn new(inner: Listed<'a>, fail_at: usize, error: StoreError) -> Self {
        Self {
            inner,
            fail_at,
            error,
            calls: 0.into(),
        }
    }
}

impl PackStore for FailNth<'_> {
    fn name(&self) -> &'static str {
        "fail-nth"
    }
    fn check(&self) -> Result<(), StoreError> {
        self.inner.check()
    }
    fn list(&self) -> Result<Listing, StoreError> {
        self.inner.list()
    }
    fn read_index(&self, id: &str) -> Result<String, StoreError> {
        self.inner.read_index(id)
    }
    fn create(&self, tag: &str, index: &str) -> Result<String, StoreError> {
        self.inner.create(tag, index)
    }
    fn append(&self, id: &str, n: u64, e: &str, s: &[Segment], i: &str) -> Result<(), StoreError> {
        let call = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        if call == self.fail_at {
            return Err(self.error.clone());
        }
        self.inner.append(id, n, e, s, i)
    }
    fn read(&self, id: &str, offset: u64, len: u64) -> Result<Box<dyn Read>, StoreError> {
        self.inner.read(id, offset, len)
    }
    fn delete(&self, id: &str) -> Result<(), StoreError> {
        self.inner.delete(id)
    }
}

/// Two sessions that need two appends: the first fills a pack, the second does not fit behind it.
fn two_appends(rig: &Rig) {
    let big = vec![7u8; crate::helpers::legacy_fake::TARGET as usize - 10];
    rig.legacy.v1(&rig.key, (A, 1), &big, None);
    rig.legacy
        .v1(&rig.key, ("claude/b.cast.gz", 1), b"small", None);
}

#[test]
fn a_failed_second_append_still_counts_what_the_first_one_moved() {
    let rig = Rig::new();
    two_appends(&rig);
    let store = FailNth::new(
        Listed(&rig.mem, &rig.legacy),
        2,
        StoreError::Other("HTTP 500".into()),
    );
    let report = migrate_run(&rig.env(&store)).unwrap();
    assert!(report.failed.is_some(), "{report:?}");
    assert_eq!(report.moved, 1, "{report:?}");
    assert_eq!(rig.legacy.deletes().len(), 0);
}

#[test]
fn a_rate_limited_append_stops_with_work_left_and_the_next_run_finishes() {
    let rig = Rig::new();
    two_appends(&rig);
    let limited = FailNth::new(Listed(&rig.mem, &rig.legacy), 2, StoreError::RateLimited);
    let report = migrate_run(&rig.env(&limited)).unwrap();
    assert!(report.failed.is_none(), "{report:?}");
    assert!(report.left > 0, "{report:?}");
    let first = rig.legacy.deletes().len();
    assert!(first < 2, "unverified gists were deleted");
    let store = Listed(&rig.mem, &rig.legacy);
    let again = migrate_run(&rig.env(&store)).unwrap();
    assert!(again.failed.is_none() && again.left == 0, "{again:?}");
    assert_eq!(rig.legacy.deletes().len(), 2);
}
