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

/// Sorted file names of a gist, read through `gh api`.
fn gist_files(id: &str) -> Vec<String> {
    let out = std::process::Command::new("gh")
        .args(["api", &format!("gists/{id}"), "--jq", ".files | keys | .[]"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
#[ignore = "creates a real secret gist, run by hand"]
fn gist_parts_stub_and_shrink() {
    let backend = GistBackend::new().with_part_size(1024 * 1024);
    backend.check().unwrap();
    let blob = binary_blob(3 * 1024 * 1024 + 5);
    let id = backend.create().unwrap();
    backend.push(&id, None, "live-parts", &blob).unwrap();
    let _cleanup = Cleanup(&backend, id.clone());
    assert_eq!(
        gist_files(&id),
        [".keep", "data.000", "data.001", "data.002", "data.003"]
    );
    assert_eq!(backend.pull(&id).unwrap().1, blob);
    let small = binary_blob(10);
    backend
        .push(&id, Some("live-parts"), "live-shrunk", &small)
        .unwrap();
    assert_eq!(gist_files(&id), [".keep", "data.000"]);
    assert_eq!(backend.pull(&id).unwrap().1, small);
}

#[test]
#[ignore = "creates a real secret gist, run by hand"]
fn gist_round_trip_binary_blob() {
    let backend = GistBackend::new();
    backend.check().unwrap();
    let blob = binary_blob(2 * 1024 * 1024);
    let token = "live-test-token".repeat(17);
    let id = backend.create().unwrap();
    backend.push(&id, None, &token, &blob).unwrap();
    backend.describe(&id, &token).unwrap();
    let _cleanup = Cleanup(&backend, id.clone());
    let listed = backend.list().unwrap();
    assert!(listed.iter().any(|e| e.id == id && e.meta_token == token));
    assert_eq!(backend.pull(&id).unwrap().1, blob);
    let blob2 = binary_blob(1024);
    backend
        .push(&id, Some(&token), "live-test-2", &blob2)
        .unwrap();
    assert_eq!(backend.pull(&id).unwrap().1, blob2);
}

#[test]
#[ignore = "creates a real secret gist, run by hand"]
fn gist_compare_and_swap_and_manifest() {
    use agr::sync::backend::BackendError;
    let backend = GistBackend::new();
    backend.check().unwrap();
    let id = backend.create().unwrap();
    let _cleanup = Cleanup(&backend, id.clone());
    let blob1 = binary_blob(2048);
    backend.push(&id, None, "gen-1", &blob1).unwrap();
    assert_eq!(gist_files(&id), [".keep", "data.000", "manifest"]);
    assert_eq!(
        backend.pull(&id).unwrap(),
        (Some("gen-1".into()), blob1.clone())
    );
    let stale = backend.push(&id, Some("gen-0"), "gen-2", b"lost");
    assert_eq!(stale, Err(BackendError::Changed(Some("gen-1".into()))));
    let none = backend.push(&id, None, "gen-2", b"lost");
    assert_eq!(none, Err(BackendError::Changed(Some("gen-1".into()))));
    assert_eq!(backend.pull(&id).unwrap(), (Some("gen-1".into()), blob1));
    let blob2 = binary_blob(512);
    backend.push(&id, Some("gen-1"), "gen-2", &blob2).unwrap();
    assert_eq!(backend.pull(&id).unwrap(), (Some("gen-2".into()), blob2));
    backend.describe(&id, "tok").unwrap();
    let listed = backend.list().unwrap();
    assert!(listed
        .iter()
        .any(|e| e.id == id && !e.created_at.is_empty()));
    assert_eq!(
        backend.pull("0000000000000000000000000000dead"),
        Err(BackendError::NotFound)
    );
}

#[test]
#[ignore = "creates a real secret gist, run by hand"]
fn gist_stale_description_is_repaired() {
    let backend = GistBackend::new();
    backend.check().unwrap();
    let id = backend.create().unwrap();
    let _cleanup = Cleanup(&backend, id.clone());
    backend.push(&id, None, "gen-1", b"one").unwrap();
    backend.describe(&id, "gen-1").unwrap();
    backend.push(&id, Some("gen-1"), "gen-2", b"two").unwrap();
    let listed = |b: &GistBackend| {
        b.list()
            .unwrap()
            .into_iter()
            .find(|e| e.id == id)
            .map(|e| e.meta_token)
    };
    assert_eq!(listed(&backend).as_deref(), Some("gen-1"));
    backend.describe(&id, "gen-2").unwrap();
    assert_eq!(listed(&backend).as_deref(), Some("gen-2"));
    assert_eq!(backend.pull(&id).unwrap().0.as_deref(), Some("gen-2"));
}
