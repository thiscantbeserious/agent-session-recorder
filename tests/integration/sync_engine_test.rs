//! Sync engine against the in-memory backend

use agr::files::{codec, lock};
use agr::sync::backend::SyncBackend;
use agr::sync::crypto::{blob_aad, seal, seal_meta, sha256_hex, Key, Meta};
use agr::sync::index::{self, Index};
use agr::sync::memory::MemoryBackend;
use agr::sync::plan::{Decision, SkipReason};
use agr::sync::run::{Engine, Options, Outcome, SyncError};
use agr::{Config, StorageManager};
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct Machine {
    temp: TempDir,
    storage: StorageManager,
}

fn machine() -> Machine {
    let temp = TempDir::new().unwrap();
    let mut config = Config::default();
    config.storage.directory = temp.path().to_string_lossy().to_string();
    Machine {
        storage: StorageManager::new(config),
        temp,
    }
}

impl Machine {
    fn dir(&self) -> &Path {
        self.temp.path()
    }

    fn session(&self, agent: &str, name: &str, text: &str) -> PathBuf {
        let dir = self.dir().join(agent);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        codec::atomic_write(&path, |w| Ok(w.write_all(text.as_bytes())?)).unwrap();
        path
    }

    fn engine<'a>(&'a self, backend: &'a MemoryBackend, key: &'a Key) -> Engine<'a> {
        Engine {
            backend,
            key,
            storage: &self.storage,
        }
    }

    fn names(&self, agent: &str) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.dir().join(agent))
            .map(|rd| {
                rd.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

const CAST: &str = "{\"version\":3,\"width\":80,\"height\":24}\n[0.1,\"o\",\"secret text\"]\n";

fn three(m: &Machine) {
    m.session("claude", "a.cast.gz", CAST);
    m.session("claude", "b.cast.gz", &CAST.replace("secret", "other"));
    m.session("codex", "c.cast.gz", &CAST.replace("secret", "third"));
}

fn opts() -> Options {
    Options::default()
}

fn pushed_machine() -> (Machine, MemoryBackend, Key) {
    let (m, backend, key) = (machine(), MemoryBackend::new(), Key::generate());
    three(&m);
    let report = m.engine(&backend, &key).push(&opts()).unwrap();
    assert_eq!(report.pushed, 3);
    (m, backend, key)
}

#[test]
fn push_uploads_sealed_blobs_and_fills_the_index() {
    let (m, backend, _key) = pushed_machine();
    assert_eq!(backend.len(), 3);
    let (index, warning) = index::load(m.dir());
    assert_eq!(warning, None);
    let ids: Vec<String> = backend.list().unwrap().into_iter().map(|e| e.id).collect();
    for (path, entry) in &index.entries {
        let hash = sha256_hex(&fs::read(m.dir().join(path)).unwrap());
        assert_eq!(entry.sha256, hash);
        assert!(ids.contains(&entry.remote_id));
        let blob = backend.blob(&entry.remote_id).unwrap();
        assert_ne!(&blob[..2], &[0x1f, 0x8b]);
    }
    assert_eq!(index.entries.len(), 3);
}

#[test]
fn second_push_plans_nothing() {
    let (m, backend, key) = pushed_machine();
    let report = m.engine(&backend, &key).push(&opts()).unwrap();
    assert_eq!((report.pushed, report.failed, report.skipped), (0, 0, 0));
    assert_eq!(backend.len(), 3);
}

#[test]
fn changed_local_session_updates_the_same_remote_id() {
    let (m, backend, key) = pushed_machine();
    let before = index::load(m.dir()).0.entries["claude/a.cast.gz"]
        .remote_id
        .clone();
    m.session(
        "claude",
        "a.cast.gz",
        "{\"version\":3}\n[0.2,\"o\",\"edited\"]\n",
    );
    let report = m.engine(&backend, &key).push(&opts()).unwrap();
    assert_eq!(report.pushed, 1);
    assert_eq!(backend.len(), 3);
    let after = index::load(m.dir()).0.entries["claude/a.cast.gz"]
        .remote_id
        .clone();
    assert_eq!(before, after);
}

#[test]
fn pull_on_empty_storage_restores_identical_files() {
    let (a, backend, key) = pushed_machine();
    let b = machine();
    let report = b.engine(&backend, &key).pull(&opts()).unwrap();
    assert_eq!(report.pulled, 3);
    for path in ["claude/a.cast.gz", "claude/b.cast.gz", "codex/c.cast.gz"] {
        assert_eq!(
            fs::read(a.dir().join(path)).unwrap(),
            fs::read(b.dir().join(path)).unwrap()
        );
    }
    assert_eq!(index::load(b.dir()).0.entries.len(), 3);
    let first = codec::open_reader(&b.dir().join("claude/a.cast.gz"))
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .unwrap();
    assert!(first.contains("\"version\":3"));
}

fn token(key: &Key, path: &str, hash: &str) -> String {
    seal_meta(
        key,
        &Meta {
            v: 1,
            path: path.into(),
            sha256: hash.into(),
            size: 1,
        },
    )
}

fn failed_pull(key: &Key, blob_key: &Key, hash: &str, tamper: bool) -> (Machine, usize) {
    let backend = MemoryBackend::new();
    let path = "claude/x.cast.gz";
    let mut blob = seal(blob_key, &blob_aad(path), b"plain");
    if tamper {
        let last = blob.len() - 1;
        blob[last] ^= 1;
    }
    backend.push(None, &token(key, path, hash), &blob).unwrap();
    let b = machine();
    let report = b.engine(&backend, key).pull(&opts()).unwrap();
    (b, report.failed)
}

#[test]
fn tampered_blob_wrong_key_and_hash_mismatch_fail_cleanly() {
    let key = Key::generate();
    let good = sha256_hex(b"plain");
    let cases = [
        failed_pull(&key, &key, &good, true),
        failed_pull(&key, &Key::generate(), &good, false),
        failed_pull(&key, &key, "00", false),
    ];
    for (i, (b, failed)) in cases.iter().enumerate() {
        assert_eq!(*failed, 1, "case {i}");
        assert!(
            b.names("claude").is_empty(),
            "case {i}: {:?}",
            b.names("claude")
        );
    }
}

#[test]
fn pull_overwrite_keeps_hidden_backup_and_plain_pull_keeps_file() {
    let (a, backend, key) = pushed_machine();
    let b = machine();
    let old = b.session(
        "claude",
        "a.cast.gz",
        "{\"version\":3}\n[0.1,\"o\",\"mine\"]\n",
    );
    let old_bytes = fs::read(&old).unwrap();
    let report = b.engine(&backend, &key).pull(&opts()).unwrap();
    assert_eq!(report.conflicts, 1);
    assert_eq!(fs::read(&old).unwrap(), old_bytes);
    let over = Options {
        overwrite: true,
        ..opts()
    };
    let report = b.engine(&backend, &key).pull(&over).unwrap();
    assert_eq!(report.pulled, 1);
    assert_eq!(
        fs::read(&old).unwrap(),
        fs::read(a.dir().join("claude/a.cast.gz")).unwrap()
    );
    assert_eq!(
        fs::read(b.dir().join("claude/.a.cast.gz.bak")).unwrap(),
        old_bytes
    );
}

#[test]
fn existing_backup_with_other_content_blocks_overwrite() {
    let (_a, backend, key) = pushed_machine();
    let b = machine();
    let old = b.session(
        "claude",
        "a.cast.gz",
        "{\"version\":3}\n[0.1,\"o\",\"mine\"]\n",
    );
    fs::write(b.dir().join("claude/.a.cast.gz.bak"), b"precious").unwrap();
    let old_bytes = fs::read(&old).unwrap();
    let over = Options {
        overwrite: true,
        ..opts()
    };
    let report = b.engine(&backend, &key).pull(&over).unwrap();
    assert_eq!(report.failed, 1);
    assert_eq!(fs::read(&old).unwrap(), old_bytes);
    assert_eq!(
        fs::read(b.dir().join("claude/.a.cast.gz.bak")).unwrap(),
        b"precious"
    );
}

#[test]
fn failing_write_leaves_nothing_behind() {
    let (_a, backend, key) = pushed_machine();
    let b = machine();
    fs::write(b.dir().join("codex"), b"i am a file").unwrap();
    let report = b.engine(&backend, &key).pull(&opts()).unwrap();
    assert_eq!((report.pulled, report.failed), (2, 1));
    assert_eq!(fs::read(b.dir().join("codex")).unwrap(), b"i am a file");
}

#[test]
fn one_failing_session_does_not_stop_the_others() {
    let (a, backend, key) = pushed_machine();
    let id = index::load(a.dir()).0.entries["claude/b.cast.gz"]
        .remote_id
        .clone();
    backend.fail_on(&id);
    let b = machine();
    let report = b.engine(&backend, &key).pull(&opts()).unwrap();
    assert_eq!((report.pulled, report.failed), (2, 1));
    let expected: u64 = ["claude/a.cast.gz", "codex/c.cast.gz"]
        .iter()
        .map(|p| fs::read(a.dir().join(p)).unwrap().len() as u64)
        .sum();
    assert_eq!(report.bytes, expected);
}

#[test]
fn dry_run_returns_the_plan_and_changes_nothing() {
    let (m, backend, key) = (machine(), MemoryBackend::new(), Key::generate());
    three(&m);
    let dry = Options {
        dry_run: true,
        ..opts()
    };
    let planned = m.engine(&backend, &key).push(&dry).unwrap();
    assert!(backend.is_empty());
    assert!(!index::path_for(m.dir()).exists());
    assert_eq!(planned.items.len(), 3);
    assert!(planned.items.iter().all(|i| i.outcome == Outcome::Planned));
    let real = m.engine(&backend, &key).push(&opts()).unwrap();
    let decisions = |r: &agr::sync::run::Report| -> Vec<_> {
        r.items
            .iter()
            .map(|i| (i.path.clone(), i.decision))
            .collect()
    };
    assert_eq!(decisions(&planned), decisions(&real));
    let b = machine();
    let pulled = b.engine(&backend, &key).pull(&dry).unwrap();
    assert_eq!(pulled.items.len(), 3);
    assert!(b.names("claude").is_empty() && b.names("codex").is_empty());
}

#[test]
fn delete_removes_remote_and_index_entry_but_keeps_the_file() {
    let (m, backend, key) = pushed_machine();
    m.engine(&backend, &key)
        .delete(&["claude/a.cast.gz".to_string()])
        .unwrap();
    assert_eq!(backend.len(), 2);
    assert!(!index::load(m.dir())
        .0
        .entries
        .contains_key("claude/a.cast.gz"));
    assert!(m.dir().join("claude/a.cast.gz").exists());
}

#[test]
fn deleted_index_rebuilds_without_uploading() {
    let (m, backend, key) = pushed_machine();
    let strip = |i: Index| -> Vec<_> {
        i.entries
            .into_iter()
            .map(|(p, e)| (p, e.sha256, e.remote_id))
            .collect()
    };
    let before = strip(index::load(m.dir()).0);
    fs::remove_file(index::path_for(m.dir())).unwrap();
    let report = m.engine(&backend, &key).push(&opts()).unwrap();
    assert_eq!((report.pushed, report.conflicts), (0, 0));
    assert_eq!(backend.len(), 3);
    assert_eq!(strip(index::load(m.dir()).0), before);
}

#[cfg(unix)]
#[test]
fn held_lock_is_busy() {
    let (m, backend, key) = (machine(), MemoryBackend::new(), Key::generate());
    three(&m);
    let held = format!("{{\"pid\":{},\"started\":\"x\"}}", std::process::id());
    fs::write(m.dir().join(index::LOCK_FILE), held).unwrap();
    let result = m.engine(&backend, &key).push(&opts());
    assert_eq!(result.err(), Some(SyncError::Busy));
    assert!(backend.is_empty());
}

#[test]
fn stale_lock_is_reclaimed_and_removed_afterwards() {
    let (m, backend, key) = (machine(), MemoryBackend::new(), Key::generate());
    three(&m);
    let lock_path = m.dir().join(index::LOCK_FILE);
    fs::write(&lock_path, "{\"pid\":2000000000,\"started\":\"x\"}").unwrap();
    assert_eq!(m.engine(&backend, &key).push(&opts()).unwrap().pushed, 3);
    assert!(!lock_path.exists());
}

#[test]
fn session_filter_uses_the_regular_name_lookup() {
    let (m, backend, key) = (machine(), MemoryBackend::new(), Key::generate());
    three(&m);
    let one = Options {
        sessions: vec!["claude/a.cast".into()],
        ..opts()
    };
    assert_eq!(m.engine(&backend, &key).push(&one).unwrap().pushed, 1);
    let bad = Options {
        sessions: vec!["claude/nope.cast".into()],
        ..opts()
    };
    assert_eq!(m.engine(&backend, &key).push(&bad).unwrap().failed, 1);
    assert_eq!(backend.len(), 1);
}

#[test]
fn live_locked_plain_and_oversized_sessions_are_skipped() {
    let (m, backend, key) = (machine(), MemoryBackend::new(), Key::generate());
    let locked = m.session("claude", "live.cast.gz", CAST);
    lock::create_lock(&locked).unwrap();
    fs::write(m.dir().join("claude/plain.cast"), CAST).unwrap();
    let big = m.session("claude", "big.cast.gz", CAST);
    fs::OpenOptions::new()
        .write(true)
        .open(&big)
        .unwrap()
        .set_len(101 * 1024 * 1024)
        .unwrap();
    let report = m.engine(&backend, &key).push(&opts()).unwrap();
    assert_eq!((report.pushed, report.skipped), (0, 3));
    assert!(backend.is_empty());
    let reasons: Vec<_> = report.items.iter().map(|i| i.decision).collect();
    for r in [SkipReason::Locked, SkipReason::Plain, SkipReason::OverCap] {
        assert!(reasons.contains(&Decision::Skip(r)), "{r:?}");
    }
}
