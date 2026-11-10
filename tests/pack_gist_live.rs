//! Live end to end check of the gist store against one real secret gist with the logged in `gh`.
//! Run by hand, it takes a few minutes because pushes to one gist are paced 10 s apart:
//! `cargo test --test pack_gist_live -- --ignored --nocapture`.

use agr::sync::testing::{sha256_hex, GistStore, PackStore, Segment, StoreError};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Instant;

const MIB: usize = 1024 * 1024;

/// Fallback delete on drop, also when an assertion panics. The test deletes the gist itself
/// and asserts it, so a failed delete is visible.
struct Cleanup<'a>(&'a GistStore, String);

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = self.0.delete(&self.1);
    }
}

/// Runs one step and prints how long it took.
fn step<T>(name: &str, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let out = f();
    eprintln!("LIVE: {name} {} ms", t.elapsed().as_millis());
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

#[test]
#[ignore = "talks to GitHub, run by hand"]
fn live_end_to_end_one_gist() {
    let total = Instant::now();
    let (cache_a, cache_b) = (
        tempfile::tempdir().expect("cache a"),
        tempfile::tempdir().expect("cache b"),
    );
    let store = GistStore::new().with_cache_dir(cache_a.path().to_path_buf());
    let other = GistStore::new().with_cache_dir(cache_b.path().to_path_buf());
    store.check().expect("gh and git ready");

    let id = step("create", || {
        store.create("agr-live-test", "i0").expect("create")
    });
    let _cleanup = Cleanup(&store, id.clone());

    let small = b"first segment".to_vec();
    step("append small", || {
        append(&store, &id, 0, "i0", &small, "i1").expect("append small")
    });
    let small_len = small.len() as u64;

    // The cached clone is reused: a marker inside `.git` only survives while the clone does.
    let dirs = clones(cache_a.path());
    assert_eq!(dirs.len(), 1, "one clone after the first append");
    let marker = dirs[0].join(".git").join("agr-live-marker");
    std::fs::write(&marker, "x").expect("marker");

    let big = big_bytes(18 * MIB);
    step("append 18 MiB", || {
        append(&store, &id, small_len, "i1", &big, "i2").expect("append big")
    });
    assert_eq!(clones(cache_a.path()), dirs, "still the one clone");
    assert!(marker.exists(), "second append reused the cached clone");
    let listing = step("list", || store.list().expect("list"));
    let info = listing
        .packs
        .iter()
        .find(|p| p.id == id)
        .expect("pack listed");
    assert_eq!(
        info.files, 4,
        "data.000 for the small one, three parts for the big one"
    );
    assert_eq!(info.len, small_len + big.len() as u64);

    let at = small_len;
    let got = step("read big", || read(&store, &id, at, big.len() as u64));
    assert_eq!(sha256_hex(&got), sha256_hex(&big), "big segment sha256");
    let edge = (at + 8 * MIB as u64 - 512, 1024);
    let part = step("read across part", || read(&store, &id, edge.0, edge.1));
    let from = (edge.0 - at) as usize;
    assert_eq!(
        part,
        big[from..from + 1024],
        "range across the 8 MiB part boundary"
    );

    // Instance B has its own cache and a stale view, A moves the gist on first.
    let len2 = small_len + big.len() as u64;
    let tail = b"third segment".to_vec();
    step("append via A", || {
        append(&store, &id, len2, "i2", &tail, "i3").expect("append A")
    });
    let len3 = len2 + tail.len() as u64;
    let res = step("stale append via B", || {
        append(&other, &id, len2, "i2", b"late", "bx")
    });
    assert_eq!(
        res,
        Err(StoreError::Moved("i3".into())),
        "stale B is refused with A's index"
    );
    step("retry via B", || {
        append(&other, &id, len3, "i3", b"late", "i4").expect("retry B")
    });
    assert_eq!(store.read_index(&id).expect("index"), "i4");
    assert_eq!(read(&other, &id, len3, 4), b"late");

    // A is stale now too, so this also fast-forwards the cached clone.
    let len4 = len3 + 4;
    step("zero byte append", || {
        store.append(&id, len4, "i4", &[], "i5").expect("zero byte")
    });
    assert_eq!(store.read_index(&id).expect("index"), "i5");
    assert_eq!(read(&store, &id, 0, small_len), small);
    assert_eq!(clones(cache_a.path()).len(), 1, "still one clone in A");

    eprintln!("LIVE: seal skipped, pack layer decides");
    step("delete", || {
        store.delete(&id).expect("delete the live gist")
    });
    assert_eq!(store.read_index(&id), Err(StoreError::NotFound));
    eprintln!("LIVE: total {} ms", total.elapsed().as_millis());
}
