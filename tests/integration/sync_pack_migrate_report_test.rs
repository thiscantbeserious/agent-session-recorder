//! `agr sync migrate`: what the report names when gists are kept or an append fails.

use crate::helpers::legacy_fake::{Listed, Rig};
use agr::sync::testing::{migrate_run, Listing, PackStore, Segment, StoreError};
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
