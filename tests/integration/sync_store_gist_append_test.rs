//! Append of the gist store over the in-memory gist remote, nothing here reaches GitHub.

use crate::helpers::gist_rig::{append, Rig, TickClock};
use agr::sync::testing::{Clock, ClockError, PackStore, StoreError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const PARTS: [(&str, &[u8]); 2] = [("data.000", b"abc"), ("data.001", b"def")];

#[test]
fn tree_len_not_expected_is_moved() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &PARTS);
    let res = append(&rig.store(), &id, (10, "i0"), (b"xyz", "i1"));
    assert_eq!(res, Err(StoreError::Moved("i0".into())));
}

#[test]
fn zero_byte_append_on_stale_index_is_moved() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &PARTS);
    let res = rig.store().append(&id, 6, "stale", &[], "i2");
    assert_eq!(res, Err(StoreError::Moved("i0".into())));
    assert!(!rig.remote.calls().iter().any(|c| c.contains("push")));
}

#[test]
fn rejected_push_is_moved_with_the_fetched_index() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &PARTS);
    rig.remote.race_on_push(|files| {
        files.insert("index".into(), b"r1".to_vec());
    });
    let res = append(&rig.store(), &id, (6, "i0"), (b"xyz", "i1"));
    assert_eq!(res, Err(StoreError::Moved("r1".into())));
    assert!(!rig.remote.calls().iter().any(|c| c.contains("--force")));
}

#[test]
fn rejected_push_with_a_failing_fetch_is_not_moved() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &PARTS);
    rig.remote.race_on_push(|files| {
        files.insert("index".into(), b"r1".to_vec());
    });
    rig.remote.fail("fetch");
    let res = append(&rig.store(), &id, (6, "i0"), (b"xyz", "i1"));
    assert!(matches!(res, Err(StoreError::Other(_))), "{res:?}");
}

#[test]
fn clone_with_an_unpushed_commit_is_reset_to_the_remote() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &PARTS);
    let store = rig.store();
    append(&store, &id, (6, "i0"), (b"xyz", "i1")).unwrap();
    // a kill between commit and push left a commit the remote never had
    rig.remote.commit_unpushed(&rig.clone_dir(&id), "ghost");
    let res = append(&store, &id, (9, "i1"), (b"uvw", "i2"));
    assert_eq!(res, Ok(()));
}

#[test]
fn stray_untracked_files_in_the_clone_are_removed() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &PARTS);
    let store = rig.store();
    append(&store, &id, (6, "i0"), (b"xyz", "i1")).unwrap();
    let stray = rig.clone_dir(&id).join("data.009");
    std::fs::write(&stray, b"half").unwrap();
    append(&store, &id, (9, "i1"), (b"uvw", "i2")).unwrap();
    assert!(!stray.exists());
}

#[test]
fn append_is_one_commit_with_parts_and_index() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &PARTS);
    append(&rig.store(), &id, (6, "i0"), (b"xyz", "i1")).unwrap();
    assert_eq!(rig.remote.commit_changes().len(), 1);
    assert_eq!(rig.remote.commit_changes()[0], vec!["data.002", "index"]);
    assert_eq!(rig.remote.push_changes(), vec![vec!["data.002", "index"]]);
}

#[test]
fn pushes_to_one_gist_are_paced_10s() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &[]);
    let store = rig.store();
    append(&store, &id, (0, "i0"), (b"abc", "i1")).unwrap();
    append(&store, &id, (3, "i1"), (b"def", "i2")).unwrap();
    assert_eq!(rig.clock.slept(), vec![Duration::from_secs(10)]);
}

/// Two stores in one process with one clock, so only the shared `pace.toml` carries the push time.
#[test]
fn pacing_is_shared_through_the_pace_file() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &[]);
    append(&rig.store(), &id, (0, "i0"), (b"abc", "i1")).unwrap();
    rig.clock.advance(Duration::from_secs(3));
    append(&rig.store(), &id, (3, "i1"), (b"def", "i2")).unwrap();
    assert_eq!(rig.clock.slept(), vec![Duration::from_secs(7)]);
}

#[test]
fn progress_percent_belongs_to_the_append() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &[]);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let store = rig
        .store()
        .with_progress(Arc::new(move |p| sink.lock().unwrap().push(p)));
    append(&store, &id, (0, "i0"), (b"abc", "i1")).unwrap();
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.last(), Some(&100));
    assert_eq!(seen, vec![50, 100], "the clone reports no percent");
}

/// Fails `unix_secs` from the `n`th call on, the way a clock that breaks after the push does.
struct FailingClock(TickClock, std::sync::atomic::AtomicUsize, usize);

impl Clock for FailingClock {
    fn mono(&self) -> Duration {
        self.0.mono()
    }

    fn unix_secs(&self) -> Result<u64, ClockError> {
        let n = self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if n >= self.2 {
            return Err(ClockError);
        }
        self.0.unix_secs()
    }

    fn sleep(&self, time: Duration) {
        self.0.sleep(time);
    }
}

#[test]
fn pace_write_failure_after_push_is_not_an_append_error() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &[]);
    // Reads of the clock before the push: commit date and the wait. The next one is `note_push`.
    let clock = FailingClock(TickClock::new(), 0.into(), 2);
    let store = rig.store().with_clock(Arc::new(clock));
    let res = append(&store, &id, (0, "i0"), (b"abc", "i1"));
    assert!(res.is_ok(), "{res:?}");
    assert_eq!(rig.remote.push_changes().len(), 1);
}

#[test]
fn sealing_drops_the_clone_at_once() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &[]);
    let store = rig.store();
    append(&store, &id, (0, "i0"), (b"abc", "i1")).unwrap();
    assert!(rig.clone_dir(&id).exists());
    store.sealed(&id);
    assert!(!rig.clone_dir(&id).exists());
}

#[test]
fn big_session_pack_does_not_evict_the_open_clone() {
    let rig = Rig::new();
    let (open, big) = (rig.remote.seed("i0", &[]), rig.remote.seed("j0", &[]));
    let store = rig.store();
    append(&store, &open, (0, "i0"), (b"abc", "i1")).unwrap();
    append(&store, &big, (0, "j0"), (b"xyz", "j1")).unwrap();
    store.sealed(&big);
    assert!(rig.clone_dir(&open).exists());
    assert!(!rig.clone_dir(&big).exists());
}
