//! Sync engine against the in-memory backend

use agr::files::{codec, lock};
use agr::sync::backend::{BackendError, BlobSource, RemoteEntry, SyncBackend};
use agr::sync::crypto::{seal, seal_meta, sha256_hex, Key, Meta};
use agr::sync::index::{self, Index};
use agr::sync::memory::MemoryBackend;
use agr::sync::plan::{Decision, SkipReason};
use agr::sync::run::{Engine, Options, Outcome, SyncError};
use agr::{Config, StorageManager};

use crate::helpers::sync_net::{open_blob, seal_blob};
use std::cell::{Cell, RefCell};
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::rc::Rc;
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

    fn engine<'a>(&'a self, backend: &'a dyn SyncBackend, key: &'a Key) -> Engine<'a> {
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
            v: 2,
            path: path.into(),
            sha256: hash.into(),
            size: 1,
            gen: 1,
        },
    )
}

fn failed_pull(key: &Key, blob_key: &Key, hash: &str, tamper: bool) -> (Machine, usize) {
    let backend = MemoryBackend::new();
    let path = "claude/x.cast.gz";
    let mut blob = seal_blob(blob_key, path, 1, b"plain");
    if tamper {
        let last = blob.len() - 1;
        blob[last] ^= 1;
    }
    backend.put(None, &token(key, path, hash), &blob);
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
fn existing_backup_with_other_content_is_rotated_not_blocking() {
    let (a, backend, key) = pushed_machine();
    let b = machine();
    let old = b.session(
        "claude",
        "a.cast.gz",
        "{\"version\":3}\n[0.1,\"o\",\"mine\"]\n",
    );
    fs::write(b.dir().join("claude/.a.cast.gz.bak"), b"stale").unwrap();
    let old_bytes = fs::read(&old).unwrap();
    let over = Options {
        overwrite: true,
        ..opts()
    };
    let report = b.engine(&backend, &key).pull(&over).unwrap();
    assert_eq!((report.pulled, report.failed), (3, 0));
    assert_eq!(
        fs::read(&old).unwrap(),
        fs::read(a.dir().join("claude/a.cast.gz")).unwrap()
    );
    assert_eq!(
        fs::read(b.dir().join("claude/.a.cast.gz.bak")).unwrap(),
        old_bytes
    );
    let again = b.engine(&backend, &key).pull(&over).unwrap();
    assert_eq!(again.failed, 0);
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
    assert_eq!(result.err(), Some(agr::sync::run::SyncError::Busy));
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
        .set_len(1024 * 1024 * 1024 + 1)
        .unwrap();
    let report = m.engine(&backend, &key).push(&opts()).unwrap();
    assert_eq!((report.pushed, report.skipped), (0, 3));
    assert!(backend.is_empty());
    let reasons: Vec<_> = report.items.iter().map(|i| i.decision).collect();
    for r in [SkipReason::Locked, SkipReason::Plain, SkipReason::OverCap] {
        assert!(reasons.contains(&Decision::Skip(r)), "{r:?}");
    }
}

#[test]
fn session_between_100mb_and_the_cap_is_pushed() {
    let (m, backend, key) = (machine(), MemoryBackend::new(), Key::generate());
    let big = m.session("claude", "big.cast.gz", CAST);
    fs::OpenOptions::new()
        .write(true)
        .open(&big)
        .unwrap()
        .set_len(101 * 1024 * 1024)
        .unwrap();
    let report = m.engine(&backend, &key).push(&opts()).unwrap();
    assert_eq!((report.pushed, report.skipped), (1, 0));
    assert_eq!(backend.len(), 1);
}

fn overwrite() -> Options {
    Options {
        overwrite: true,
        ..opts()
    }
}

fn remote_ids(backend: &MemoryBackend) -> Vec<String> {
    backend.list().unwrap().into_iter().map(|e| e.id).collect()
}

#[test]
fn push_overwrite_lets_local_win_and_reuses_the_remote_id() {
    let (a, backend, key) = pushed_machine();
    let b = machine();
    b.engine(&backend, &key).pull(&opts()).unwrap();
    a.session("claude", "a.cast.gz", &CAST.replace("secret", "from a"));
    b.session("claude", "a.cast.gz", &CAST.replace("secret", "from b"));
    a.engine(&backend, &key).push(&opts()).unwrap();
    let ids = remote_ids(&backend);
    let plain = b.engine(&backend, &key).push(&opts()).unwrap();
    assert_eq!((plain.conflicts, plain.pushed), (1, 0));
    let report = b.engine(&backend, &key).push(&overwrite()).unwrap();
    assert_eq!((report.pushed, report.conflicts), (1, 0));
    assert_eq!(remote_ids(&backend), ids);
    let c = machine();
    c.engine(&backend, &key).pull(&opts()).unwrap();
    assert_eq!(
        fs::read(c.dir().join("claude/a.cast.gz")).unwrap(),
        fs::read(b.dir().join("claude/a.cast.gz")).unwrap()
    );
}

#[test]
fn cleanup_of_the_local_file_shows_remote_only_and_touches_nothing() {
    let (m, backend, key) = pushed_machine();
    fs::remove_file(m.dir().join("claude/a.cast.gz")).unwrap();
    let ids = remote_ids(&backend);
    let blob = backend.blob(&ids[0]);
    let report = m.engine(&backend, &key).status(&[]).unwrap();
    let item = report
        .items
        .iter()
        .find(|i| i.path == "claude/a.cast.gz")
        .unwrap();
    assert_eq!(agr::sync::format::label(item), "remote only (pull)");
    let pushed = m.engine(&backend, &key).push(&opts()).unwrap();
    assert_eq!((pushed.pushed, pushed.deleted, pushed.failed), (0, 0, 0));
    assert_eq!(remote_ids(&backend), ids);
    assert_eq!(backend.blob(&ids[0]), blob);
    assert!(!m.dir().join("claude/a.cast.gz").exists());
}

#[test]
fn vanished_gist_shows_local_only_and_deletes_no_local_file() {
    let (m, backend, key) = pushed_machine();
    let gone = index::load(m.dir()).0.entries["claude/a.cast.gz"]
        .remote_id
        .clone();
    backend.delete(&gone).unwrap();
    let report = m.engine(&backend, &key).status(&[]).unwrap();
    let item = report
        .items
        .iter()
        .find(|i| i.path == "claude/a.cast.gz")
        .unwrap();
    assert_eq!(agr::sync::format::label(item), "local only (push)");
    let pulled = m.engine(&backend, &key).pull(&overwrite()).unwrap();
    assert_eq!(pulled.failed, 0);
    for path in ["claude/a.cast.gz", "claude/b.cast.gz", "codex/c.cast.gz"] {
        assert!(m.dir().join(path).exists(), "{path}");
    }
}

#[test]
fn unknown_version_and_other_key_entries_are_reported_and_left_alone() {
    let (m, backend, key) = pushed_machine();
    let other = Key::generate();
    let v2 = seal_meta(
        &key,
        &Meta {
            v: 3,
            path: "claude/z.cast.gz".into(),
            sha256: "00".into(),
            size: 1,
            gen: 1,
        },
    );
    let foreign = token(&other, "claude/y.cast.gz", "00");
    let v2_id = backend.put(None, &v2, b"v2 blob");
    let foreign_id = backend.put(None, &foreign, b"foreign blob");
    let status = m.engine(&backend, &key).status(&[]).unwrap();
    let decisions: Vec<_> = status.items.iter().map(|i| i.decision).collect();
    assert!(decisions.contains(&Decision::UnknownVersion));
    assert!(decisions.contains(&Decision::OtherKey));
    let push = m.engine(&backend, &key).push(&overwrite()).unwrap();
    let pull = m.engine(&backend, &key).pull(&overwrite()).unwrap();
    assert_eq!((push.failed, pull.failed, pull.pulled), (0, 0, 0));
    assert_eq!(push.skipped, 2);
    assert_eq!(backend.blob(&v2_id).as_deref(), Some(&b"v2 blob"[..]));
    assert_eq!(
        backend.blob(&foreign_id).as_deref(),
        Some(&b"foreign blob"[..])
    );
    assert_eq!(backend.len(), 5);
}

/// Seal a raw JSON manifest the way a v1 build did, with the meta AAD and no `gen` field.
fn raw_token(key: &Key, json: &str) -> String {
    data_encoding::BASE64URL_NOPAD.encode(&seal(key, b"agr:meta", json.as_bytes()))
}

#[test]
fn gen_zero_manifest_is_unknown_version_and_never_touched() {
    let (m, backend, key) = pushed_machine();
    let zero = seal_meta(
        &key,
        &Meta {
            v: 2,
            path: "claude/z.cast.gz".into(),
            sha256: "00".into(),
            size: 1,
            gen: 0,
        },
    );
    let id = backend.put(None, &zero, b"zero blob");
    let status = m.engine(&backend, &key).status(&[]).unwrap();
    let decisions: Vec<_> = status.items.iter().map(|i| i.decision).collect();
    assert!(
        decisions.contains(&Decision::UnknownVersion),
        "{decisions:?}"
    );
    assert_eq!(backend.blob(&id).as_deref(), Some(&b"zero blob"[..]));
}

#[test]
fn v1_token_is_unknown_version_and_never_touched() {
    let (m, backend, key) = pushed_machine();
    let json = r#"{"v":1,"path":"claude/x.cast.gz","sha256":"00","size":1}"#;
    let v1 = raw_token(&key, json);
    let id = backend.put(None, &v1, b"v1 blob");
    let status = m.engine(&backend, &key).status(&[]).unwrap();
    let decisions: Vec<_> = status.items.iter().map(|i| i.decision).collect();
    assert!(decisions.contains(&Decision::UnknownVersion));
    let pull = m.engine(&backend, &key).pull(&overwrite()).unwrap();
    assert_eq!((pull.failed, pull.pulled), (0, 0));
    assert!(!m.dir().join("claude/z.cast.gz").exists());
    assert_eq!(backend.blob(&id).as_deref(), Some(&b"v1 blob"[..]));
}

#[test]
fn first_push_is_v2_gen_1_and_the_next_push_is_gen_2_with_a_gen_2_blob() {
    let (m, backend, key) = pushed_machine();
    let first = backend.list().unwrap().remove(0);
    let meta = agr::sync::crypto::open_meta(&key, &first.meta_token).unwrap();
    assert_eq!((meta.v, meta.gen), (2, 1));
    let blob = backend.blob(&first.id).unwrap();
    assert!(open_blob(&key, &meta.path, 1, &blob).is_ok());
    m.session("claude", "a.cast.gz", &CAST.replace("secret", "edited"));
    m.engine(&backend, &key).push(&opts()).unwrap();
    let id = only_entry(&m, "claude/a.cast.gz").remote_id;
    let token = backend.manifest(&id).unwrap();
    let meta = agr::sync::crypto::open_meta(&key, &token).unwrap();
    assert_eq!(meta.gen, 2);
    let blob = backend.blob(&id).unwrap();
    assert!(open_blob(&key, &meta.path, 2, &blob).is_ok());
    assert!(open_blob(&key, &meta.path, 1, &blob).is_err());
}

#[test]
fn push_stores_the_manifest_and_then_describes_with_the_same_token() {
    let (_m, backend, _key) = pushed_machine();
    let listed = backend.list().unwrap();
    assert_eq!(listed.len(), 3);
    for entry in listed {
        assert_eq!(backend.manifest(&entry.id), Some(entry.meta_token));
    }
}

fn only_entry(m: &Machine, path: &str) -> agr::sync::index::IndexEntry {
    index::load(m.dir()).0.entries[path].clone()
}

#[test]
fn push_records_gen_and_token_in_the_index() {
    let (m, backend, _key) = pushed_machine();
    let entry = only_entry(&m, "claude/a.cast.gz");
    assert_eq!(entry.gen, 1);
    assert_eq!(entry.token, backend.manifest(&entry.remote_id));
    assert!(entry.token.is_some());
}

#[test]
fn unlisted_gist_is_pushed_with_the_index_token_as_expected() {
    let (m, backend, key) = pushed_machine();
    let id = only_entry(&m, "claude/a.cast.gz").remote_id;
    backend.set_description(&id, None);
    m.session("claude", "a.cast.gz", &CAST.replace("secret", "edited"));
    let report = m.engine(&backend, &key).push(&opts()).unwrap();
    assert_eq!((report.pushed, report.failed), (1, 0), "{:?}", report.items);
    assert_eq!(only_entry(&m, "claude/a.cast.gz").remote_id, id);
    assert_eq!(backend.len(), 3);
}

#[test]
fn rebuilt_index_keeps_the_listed_gen_and_token() {
    let (m, backend, key) = pushed_machine();
    fs::remove_file(index::path_for(m.dir())).unwrap();
    m.engine(&backend, &key).push(&opts()).unwrap();
    let entry = only_entry(&m, "claude/a.cast.gz");
    assert_eq!(entry.gen, 1);
    assert_eq!(entry.token, backend.manifest(&entry.remote_id));
}

#[test]
fn pull_records_the_manifest_gen_and_token_and_repair_is_marked() {
    let (key, backend) = (Key::generate(), MemoryBackend::new());
    let path = "claude/x.cast.gz";
    let plain = b"plain";
    let seal_gen = |gen| {
        let meta = Meta {
            v: 2,
            path: path.into(),
            sha256: sha256_hex(plain),
            size: 5,
            gen,
        };
        seal_meta(&key, &meta)
    };
    let (t2, t1) = (seal_gen(2), seal_gen(1));
    let id = backend.put(None, &t2, &seal_blob(&key, path, 2, plain));
    let b = machine();
    let report = b.engine(&backend, &key).pull(&opts()).unwrap();
    assert_eq!(report.pulled, 1, "{:?}", report.items);
    let entry = only_entry(&b, path);
    assert_eq!((entry.gen, entry.token.as_deref()), (2, Some(t2.as_str())));
    backend.set_description(&id, Some(t1));
    let index = index::load(b.dir()).0;
    let rows = agr::sync::scan::scan(&b.storage, &key, &backend.list().unwrap(), &index).unwrap();
    assert!(
        rows.iter()
            .find(|r| r.path == path)
            .unwrap()
            .repair_description
    );
}

const GONE: &str = "remote copy gone, `agr sync push --overwrite` uploads it again";

fn gone_machine() -> (Machine, MemoryBackend, Key, String) {
    let (m, backend, key) = pushed_machine();
    let id = only_entry(&m, "claude/a.cast.gz").remote_id;
    backend.delete(&id).unwrap();
    (m, backend, key, id)
}

fn item_of<'a>(report: &'a agr::sync::run::Report, path: &str) -> &'a agr::sync::run::Item {
    report.items.iter().find(|i| i.path == path).unwrap()
}

#[test]
fn push_of_a_deleted_gist_is_skipped_as_gone_and_creates_nothing() {
    let (m, backend, key, id) = gone_machine();
    let report = m.engine(&backend, &key).push(&opts()).unwrap();
    let item = item_of(&report, "claude/a.cast.gz");
    assert_eq!(item.outcome, Outcome::Skipped(GONE.into()), "{item:?}");
    assert_eq!((report.pushed, report.failed, backend.len()), (0, 0, 2));
    assert!(m.dir().join("claude/a.cast.gz").exists());
    assert_eq!(only_entry(&m, "claude/a.cast.gz").remote_id, id);
}

#[test]
fn push_overwrite_recreates_a_deleted_gist_once() {
    let (m, backend, key, id) = gone_machine();
    let report = m.engine(&backend, &key).push(&overwrite()).unwrap();
    assert_eq!((report.pushed, report.failed, backend.len()), (1, 0, 3));
    let entry = only_entry(&m, "claude/a.cast.gz");
    assert_ne!(entry.remote_id, id);
    assert_eq!((entry.gen, entry.pending), (2, false));
    let again = m.engine(&backend, &key).push(&opts()).unwrap();
    assert_eq!((again.pushed, backend.len()), (0, 3));
}

/// Lists like the inner backend but the gist is gone when pulled.
struct GoneOnPull(MemoryBackend);

impl SyncBackend for GoneOnPull {
    fn name(&self) -> &'static str {
        "gone"
    }
    fn check(&self) -> Result<(), agr::sync::backend::BackendError> {
        self.0.check()
    }
    fn list(
        &self,
    ) -> Result<Vec<agr::sync::backend::RemoteEntry>, agr::sync::backend::BackendError> {
        self.0.list()
    }
    fn create(&self) -> Result<String, agr::sync::backend::BackendError> {
        self.0.create()
    }
    fn push(
        &self,
        id: &str,
        e: Option<&str>,
        m: &str,
        b: BlobSource,
    ) -> Result<(), agr::sync::backend::BackendError> {
        self.0.push(id, e, m, b)
    }
    fn describe(&self, id: &str, t: &str) -> Result<(), agr::sync::backend::BackendError> {
        self.0.describe(id, t)
    }
    fn pull(
        &self,
        _id: &str,
    ) -> Result<(Option<String>, Box<dyn std::io::Read>), agr::sync::backend::BackendError> {
        Err(agr::sync::backend::BackendError::NotFound)
    }
    fn delete(&self, id: &str) -> Result<(), agr::sync::backend::BackendError> {
        self.0.delete(id)
    }
}

#[test]
fn pull_of_a_deleted_gist_is_skipped_as_gone_and_keeps_the_index_entry() {
    let (a, backend, key) = pushed_machine();
    fs::remove_file(a.dir().join("claude/a.cast.gz")).unwrap();
    let wrapped = GoneOnPull(backend);
    let report = a.engine(&wrapped, &key).pull(&overwrite()).unwrap();
    let item = item_of(&report, "claude/a.cast.gz");
    assert_eq!(item.outcome, Outcome::Skipped(GONE.into()), "{item:?}");
    assert_eq!((report.pulled, report.failed), (0, 0));
    assert!(!a.dir().join("claude/a.cast.gz").exists());
    assert!(only_entry(&a, "claude/a.cast.gz").token.is_some());
}

#[test]
fn duplicate_status_line_says_duplicate_with_its_reason() {
    let (m, backend, key) = (machine(), MemoryBackend::new(), Key::generate());
    let path = "claude/a.cast.gz";
    m.session("claude", "a.cast.gz", CAST);
    for hash in ["one", "two"] {
        let tok = token(&key, path, hash);
        backend.put(None, &tok, &seal_blob(&key, path, 1, b"x"));
    }
    let report = m.engine(&backend, &key).status(&[]).unwrap();
    let lines = agr::sync::format::status_lines(&report, "memory");
    let line = lines.iter().find(|l| l.contains(path)).unwrap();
    assert!(
        line.starts_with("  claude/a.cast.gz duplicate (")
            && line.ends_with("): duplicate remote copies"),
        "{line}"
    );
    assert!(lines.last().unwrap().contains("1 duplicate"), "{lines:?}");
}

const A: &str = "claude/a.cast.gz";

type Hook = Box<dyn FnOnce(&MemoryBackend)>;

/// Memory backend that records calls and injects failures at chosen pushes.
#[derive(Default)]
struct Spy {
    inner: MemoryBackend,
    creates: Cell<usize>,
    pushes: Cell<usize>,
    expected: RefCell<Vec<Option<String>>>,
    /// Runs inside the next push before the compare, models another machine winning the race.
    before_push: RefCell<Option<Hook>>,
    /// The nth push (1 based) fails with this error.
    push_error: RefCell<Option<(usize, BackendError)>>,
    describe_error: RefCell<Option<BackendError>>,
    /// Runs once before the next describe or pull, models another machine finishing a push
    /// between our failed compare and our description repair.
    between: RefCell<Option<Hook>>,
    /// The nth delete (1 based) fails with this error.
    delete_error: RefCell<Option<(usize, BackendError)>>,
    deletes: Cell<usize>,
    pulls: Cell<usize>,
    /// The nth pull (1 based) fails with this error.
    pull_error: RefCell<Option<(usize, BackendError)>>,
}

impl Spy {
    fn interleave(&self) {
        if let Some(hook) = self.between.borrow_mut().take() {
            hook(&self.inner);
        }
    }
}

impl SyncBackend for Spy {
    fn name(&self) -> &'static str {
        "spy"
    }
    fn check(&self) -> Result<(), BackendError> {
        self.inner.check()
    }
    fn list(&self) -> Result<Vec<RemoteEntry>, BackendError> {
        self.inner.list()
    }
    fn create(&self) -> Result<String, BackendError> {
        self.creates.set(self.creates.get() + 1);
        self.inner.create()
    }
    fn push(&self, id: &str, e: Option<&str>, m: &str, b: BlobSource) -> Result<(), BackendError> {
        self.pushes.set(self.pushes.get() + 1);
        self.expected.borrow_mut().push(e.map(String::from));
        if let Some(hook) = self.before_push.borrow_mut().take() {
            hook(&self.inner);
        }
        let mut failing = self.push_error.borrow_mut();
        if failing
            .as_ref()
            .is_some_and(|(n, _)| *n == self.pushes.get())
        {
            return Err(failing.take().unwrap().1);
        }
        self.inner.push(id, e, m, b)
    }
    fn describe(&self, id: &str, t: &str) -> Result<(), BackendError> {
        self.interleave();
        match self.describe_error.borrow_mut().take() {
            Some(e) => Err(e),
            None => self.inner.describe(id, t),
        }
    }
    fn pull(&self, id: &str) -> Result<(Option<String>, Box<dyn std::io::Read>), BackendError> {
        self.interleave();
        self.pulls.set(self.pulls.get() + 1);
        let mut failing = self.pull_error.borrow_mut();
        if failing
            .as_ref()
            .is_some_and(|(n, _)| *n == self.pulls.get())
        {
            return Err(failing.take().unwrap().1);
        }
        self.inner.pull(id)
    }
    fn delete(&self, id: &str) -> Result<(), BackendError> {
        self.deletes.set(self.deletes.get() + 1);
        let mut failing = self.delete_error.borrow_mut();
        if failing
            .as_ref()
            .is_some_and(|(n, _)| *n == self.deletes.get())
        {
            return Err(failing.take().unwrap().1);
        }
        self.inner.delete(id)
    }
}

fn token_gen(key: &Key, hash: &str, gen: u64) -> String {
    let meta = Meta {
        v: 2,
        path: A.into(),
        sha256: hash.into(),
        size: 1,
        gen,
    };
    seal_meta(key, &meta)
}

fn spied() -> (Machine, Spy, Key) {
    let (m, backend, key) = pushed_machine();
    (
        m,
        Spy {
            inner: backend,
            ..Spy::default()
        },
        key,
    )
}

fn edit_a(m: &Machine) -> String {
    let path = m.session("claude", "a.cast.gz", &CAST.replace("secret", "edited"));
    sha256_hex(&fs::read(path).unwrap())
}

fn race(spy: &Spy, hook: impl FnOnce(&MemoryBackend) + 'static) {
    *spy.before_push.borrow_mut() = Some(Box::new(hook));
}

fn fresh_spy() -> (Machine, Spy, Key) {
    let m = machine();
    m.session("claude", "a.cast.gz", CAST);
    (m, Spy::default(), Key::generate())
}

const CHANGED: &str = "changed on the remote, run again";

#[test]
fn push_seals_gen_plus_one_with_the_planned_token_as_expected() {
    let (m, spy, key) = spied();
    let planned = only_entry(&m, A).token;
    edit_a(&m);
    let report = m.engine(&spy, &key).push(&opts()).unwrap();
    assert_eq!((report.pushed, report.failed), (1, 0), "{:?}", report.items);
    assert_eq!(spy.expected.borrow().as_slice(), [planned]);
    let entry = only_entry(&m, A);
    assert_eq!(entry.gen, 2);
    assert_eq!(spy.inner.description(&entry.remote_id), entry.token);
}

#[test]
fn changed_is_reported_as_changed_remotely_and_nothing_is_recorded() {
    let (m, spy, key) = spied();
    let before = only_entry(&m, A);
    let theirs = token_gen(&key, "other-hash", 2);
    let (id, t) = (before.remote_id.clone(), theirs.clone());
    race(&spy, move |b| {
        b.put(Some(&id), &t, b"theirs");
    });
    edit_a(&m);
    let report = m.engine(&spy, &key).push(&opts()).unwrap();
    let item = item_of(&report, A);
    assert_eq!(item.outcome, Outcome::Failed(CHANGED.into()), "{item:?}");
    assert_eq!((report.pushed, report.conflicts), (0, 0));
    assert_eq!(only_entry(&m, A), before);
    assert_eq!(spy.inner.manifest(&before.remote_id), Some(theirs));
}

#[test]
fn own_earlier_push_with_a_lost_index_save_is_adopted() {
    let (m, spy, key) = spied();
    let id = only_entry(&m, A).remote_id;
    let hash = edit_a(&m);
    let own = token_gen(&key, &hash, 2);
    let (id2, t) = (id.clone(), own.clone());
    race(&spy, move |b| {
        b.set_manifest(&id2, Some(t));
        b.set_blob(&id2, b"our earlier blob".to_vec());
    });
    let report = m.engine(&spy, &key).push(&opts()).unwrap();
    assert_eq!(
        (report.failed, report.conflicts),
        (0, 0),
        "{:?}",
        report.items
    );
    let entry = only_entry(&m, A);
    assert_eq!((entry.gen, entry.token.as_deref()), (2, Some(own.as_str())));
    assert_eq!(spy.inner.description(&id), Some(own));
    assert_eq!(
        spy.inner.blob(&id).as_deref(),
        Some(&b"our earlier blob"[..])
    );
}

/// Another machine wins the race with a manifest of ours at `gen`, description still lagging.
fn lagging_theirs(m: &Machine, spy: &Spy, key: &Key, gen: u64) -> (String, String) {
    let id = only_entry(m, A).remote_id;
    let theirs = token_gen(key, "other-hash", gen);
    let (id2, t) = (id.clone(), theirs.clone());
    race(spy, move |b| b.set_manifest(&id2, Some(t)));
    (id, theirs)
}

#[test]
fn the_description_repair_never_overwrites_a_newer_describe() {
    let (m, spy, key) = spied();
    let (id, _) = lagging_theirs(&m, &spy, &key, 2);
    let newer = token_gen(&key, "newest-hash", 3);
    let (id2, n) = (id.clone(), newer.clone());
    *spy.between.borrow_mut() = Some(Box::new(move |b| {
        b.put(Some(&id2), &n, b"newest");
    }));
    edit_a(&m);
    let report = m.engine(&spy, &key).push(&opts()).unwrap();
    assert_eq!(item_of(&report, A).outcome, Outcome::Failed(CHANGED.into()));
    assert_eq!(spy.inner.manifest(&id), Some(newer.clone()));
    assert_eq!(spy.inner.description(&id), Some(newer));
}

#[test]
fn same_gen_other_content_is_described_as_theirs_and_reported_changed() {
    let (m, spy, key) = spied();
    let (id, theirs) = lagging_theirs(&m, &spy, &key, 1);
    edit_a(&m);
    let report = m.engine(&spy, &key).push(&opts()).unwrap();
    assert_eq!(item_of(&report, A).outcome, Outcome::Failed(CHANGED.into()));
    assert_eq!(spy.inner.description(&id), Some(theirs));
}

#[test]
fn new_session_stores_the_pending_id_first_and_a_failed_push_is_resumed() {
    let (m, spy, key) = fresh_spy();
    let dir = m.dir().to_path_buf();
    race(&spy, move |_| {
        let entry = index::load(&dir).0.entries[A].clone();
        assert!(entry.pending && entry.token.is_none(), "{entry:?}");
    });
    *spy.push_error.borrow_mut() = Some((1, BackendError::Unreachable("down".into())));
    let report = m.engine(&spy, &key).push(&opts()).unwrap();
    assert_eq!((report.pushed, report.failed), (0, 1));
    let pending = only_entry(&m, A);
    assert!(pending.pending, "{pending:?}");
    let report = m.engine(&spy, &key).push(&opts()).unwrap();
    assert_eq!((report.pushed, report.failed), (1, 0), "{:?}", report.items);
    assert_eq!((spy.creates.get(), spy.inner.len()), (1, 1));
    let done = only_entry(&m, A);
    assert_eq!(
        (done.remote_id, done.gen, done.pending),
        (pending.remote_id, 1, false)
    );
}

#[test]
fn describe_failing_after_a_new_push_still_records_it_and_the_next_run_repairs() {
    let (m, spy, key) = fresh_spy();
    *spy.describe_error.borrow_mut() = Some(BackendError::Unreachable("down".into()));
    let report = m.engine(&spy, &key).push(&opts()).unwrap();
    let Outcome::Failed(why) = &item_of(&report, A).outcome else {
        panic!("{:?}", report.items)
    };
    assert!(why.contains("next run"), "{why}");
    let entry = only_entry(&m, A);
    assert_eq!((entry.gen, entry.pending), (1, false));
    assert!(entry.token.is_some() && spy.inner.description(&entry.remote_id).is_none());
    let report = m.engine(&spy, &key).push(&opts()).unwrap();
    assert_eq!(report.failed, 0, "{:?}", report.items);
    assert_eq!((spy.creates.get(), spy.pushes.get()), (1, 1));
    assert_eq!(spy.inner.description(&entry.remote_id), entry.token);
}

#[test]
fn describe_failing_after_an_update_is_repaired_to_the_index_token() {
    let (m, spy, key) = spied();
    let old = only_entry(&m, A).token;
    edit_a(&m);
    *spy.describe_error.borrow_mut() = Some(BackendError::Unreachable("down".into()));
    m.engine(&spy, &key).push(&opts()).unwrap();
    let entry = only_entry(&m, A);
    assert_eq!(entry.gen, 2);
    assert_eq!(spy.inner.description(&entry.remote_id), old);
    let report = m.engine(&spy, &key).push(&opts()).unwrap();
    assert_eq!(report.failed, 0, "{:?}", report.items);
    assert_eq!(spy.inner.description(&entry.remote_id), entry.token);
    assert_eq!(spy.pushes.get(), 1);
}

#[test]
fn description_is_never_repaired_to_a_lower_gen() {
    let (m, spy, key) = spied();
    let id = only_entry(&m, A).remote_id;
    let ahead = token_gen(&key, "remote-hash", 2);
    spy.inner.put(Some(&id), &ahead, b"theirs");
    let report = m.engine(&spy, &key).push(&opts()).unwrap();
    assert_eq!((report.pushed, report.failed), (0, 0), "{:?}", report.items);
    assert_eq!(spy.inner.description(&id), Some(ahead));
    assert_eq!(spy.pushes.get(), 0);
}

#[test]
fn rate_limit_stops_the_run_keeps_earlier_sessions_and_the_next_run_resumes() {
    let (m, key) = (machine(), Key::generate());
    three(&m);
    let spy = Spy::default();
    *spy.push_error.borrow_mut() = Some((2, BackendError::RateLimited));
    let err = m.engine(&spy, &key).push(&opts()).unwrap_err();
    assert_eq!(err, SyncError::RateLimited { left: 2 });
    let (index, _) = index::load(m.dir());
    assert!(!index.entries[A].pending && index.entries[A].gen == 1);
    assert!(index.entries["claude/b.cast.gz"].pending);
    assert!(!index.entries.contains_key("codex/c.cast.gz"));
    let report = m.engine(&spy, &key).push(&opts()).unwrap();
    assert_eq!((report.pushed, report.failed), (2, 0), "{:?}", report.items);
    assert_eq!((spy.creates.get(), spy.inner.len()), (3, 3));
}

const OLDER: &str = "remote version is older than the last sync, not pulling";

fn sealed_gen(key: &Key, path: &str, plain: &[u8], gen: u64) -> (String, Vec<u8>) {
    let meta = Meta {
        v: 2,
        path: path.into(),
        sha256: sha256_hex(plain),
        size: plain.len() as u64,
        gen,
    };
    (seal_meta(key, &meta), seal_blob(key, path, gen, plain))
}

/// Machine A pushed gen 1, another machine then pushed gen 2 that A pulled.
fn pulled_gen_2() -> (Machine, MemoryBackend, Key, String) {
    let (m, backend, key) = pushed_machine();
    let id = only_entry(&m, A).remote_id;
    let (token, blob) = sealed_gen(&key, A, CAST.replace("secret", "two").as_bytes(), 2);
    backend.put(Some(&id), &token, &blob);
    let report = m.engine(&backend, &key).pull(&opts()).unwrap();
    assert_eq!(report.pulled, 1, "{:?}", report.items);
    (m, backend, key, id)
}

fn pull_item(m: &Machine, backend: &dyn SyncBackend, key: &Key) -> Outcome {
    let report = m.engine(backend, key).pull(&opts()).unwrap();
    assert_eq!(report.pulled, 0, "{:?}", report.items);
    item_of(&report, A).outcome.clone()
}

#[test]
fn manifest_gen_lower_than_the_index_gen_is_rejected_and_nothing_changes() {
    let (m, backend, key, id) = pulled_gen_2();
    let old = token_gen(&key, "x", 1);
    let (t3, b3) = sealed_gen(&key, A, b"three", 3);
    backend.put(Some(&id), &t3, &b3);
    backend.set_manifest(&id, Some(old));
    let (before, entry) = (fs::read(m.dir().join(A)).unwrap(), only_entry(&m, A));
    let names = m.names("claude");
    let outcome = pull_item(&m, &backend, &key);
    assert_eq!(outcome, Outcome::Failed(OLDER.into()));
    assert_eq!(fs::read(m.dir().join(A)).unwrap(), before);
    assert_eq!(only_entry(&m, A), entry);
    assert_eq!(m.names("claude"), names);
}

#[test]
fn manifest_for_another_path_is_rejected_and_nothing_is_written() {
    let (key, backend) = (Key::generate(), MemoryBackend::new());
    let (token, _) = sealed_gen(&key, A, b"plain", 1);
    let (other, blob) = sealed_gen(&key, "claude/b.cast.gz", b"plain", 1);
    let id = backend.put(None, &token, &blob);
    backend.set_manifest(&id, Some(other));
    let b = machine();
    let outcome = pull_item(&b, &backend, &key);
    assert!(matches!(&outcome, Outcome::Failed(why) if why.contains("another session")));
    assert!(b.names("claude").is_empty());
    assert!(index::load(b.dir()).0.entries.is_empty());
}

#[test]
fn blob_that_does_not_open_with_the_manifest_gen_is_rejected() {
    let (key, backend) = (Key::generate(), MemoryBackend::new());
    let (token, _) = sealed_gen(&key, A, b"plain", 2);
    let stale_blob = seal_blob(&key, A, 1, b"plain");
    backend.put(None, &token, &stale_blob);
    let b = machine();
    assert!(matches!(pull_item(&b, &backend, &key), Outcome::Failed(_)));
    assert!(b.names("claude").is_empty());
    assert!(index::load(b.dir()).0.entries.is_empty());
}

#[test]
fn missing_manifest_is_rejected_not_guessed() {
    let (key, backend) = (Key::generate(), MemoryBackend::new());
    let (token, blob) = sealed_gen(&key, A, b"plain", 1);
    let id = backend.put(None, &token, &blob);
    backend.set_manifest(&id, None);
    let b = machine();
    let outcome = pull_item(&b, &backend, &key);
    assert_eq!(
        outcome,
        Outcome::Failed("remote has no manifest, not pulling".into())
    );
    assert!(b.names("claude").is_empty());
}

/// Description lags at gen 1, the manifest and blob are gen 2.
fn manifest_newer_than_description() -> (MemoryBackend, Key, String, String) {
    let (key, backend) = (Key::generate(), MemoryBackend::new());
    let (t1, _) = sealed_gen(&key, A, b"one", 1);
    let (t2, b2) = sealed_gen(&key, A, b"two", 2);
    let id = backend.put(None, &t2, &b2);
    backend.set_description(&id, Some(t1));
    (backend, key, id, t2)
}

#[test]
fn manifest_newer_than_the_description_wins_and_the_description_is_repaired() {
    let (backend, key, id, t2) = manifest_newer_than_description();
    let b = machine();
    let report = b.engine(&backend, &key).pull(&opts()).unwrap();
    assert_eq!(report.pulled, 1, "{:?}", report.items);
    assert_eq!(fs::read(b.dir().join(A)).unwrap(), b"two");
    assert_eq!(backend.description(&id), Some(t2));
}

#[test]
fn pull_records_the_manifest_token_and_gen_not_the_description_ones() {
    let (backend, key, _id, t2) = manifest_newer_than_description();
    let b = machine();
    assert!(b.engine(&backend, &key).pull(&opts()).is_ok());
    let entry = index::load(b.dir()).0.entries.get(A).cloned();
    assert_eq!(entry.map(|e| (e.gen, e.token)), Some((2, Some(t2))));
}

#[test]
fn rate_limit_during_delete_stops_the_run_and_keeps_earlier_deletes() {
    let (m, spy, key) = spied();
    *spy.delete_error.borrow_mut() = Some((2, BackendError::RateLimited));
    let names: Vec<String> = [A, "claude/b.cast.gz", "codex/c.cast.gz"]
        .map(String::from)
        .to_vec();
    let result = m.engine(&spy, &key).delete(&names);
    assert_eq!(result.err(), Some(SyncError::RateLimited { left: 2 }));
    let (index, _) = index::load(m.dir());
    assert!(!index.entries.contains_key(A));
    assert!(index.entries.contains_key("claude/b.cast.gz"));
    assert_eq!(spy.inner.len(), 2);
}

/// Our push of an edited A loses to a manifest from `theirs`, the description stays behind.
fn lost_to(
    theirs: impl Fn(&Key) -> String,
    describe: Option<BackendError>,
) -> (Result<Outcome, SyncError>, Option<String>, Option<String>) {
    let (m, spy, key) = spied();
    let before = only_entry(&m, A);
    let (id, token) = (before.remote_id.clone(), theirs(&key));
    race(&spy, move |b| {
        b.set_manifest(&id, Some(token));
        b.set_blob(&id, b"theirs".to_vec());
    });
    *spy.describe_error.borrow_mut() = describe;
    edit_a(&m);
    let result = m.engine(&spy, &key).push(&opts());
    assert_eq!(only_entry(&m, A), before, "nothing is recorded");
    let description = spy.inner.description(&before.remote_id);
    let outcome = result.map(|r| item_of(&r, A).outcome.clone());
    (outcome, description, spy.inner.manifest(&before.remote_id))
}

type Theirs = Box<dyn Fn(&Key) -> String>;

fn failed_changed() -> Result<Outcome, SyncError> {
    Ok(Outcome::Failed(CHANGED.into()))
}

#[test]
fn lost_race_to_our_path_at_the_same_or_a_higher_gen_heals_the_description() {
    for gen in [1, 2, 5] {
        let (outcome, description, manifest) =
            lost_to(|key| token_gen(key, "other-hash", gen), None);
        assert_eq!(outcome, failed_changed(), "gen {gen}");
        assert!(manifest.is_some(), "gen {gen}");
        assert_eq!(description, manifest, "gen {gen}");
    }
}

#[test]
fn lost_race_to_a_lower_gen_another_path_or_another_key_describes_nothing() {
    let other_path = |key: &Key| {
        let meta = Meta {
            v: 2,
            path: "claude/b.cast.gz".into(),
            sha256: "x".into(),
            size: 1,
            gen: 2,
        };
        seal_meta(key, &meta)
    };
    let cases: [(&str, Theirs); 3] = [
        ("lower gen", Box::new(|key| token_gen(key, "x", 0))),
        ("other path", Box::new(other_path)),
        (
            "other key",
            Box::new(|_| token_gen(&Key::generate(), "x", 2)),
        ),
    ];
    for (name, theirs) in cases {
        let (outcome, description, manifest) = lost_to(theirs, None);
        assert_eq!(outcome, failed_changed(), "{name}");
        assert_ne!(description, manifest, "{name}: the description moved");
    }
}

#[test]
fn rate_limit_from_the_healing_describe_stops_the_run_any_other_error_reports_changed() {
    let theirs = |key: &Key| token_gen(key, "other-hash", 2);
    let (outcome, _, _) = lost_to(theirs, Some(BackendError::RateLimited));
    assert!(
        matches!(outcome, Err(SyncError::RateLimited { .. })),
        "{outcome:?}"
    );
    let down = BackendError::Unreachable("down".into());
    let (outcome, _, _) = lost_to(theirs, Some(down));
    assert_eq!(outcome, failed_changed());
}

#[test]
fn rate_limit_on_the_second_of_three_pulls_stops_the_run_and_keeps_the_first() {
    let (a, backend, key) = pushed_machine();
    let spy = Spy {
        inner: backend,
        ..Spy::default()
    };
    *spy.pull_error.borrow_mut() = Some((2, BackendError::RateLimited));
    let b = machine();
    let err = b.engine(&spy, &key).pull(&opts()).unwrap_err();
    assert_eq!(err, SyncError::RateLimited { left: 2 });
    let (index, _) = index::load(b.dir());
    assert_eq!(index.entries.len(), 1, "{:?}", index.entries);
    let first = index.entries.keys().next().unwrap();
    assert_eq!(
        fs::read(b.dir().join(first)).unwrap(),
        fs::read(a.dir().join(first)).unwrap()
    );
}

const CHANGED_WHILE: &str = "session changed while pushing, run again";

type Remote = (Option<String>, Option<Vec<u8>>);

fn remote_of(spy: &Spy, id: &str) -> Remote {
    (spy.inner.manifest(id), spy.inner.blob(id))
}

#[test]
fn a_session_changed_between_the_hash_and_the_seal_pushes_nothing() {
    let (m, spy, key) = spied();
    let before = only_entry(&m, A);
    let remote = remote_of(&spy, &before.remote_id);
    edit_a(&m);
    let file = m.dir().join(A);
    race(&spy, move |_| {
        let text = CAST.replace("secret", "edited again");
        codec::atomic_write(&file, |w| Ok(w.write_all(text.as_bytes())?)).unwrap();
    });
    let report = m.engine(&spy, &key).push(&opts()).unwrap();
    assert_eq!(
        item_of(&report, A).outcome,
        Outcome::Failed(CHANGED_WHILE.into())
    );
    assert_eq!(report.pushed, 0);
    assert_eq!(remote_of(&spy, &before.remote_id), remote);
    assert_eq!(only_entry(&m, A), before);
}

/// Reader that hands out one byte per call.
struct OneByte(Box<dyn std::io::Read>);

impl std::io::Read for OneByte {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let end = buf.len().min(1);
        self.0.read(&mut buf[..end])
    }
}

/// Memory backend whose pull reader gives one byte at a time and reports the local state at the
/// first read.
struct Slow<'a> {
    inner: &'a MemoryBackend,
    on_first_read: RefCell<Option<Box<dyn FnOnce() + 'a>>>,
}

impl SyncBackend for Slow<'_> {
    fn name(&self) -> &'static str {
        "slow"
    }
    fn check(&self) -> Result<(), BackendError> {
        self.inner.check()
    }
    fn list(&self) -> Result<Vec<RemoteEntry>, BackendError> {
        self.inner.list()
    }
    fn create(&self) -> Result<String, BackendError> {
        self.inner.create()
    }
    fn push(&self, id: &str, e: Option<&str>, m: &str, b: BlobSource) -> Result<(), BackendError> {
        self.inner.push(id, e, m, b)
    }
    fn describe(&self, id: &str, t: &str) -> Result<(), BackendError> {
        self.inner.describe(id, t)
    }
    fn pull(&self, id: &str) -> Result<(Option<String>, Box<dyn std::io::Read>), BackendError> {
        if let Some(hook) = self.on_first_read.borrow_mut().take() {
            hook();
        }
        let (manifest, reader) = self.inner.pull(id)?;
        Ok((manifest, Box::new(OneByte(reader))))
    }
    fn delete(&self, id: &str) -> Result<(), BackendError> {
        self.inner.delete(id)
    }
}

const BIG_PATH: &str = "claude/big.cast.gz";

fn big_plain() -> Vec<u8> {
    (0..200_000u32).map(|i| (i % 251) as u8).collect()
}

fn big_token(key: &Key, plain: &[u8], hash: &str) -> String {
    let meta = Meta {
        v: 2,
        path: BIG_PATH.into(),
        sha256: hash.into(),
        size: plain.len() as u64,
        gen: 1,
    };
    seal_meta(key, &meta)
}

/// Local copy of the session, its `.bak`, and the directory listing.
fn local_state(m: &Machine) -> (Vec<u8>, Vec<u8>, Vec<String>) {
    let dir = m.dir().join("claude");
    let bak = fs::read(dir.join(".big.cast.gz.bak")).unwrap();
    (
        fs::read(dir.join("big.cast.gz")).unwrap(),
        bak,
        m.names("claude"),
    )
}

fn local_with_bak() -> Machine {
    let m = machine();
    m.session("claude", "big.cast.gz", "mine");
    fs::write(m.dir().join("claude/.big.cast.gz.bak"), b"old bak").unwrap();
    m
}

fn pull_overwrite(m: &Machine, backend: &dyn SyncBackend, key: &Key) -> agr::sync::run::Report {
    m.engine(backend, key).pull(&overwrite()).unwrap()
}

#[test]
fn a_good_blob_replaces_the_file_after_rotating_the_backup() {
    let (key, backend) = (Key::generate(), MemoryBackend::new());
    let plain = big_plain();
    let blob = seal_blob(&key, BIG_PATH, 1, &plain);
    backend.put(None, &big_token(&key, &plain, &sha256_hex(&plain)), &blob);
    let m = local_with_bak();
    let old = fs::read(m.dir().join(BIG_PATH)).unwrap();
    let dir = m.dir().join("claude");
    let seen = Rc::new(RefCell::new(None));
    let slow = Slow {
        inner: &backend,
        on_first_read: RefCell::new(Some(Box::new({
            let seen = seen.clone();
            move || *seen.borrow_mut() = fs::read(dir.join(".big.cast.gz.bak")).ok()
        }))),
    };
    let report = pull_overwrite(&m, &slow, &key);
    assert_eq!((report.pulled, report.failed), (1, 0), "{:?}", report.items);
    assert_eq!(seen.borrow().as_deref(), Some(&b"old bak"[..]));
    let (file, bak, names) = local_state(&m);
    assert_eq!((file == plain, bak), (true, old));
    assert_eq!(names, [".big.cast.gz.bak", "big.cast.gz"]);
}

fn tampered(f: impl Fn(Vec<u8>) -> Vec<u8>, good_hash: bool) {
    let (key, backend) = (Key::generate(), MemoryBackend::new());
    let plain = big_plain();
    let hash = if good_hash {
        sha256_hex(&plain)
    } else {
        "00".repeat(32)
    };
    let id = backend.put(
        None,
        &big_token(&key, &plain, &hash),
        &seal_blob(&key, BIG_PATH, 1, &plain),
    );
    let control = machine();
    let ok = pull_overwrite(&control, &backend, &key).pulled;
    backend.set_blob(&id, f(backend.blob(&id).unwrap()));
    let m = local_with_bak();
    let before = local_state(&m);
    let report = pull_overwrite(&m, &backend, &key);
    assert_eq!((report.failed, report.pulled), (1, 0), "{:?}", report.items);
    assert_eq!(local_state(&m), before);
    assert_eq!(ok == 1, good_hash, "control pull of the untouched blob");
}

#[test]
fn a_truncated_blob_changes_nothing_locally() {
    tampered(|b| b[..b.len() - 100].to_vec(), true);
}

#[test]
fn a_blob_without_its_last_frame_changes_nothing_locally() {
    tampered(|b| b[..b.len() - 3_000 - 16].to_vec(), true);
}

#[test]
fn a_blob_with_a_dropped_middle_frame_changes_nothing_locally() {
    tampered(
        |b| [&b[..24 + 65_552], &b[24 + 2 * 65_552..]].concat(),
        true,
    );
}

#[test]
fn a_hash_mismatch_against_the_manifest_changes_nothing_locally() {
    tampered(|b| b, false);
}

#[test]
fn the_scan_hash_of_a_session_is_the_sha256_of_its_bytes() {
    let (m, backend, key) = (machine(), MemoryBackend::new(), Key::generate());
    let path = m.session("claude", "a.cast.gz", CAST);
    let report = m.engine(&backend, &key).status(&[]).unwrap();
    let hash = sha256_hex(&fs::read(path).unwrap());
    assert_eq!(
        item_of(&report, A).local_hash.as_deref(),
        Some(hash.as_str())
    );
}
