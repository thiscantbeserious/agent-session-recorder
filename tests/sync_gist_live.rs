//! Live gist round trip, creates and deletes a real secret gist with the logged in `gh`.
//! Run by hand only: `cargo test --test sync_gist_live -- --ignored`.

use agr::sync::backend::SyncBackend;
use agr::sync::gist::GistBackend;

/// Deterministic pseudo random bytes, incompressible enough for a binary push check.
fn binary_blob(len: usize) -> Vec<u8> {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

/// Deletes the gist on drop, also when an assertion panics.
struct Cleanup<'a>(&'a GistBackend, String);

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = self.0.delete(&self.1);
    }
}

#[test]
#[ignore = "creates a real secret gist, run by hand"]
fn gist_round_trip_binary_blob() {
    let backend = GistBackend::new();
    backend.check().unwrap();
    let blob = binary_blob(2 * 1024 * 1024);
    let token = "live-test-token".repeat(17);
    let id = backend.push(None, &token, &blob).unwrap();
    let _cleanup = Cleanup(&backend, id.clone());
    let listed = backend.list().unwrap();
    assert!(listed.iter().any(|e| e.id == id && e.meta_token == token));
    assert_eq!(backend.pull(&id).unwrap(), blob);
    let blob2 = binary_blob(1024);
    backend.push(Some(&id), "live-test-2", &blob2).unwrap();
    assert_eq!(backend.pull(&id).unwrap(), blob2);
}
