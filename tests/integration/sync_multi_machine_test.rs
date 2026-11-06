//! Three machines on one remote, interleaved runs, memory backend only

use crate::helpers::sync_net::{
    assert_settled, mach, only_id, overwrite, remote_bytes, Mach, Net, PATH,
};
use agr::sync::backend::BackendError;
use agr::sync::crypto::Key;
use agr::sync::memory::MemoryBackend;
use agr::sync::plan::Decision;
use agr::sync::run::{Options, Outcome, Report};

const CHANGED: &str = "changed on the remote, run again";

fn opts() -> Options {
    Options::default()
}

/// A pushes v0 and B pulls it, both are synced at gen 1.
fn synced_pair() -> (Mach, Mach, MemoryBackend, Key) {
    let (a, b, backend, key) = (mach(), mach(), MemoryBackend::new(), Key::generate());
    a.write("v0");
    assert_eq!(a.push(&backend, &key, &opts()).pushed, 1);
    assert_eq!(b.pull(&backend, &key, &opts()).pulled, 1);
    (a, b, backend, key)
}

fn outcomes(report: &Report) -> Vec<Outcome> {
    report.items.iter().map(|i| i.outcome.clone()).collect()
}

/// B edits and pushes in the middle of A's push, B wins, A is reported.
fn a_loses_the_race(a: &Mach, b: &Mach, backend: &MemoryBackend, key: &Key) -> Report {
    a.write("fromA");
    b.write("fromB");
    let net = Net::new(backend);
    net.before(|| assert_eq!(b.push(backend, key, &opts()).pushed, 1));
    a.push(&net, key, &opts())
}

#[test]
fn interleaved_edit_has_one_winner_and_the_loser_is_reported() {
    let (a, b, backend, key) = synced_pair();
    let report = a_loses_the_race(&a, &b, &backend, &key);
    assert_eq!((report.pushed, report.failed), (0, 1), "{:?}", report.items);
    assert_eq!(outcomes(&report), [Outcome::Failed(CHANGED.into())]);
    let id = only_id(&backend);
    assert_eq!(remote_bytes(&backend, &key, &id), b.bytes().unwrap());
    assert!(a.text().unwrap().contains("fromA"), "A keeps its edit");
    assert_eq!(a.bak(), None);
}

#[test]
fn loser_pull_is_a_conflict_and_pull_overwrite_takes_the_winner() {
    let (a, b, backend, key) = synced_pair();
    a_loses_the_race(&a, &b, &backend, &key);
    let mine = a.bytes();
    let plain = a.pull(&backend, &key, &opts());
    assert_eq!((plain.pulled, plain.conflicts), (0, 1), "{:?}", plain.items);
    assert_eq!(a.bytes(), mine);
    let taken = a.pull(&backend, &key, &overwrite());
    assert_eq!(taken.pulled, 1, "{:?}", taken.items);
    assert_eq!(a.bytes(), b.bytes());
    assert_eq!(a.bak(), mine, "the lost edit is in the .bak");
    assert_settled(&backend, &key, &[&a, &b], 1);
    assert_eq!(
        remote_bytes(&backend, &key, &only_id(&backend)),
        b.bytes().unwrap()
    );
}

#[test]
fn loser_push_overwrite_puts_the_losers_version_on_the_remote() {
    let (a, b, backend, key) = synced_pair();
    a_loses_the_race(&a, &b, &backend, &key);
    let kept = a.push(&backend, &key, &overwrite());
    assert_eq!((kept.pushed, kept.failed), (1, 0), "{:?}", kept.items);
    let id = only_id(&backend);
    assert_eq!(remote_bytes(&backend, &key, &id), a.bytes().unwrap());
    let theirs = b.bytes();
    let pulled = b.pull(&backend, &key, &opts());
    assert_eq!(pulled.pulled, 1, "{:?}", pulled.items);
    assert_eq!(b.bytes(), a.bytes());
    assert_eq!(b.bak(), theirs, "B's pushed edit is kept in the .bak");
    assert_settled(&backend, &key, &[&a, &b], 1);
}

#[test]
fn dropped_description_is_beaten_by_the_manifest_and_repaired_by_the_next_run() {
    let (a, b, backend, key) = synced_pair();
    let c = mach();
    a.write("v1");
    let net = Net::new(&backend);
    net.drop_describe.set(true);
    assert_eq!(a.push(&net, &key, &opts()).pushed, 1);
    let id = only_id(&backend);
    assert_ne!(
        backend.description(&id),
        backend.manifest(&id),
        "PATCH dropped"
    );
    // C is new and plans against the lagging description, B has no change to see
    let c_net = Net::new(&backend);
    c_net.drop_describe.set(true);
    let report = c.pull(&c_net, &key, &opts());
    assert_eq!(report.pulled, 1, "{:?}", report.items);
    assert_eq!(c.bytes(), a.bytes(), "manifest wins over the description");
    assert_eq!(c.index_entry().unwrap().gen, 2);
    assert_ne!(backend.description(&id), backend.manifest(&id));
    b.pull(&backend, &key, &opts());
    // A's next run repairs the description, then B sees the change
    assert_eq!(a.push(&backend, &key, &opts()).failed, 0);
    assert_eq!(backend.description(&id), backend.manifest(&id));
    let report = b.pull(&backend, &key, &opts());
    assert_eq!(report.pulled, 1, "{:?}", report.items);
    assert_eq!(b.bytes(), a.bytes());
    assert!(b.bak().is_some());
    assert_settled(&backend, &key, &[&a, &b, &c], 1);
    assert_eq!(c.bytes(), a.bytes());
}

#[test]
fn interrupted_create_is_resumed_on_the_same_gist() {
    let (a, b, c) = (mach(), mach(), mach());
    let (backend, key) = (MemoryBackend::new(), Key::generate());
    a.write("fresh");
    let net = Net::new(&backend);
    *net.fail_push.borrow_mut() = Some(BackendError::Unreachable("cut".into()));
    let report = a.push(&net, &key, &opts());
    assert_eq!((report.pushed, report.failed), (0, 1), "{:?}", report.items);
    assert!(a.pending());
    assert_eq!(backend.len(), 1);
    for m in [&b, &c] {
        let seen = m.pull(&backend, &key, &opts());
        assert!(seen.items.is_empty() && m.bytes().is_none(), "half session");
    }
    let pending_id = a.index_entry().unwrap().remote_id;
    let report = a.push(&backend, &key, &opts());
    assert_eq!(report.pushed, 1, "{:?}", report.items);
    assert_eq!(a.index_entry().unwrap().remote_id, pending_id);
    assert_eq!(only_id(&backend), pending_id);
    for m in [&b, &c] {
        assert_eq!(m.pull(&backend, &key, &opts()).pulled, 1);
        assert_eq!(m.bytes(), a.bytes());
    }
    assert_settled(&backend, &key, &[&a, &b, &c], 1);
}

#[test]
fn rolled_back_remote_is_refused_by_every_machine_that_saw_the_newer_gen() {
    let (a, b, backend, key) = synced_pair();
    let c = mach();
    let id = only_id(&backend);
    let (old_manifest, old_blob) = (backend.manifest(&id).unwrap(), backend.blob(&id).unwrap());
    a.write("v1");
    assert_eq!(a.push(&backend, &key, &opts()).pushed, 1);
    for m in [&b, &c] {
        assert_eq!(m.pull(&backend, &key, &opts()).pulled, 1);
    }
    backend.put(Some(&id), &old_manifest, &old_blob);
    for m in [&a, &b, &c] {
        let before = (m.bytes(), m.bak(), m.index_entry());
        for o in [opts(), overwrite()] {
            let report = m.pull(&backend, &key, &o);
            assert_eq!((report.pulled, report.failed), (0, 1), "{:?}", report.items);
            let Outcome::Failed(why) = &report.items[0].outcome else {
                panic!("{:?}", report.items);
            };
            assert!(why.contains("older"), "{why}");
            assert_eq!((m.bytes(), m.bak(), m.index_entry()), before);
        }
    }
    assert_eq!(backend.len(), 1);
}

#[test]
fn two_creates_of_one_new_session_are_duplicates_everywhere_and_nothing_is_deleted() {
    let (a, b, c) = (mach(), mach(), mach());
    let (backend, key) = (MemoryBackend::new(), Key::generate());
    a.write("fromA");
    b.write("fromB");
    let net = Net::new(&backend);
    net.before(|| assert_eq!(b.push(&backend, &key, &opts()).pushed, 1));
    assert_eq!(a.push(&net, &key, &opts()).pushed, 1);
    assert_eq!(backend.len(), 2);
    let (mine_a, mine_b) = (a.bytes(), b.bytes());
    let mut picks = Vec::new();
    for m in [&a, &b, &c] {
        for o in [opts(), overwrite()] {
            for report in [m.pull(&backend, &key, &o), m.push(&backend, &key, &o)] {
                let item = &report.items[0];
                assert_eq!(item.path, PATH);
                assert_eq!(item.decision, Decision::Duplicate, "{item:?}");
                assert_eq!(item.duplicates.len(), 1);
                picks.push(item.remote_id.clone());
            }
        }
    }
    assert!(
        picks.iter().all(|p| *p == picks[0] && p.is_some()),
        "{picks:?}"
    );
    assert_eq!(backend.len(), 2, "D1: nothing is deleted");
    assert_eq!((a.bytes(), b.bytes(), c.bytes()), (mine_a, mine_b, None));
}
