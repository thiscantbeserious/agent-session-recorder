//! Live check of the gist store append against a real secret gist with the logged in `gh`.
//! Run by hand once: `cargo test --test pack_gist_live -- --ignored --nocapture`.

use agr::sync::testing::{GistStore, PackStore, Segment};
use std::io::Read;

/// Fallback delete on drop, also when an assertion panics. The test deletes the gist itself
/// and asserts it, so a failed delete is visible.
struct Cleanup<'a>(&'a GistStore, String);

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = self.0.delete(&self.1);
    }
}

fn read(store: &GistStore, id: &str, offset: u64, len: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut reader = store.read(id, offset, len).expect("read");
    reader.read_to_end(&mut bytes).expect("bytes");
    bytes
}

#[test]
#[ignore = "talks to GitHub, run by hand"]
fn create_append_twice_zero_byte_read_delete() {
    let cache = tempfile::tempdir().expect("temp cache dir");
    let store = GistStore::new().with_cache_dir(cache.path().to_path_buf());
    store.check().expect("gh and git ready");
    let id = store.create("agr-live-test", "i0").expect("create");
    let _cleanup = Cleanup(&store, id.clone());
    let (a, b) = (b"first segment".to_vec(), b"second segment".to_vec());
    let wa = |w: &mut dyn std::io::Write| w.write_all(&a);
    let wb = |w: &mut dyn std::io::Write| w.write_all(&b);
    let seg_a = [Segment {
        len: a.len() as u64,
        write: &wa,
    }];
    store
        .append(&id, 0, "i0", &seg_a, "i1")
        .expect("append one");
    let seg_b = [Segment {
        len: b.len() as u64,
        write: &wb,
    }];
    store
        .append(&id, a.len() as u64, "i1", &seg_b, "i2")
        .expect("append two");
    let total = (a.len() + b.len()) as u64;
    store
        .append(&id, total, "i2", &[], "i3")
        .expect("zero byte append");
    assert_eq!(store.read_index(&id).expect("index"), "i3");
    assert_eq!(read(&store, &id, 0, a.len() as u64), a);
    assert_eq!(read(&store, &id, a.len() as u64, b.len() as u64), b);
    store.delete(&id).expect("delete the live gist");
}
