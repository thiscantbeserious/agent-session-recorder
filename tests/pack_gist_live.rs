//! Live checks of the gist store, one test per feature, each on its own short-lived secret gist
//! with the logged in `gh`. Run by hand:
//! `cargo test --test pack_gist_live -- --ignored --nocapture --test-threads=1`.
//!
//! One thread, because pushes to one gist are paced 10 s apart and parallel tests would run
//! into the GitHub rate limits. Expect about 60 to 90 s in total. The run creates and deletes
//! 5 secret gists, a test that fails midway still deletes its gist through the guard.

use agr::sync::testing::{sha256_hex, GistStore, PackStore, Segment, StoreError};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Instant;

const MIB: usize = 1024 * 1024;

/// Fallback delete on drop, also when an assertion panics. Each test deletes its gist itself
/// and asserts it, so a failed delete is visible. The guard ignores the result, so a gist that
/// is already deleted is fine.
struct Cleanup(GistStore, String);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = self.0.delete(&self.1);
    }
}

/// Runs one step and prints how long it took.
fn step<T>(test: &str, name: &str, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let out = f();
    eprintln!("LIVE: {test} {name} {} ms", t.elapsed().as_millis());
    out
}

fn read(store: &GistStore, id: &str, offset: u64, len: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut reader = store.read(id, offset, len).expect("read");
    reader.read_to_end(&mut bytes).expect("bytes");
    bytes
}

/// Every byte value in a row, so it is no valid UTF-8 and no part is all the same.
fn big_bytes(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i % 251 + i / MIB) as u8)
        .chain([0xFF, 0xC0])
        .collect()
}

fn append(
    store: &GistStore,
    id: &str,
    len: u64,
    old: &str,
    data: &[u8],
    new: &str,
) -> Result<(), StoreError> {
    let write = |w: &mut dyn Write| w.write_all(data);
    let seg = [Segment {
        len: data.len() as u64,
        write: &write,
    }];
    store.append(id, len, old, &seg, new)
}

/// The clone dir of the pack under a cache dir, the one that must be reused.
fn clones(cache: &Path) -> Vec<std::path::PathBuf> {
    let dir = cache.join("agr").join("packs");
    let all = std::fs::read_dir(dir)
        .expect("packs dir")
        .map(|e| e.expect("entry").path());
    all.filter(|p| p.join(".git").is_dir()).collect()
}

/// A store with its own temp cache, one fresh gist and the guard that deletes it.
fn setup(test: &str) -> (GistStore, tempfile::TempDir, String, Cleanup) {
    let cache = tempfile::tempdir().expect("cache");
    let store = GistStore::new().with_cache_dir(cache.path().to_path_buf());
    store.check().expect("gh and git ready");
    let id = step(test, "create", || {
        store.create("agr-live-test", "i0").expect("create")
    });
    let guard = Cleanup(
        GistStore::new().with_cache_dir(cache.path().to_path_buf()),
        id.clone(),
    );
    (store, cache, id, guard)
}

fn delete(test: &str, store: &GistStore, id: &str) {
    step(test, "delete", || {
        store.delete(id).expect("delete the live gist")
    });
}

#[test]
#[ignore = "talks to GitHub, run by hand"]
fn live_multi_part_session_round_trip() {
    let t = "multi_part";
    let (store, _cache, id, _guard) = setup(t);
    let small = b"first segment".to_vec();
    step(t, "append small", || {
        append(&store, &id, 0, "i0", &small, "i1").expect("append small")
    });
    let at = small.len() as u64;
    let big = big_bytes(18 * MIB);
    step(t, "append 18 MiB", || {
        append(&store, &id, at, "i1", &big, "i2").expect("append big")
    });

    let listing = step(t, "list", || store.list().expect("list"));
    let info = listing.packs.iter().find(|p| p.id == id).expect("listed");
    assert_eq!(
        info.files, 4,
        "data.000 for the small one, three for the big"
    );
    assert_eq!(info.len, at + big.len() as u64);

    let got = step(t, "read big", || read(&store, &id, at, big.len() as u64));
    assert_eq!(sha256_hex(&got), sha256_hex(&big), "big segment sha256");
    let edge = at + 8 * MIB as u64 - 512;
    let part = step(t, "read across part", || read(&store, &id, edge, 1024));
    let from = (edge - at) as usize;
    assert_eq!(
        part,
        big[from..from + 1024],
        "range across the 8 MiB boundary"
    );
    delete(t, &store, &id);
}

#[test]
#[ignore = "talks to GitHub, run by hand"]
fn live_cached_clone_is_reused() {
    let t = "cached_clone";
    let (store, cache, id, _guard) = setup(t);
    step(t, "append 1", || {
        append(&store, &id, 0, "i0", b"one", "i1").expect("append 1")
    });
    // A marker inside `.git` only survives while the clone does.
    let dirs = clones(cache.path());
    assert_eq!(dirs.len(), 1, "one clone after the first append");
    let marker = dirs[0].join(".git").join("agr-live-marker");
    std::fs::write(&marker, "x").expect("marker");
    step(t, "append 2", || {
        append(&store, &id, 3, "i1", b"two", "i2").expect("append 2")
    });
    assert_eq!(clones(cache.path()), dirs, "still the one clone");
    assert!(marker.exists(), "second append reused the cached clone");
    delete(t, &store, &id);
}

#[test]
#[ignore = "talks to GitHub, run by hand"]
fn live_stale_writer_gets_moved_and_retries() {
    let t = "stale_writer";
    let (a, _cache_a, id, _guard) = setup(t);
    let cache_b = tempfile::tempdir().expect("cache b");
    let b = GistStore::new().with_cache_dir(cache_b.path().to_path_buf());
    step(t, "append A1", || {
        append(&a, &id, 0, "i0", b"first", "i1").expect("append A1")
    });
    step(t, "append B", || {
        append(&b, &id, 5, "i1", b"second", "i2").expect("append B")
    });
    step(t, "append A2", || {
        append(&a, &id, 11, "i2", b"third", "i3").expect("append A2")
    });
    // B still thinks the gist is at i2.
    let res = step(t, "stale append B", || {
        append(&b, &id, 11, "i2", b"late", "bx")
    });
    assert_eq!(res, Err(StoreError::Moved("i3".into())), "A's index");
    step(t, "retry B", || {
        append(&b, &id, 16, "i3", b"late", "i4").expect("retry B")
    });
    assert_eq!(a.read_index(&id).expect("index a"), "i4");
    assert_eq!(b.read_index(&id).expect("index b"), "i4");
    assert_eq!(read(&a, &id, 16, 4), b"late");
    assert_eq!(read(&b, &id, 16, 4), b"late");
    delete(t, &a, &id);
}

#[test]
#[ignore = "talks to GitHub, run by hand"]
fn live_zero_byte_append_replaces_the_index() {
    let t = "zero_byte";
    let (store, _cache, id, _guard) = setup(t);
    let data = b"some bytes".to_vec();
    step(t, "append data", || {
        append(&store, &id, 0, "i0", &data, "i1").expect("append data")
    });
    step(t, "zero byte append", || {
        store
            .append(&id, data.len() as u64, "i1", &[], "i2")
            .expect("zero byte")
    });
    assert_eq!(store.read_index(&id).expect("index"), "i2");
    assert_eq!(read(&store, &id, 0, data.len() as u64), data);
    delete(t, &store, &id);
}

#[test]
#[ignore = "talks to GitHub, run by hand"]
fn live_delete_removes_the_pack() {
    let t = "delete";
    let (store, _cache, id, _guard) = setup(t);
    step(t, "append", || {
        append(&store, &id, 0, "i0", b"bytes", "i1").expect("append")
    });
    delete(t, &store, &id);
    assert_eq!(store.read_index(&id), Err(StoreError::NotFound));
    let listing = store.list().expect("list");
    assert!(listing.packs.iter().all(|p| p.id != id), "gone from list");
}
