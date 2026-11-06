//! Sync engine against the in-memory backend

use agr::files::{codec, lock};
use agr::sync::backend::{BackendError, RemoteEntry, SyncBackend};
use agr::sync::crypto::{blob_aad, seal, seal_meta, sha256_hex, Key, Meta};
use agr::sync::index::{self, Index};
use agr::sync::memory::MemoryBackend;
use agr::sync::plan::{Decision, SkipReason};
use agr::sync::run::{Engine, Options, Outcome, SyncError};
use agr::{Config, StorageManager};
use std::cell::{Cell, RefCell};
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
    let mut blob = seal(blob_key, &blob_aad(path, 1), b"plain");
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
    assert!(agr::sync::crypto::open(&key, &blob_aad(&meta.path, 1), &blob).is_ok());
    m.session("claude", "a.cast.gz", &CAST.replace("secret", "edited"));
    m.engine(&backend, &key).push(&opts()).unwrap();
    let id = only_entry(&m, "claude/a.cast.gz").remote_id;
    let token = backend.manifest(&id).unwrap();
    let meta = agr::sync::crypto::open_meta(&key, &token).unwrap();
    assert_eq!(meta.gen, 2);
    let blob = backend.blob(&id).unwrap();
    assert!(agr::sync::crypto::open(&key, &blob_aad(&meta.path, 2), &blob).is_ok());
    assert!(agr::sync::crypto::open(&key, &blob_aad(&meta.path, 1), &blob).is_err());
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
    let id = backend.put(None, &t2, &seal(&key, &blob_aad(path, 2), plain));
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
        b: &[u8],
    ) -> Result<(), agr::sync::backend::BackendError> {
        self.0.push(id, e, m, b)
    }
    fn describe(&self, id: &str, t: &str) -> Result<(), agr::sync::backend::BackendError> {
        self.0.describe(id, t)
    }
    fn pull(
        &self,
        _id: &str,
    ) -> Result<(Option<String>, Vec<u8>), agr::sync::backend::BackendError> {
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
        backend.put(None, &tok, &seal(&key, &blob_aad(path, 1), b"x"));
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
    fn push(&self, id: &str, e: Option<&str>, m: &str, b: &[u8]) -> Result<(), BackendError> {
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
        match self.describe_error.borrow_mut().take() {
            Some(e) => Err(e),
            None => self.inner.describe(id, t),
        }
    }
    fn pull(&self, id: &str) -> Result<(Option<String>, Vec<u8>), BackendError> {
        self.inner.pull(id)
    }
    fn delete(&self, id: &str) -> Result<(), BackendError> {
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
