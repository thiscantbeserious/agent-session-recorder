use std::path::Path;

use super::{GistStore, PackStore, StoreError};
use crate::sync::backend::gist::fake::{fail, ok, Fake};

/// One gist of a listing page, files are `(name, size)`.
fn gist(id: &str, desc: &str, updated: &str, files: &[(&str, u64)]) -> serde_json::Value {
    let files: serde_json::Map<String, serde_json::Value> = files
        .iter()
        .map(|(n, size)| {
            let raw = format!("https://gist.githubusercontent.com/u/{id}/raw/sha/{n}");
            (
                n.to_string(),
                serde_json::json!({ "size": size, "raw_url": raw }),
            )
        })
        .collect();
    serde_json::json!({ "id": id, "description": desc, "updated_at": updated, "files": files })
}

/// `gh api --paginate` prints one array per page back to back.
fn pages(pages: &[Vec<serde_json::Value>]) -> String {
    pages
        .iter()
        .map(|p| serde_json::Value::from(p.clone()).to_string())
        .collect()
}

fn store<'a>(fake: &'a Fake, base: &Path) -> GistStore<&'a Fake> {
    GistStore::with_runner(fake, base.to_path_buf(), None)
}

fn pack(id: &str) -> serde_json::Value {
    gist(id, "agr:v2 TAG", "t1", &[("index", 10), ("data.000", 100)])
}

#[test]
fn list_keeps_only_agr_v2_over_pages() {
    let page1 = vec![pack("a"), gist("b", "my notes", "t1", &[("x", 1)])];
    let page2 = vec![pack("c"), gist("d", "agr:pending", "t1", &[(".keep", 4)])];
    let fake = Fake::with(vec![ok(&pages(&[page1, page2]))]);
    let dir = tempfile::tempdir().unwrap();
    let listing = store(&fake, dir.path()).list().unwrap();
    let ids: Vec<_> = listing.packs.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec!["a", "c"]);
    assert_eq!(
        (listing.packs[0].len, listing.packs[0].tag.as_str()),
        (100, "TAG")
    );
}

#[test]
fn list_passes_agr_v1_descriptions_as_legacy() {
    let items = vec![
        gist("a", "agr:v1 tok", "t1", &[("manifest", 1)]),
        gist("b", "agr:pending", "t1", &[(".keep", 4)]),
        gist("c", "someone else", "t1", &[("x", 1)]),
    ];
    let fake = Fake::with(vec![ok(&pages(&[items]))]);
    let dir = tempfile::tempdir().unwrap();
    let listing = store(&fake, dir.path()).list().unwrap();
    assert_eq!(listing.legacy, vec!["agr:v1 tok"]);
    assert!(listing.packs.is_empty());
}

#[test]
fn truncated_index_is_other_not_half_an_index() {
    let body = serde_json::json!({ "id": "a", "updated_at": "t1", "files": {
        "index": { "size": 9, "raw_url": "u", "truncated": true, "content": "half" } } });
    let fake = Fake::with(vec![ok(&body.to_string())]);
    let dir = tempfile::tempdir().unwrap();
    let res = store(&fake, dir.path()).read_index("a");
    assert!(matches!(res, Err(StoreError::Other(_))));
}

fn create_call(fake: &Fake) -> String {
    let calls = fake.calls();
    calls
        .into_iter()
        .find(|c| c.starts_with("gh gist create"))
        .unwrap_or_default()
}

#[test]
fn create_sends_only_the_index() {
    let fake = Fake::with(vec![ok("https://gist.github.com/u/abc123\n")]);
    let dir = tempfile::tempdir().unwrap();
    let id = store(&fake, dir.path())
        .create("TAG", "sealed-index")
        .unwrap();
    let call = create_call(&fake);
    let before_desc = call.split(" --desc ").next().unwrap_or_default();
    let files: Vec<_> = before_desc
        .split(' ')
        .skip(3)
        .map(|p| p.rsplit('/').next().unwrap_or_default())
        .collect();
    assert_eq!(files, vec!["index"]);
    assert_eq!(id, "abc123");
}

#[test]
fn pack_description_is_agr_v2_and_uuid_only() {
    let fake = Fake::with(vec![ok("https://gist.github.com/u/abc123\n")]);
    let dir = tempfile::tempdir().unwrap();
    store(&fake, dir.path())
        .create("TAG", "sealed-index")
        .unwrap();
    let call = create_call(&fake);
    let desc = call.split(" --desc ").nth(1).unwrap_or_default();
    assert_eq!(desc, format!("agr:v2 {}", "TAG"));
}

fn range_calls(calls: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for call in calls.iter().filter(|c| c.starts_with("gh api -H Range: ")) {
        let name = call.rsplit('/').next().unwrap_or_default();
        let range = call
            .split("Range: ")
            .nth(1)
            .and_then(|s| s.split(' ').next());
        out.push(format!("{name} {}", range.unwrap_or_default()));
    }
    out
}

fn three_parts() -> serde_json::Value {
    let files = [
        ("index", 10),
        ("data.000", 100),
        ("data.001", 100),
        ("data.002", 50),
    ];
    gist("a", "agr:v2 TAG", "t1", &files)
}

#[test]
fn read_range_requests_only_covering_parts() {
    let mut fake = Fake::with(vec![ok(&pages(&[vec![three_parts()]])), ok("206")]);
    fake.remote = vec![("data.001".into(), (0..100).collect())];
    let dir = tempfile::tempdir().unwrap();
    let store = store(&fake, dir.path());
    store.list().unwrap();
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut store.read("a", 105, 10).unwrap(), &mut bytes).unwrap();
    assert_eq!(range_calls(&fake.calls()), vec!["data.001 bytes=5-14"]);
    assert_eq!(bytes, (5..15).collect::<Vec<u8>>());
}

#[test]
fn read_range_uses_gh_api_not_curl() {
    let mut fake = Fake::with(vec![ok(&pages(&[vec![three_parts()]])), ok("")]);
    fake.remote = vec![("data.001".into(), (0..100).collect())];
    let dir = tempfile::tempdir().unwrap();
    let store = store(&fake, dir.path());
    store.list().unwrap();
    std::io::Read::read_to_end(&mut store.read("a", 105, 10).unwrap(), &mut Vec::new()).unwrap();
    let calls = fake.calls();
    assert!(calls.iter().all(|c| !c.starts_with("curl")));
    assert!(calls
        .iter()
        .any(|c| c.starts_with("gh api -H Range: bytes=")));
}

#[test]
fn read_range_is_binary_safe() {
    let original: Vec<u8> = (0..=255u8).chain([0xff, 0xfe, 0xc3, 0x28]).collect();
    let files = [("index", 10), ("data.000", original.len() as u64)];
    let item = gist("a", "agr:v2 TAG", "t1", &files);
    let mut fake = Fake::with(vec![ok(&pages(&[vec![item]])), ok("")]);
    fake.remote = vec![("data.000".into(), original.clone())];
    let dir = tempfile::tempdir().unwrap();
    let store = store(&fake, dir.path());
    store.list().unwrap();
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(
        &mut store.read("a", 0, original.len() as u64).unwrap(),
        &mut bytes,
    )
    .unwrap();
    assert_eq!(bytes, original);
}

#[test]
fn short_or_full_body_for_a_range_is_an_error() {
    let mut fake = Fake::with(vec![ok(&pages(&[vec![three_parts()]])), ok("200")]);
    fake.remote = vec![("data.001".into(), (0..100).collect())];
    fake.ignore_range = true;
    let dir = tempfile::tempdir().unwrap();
    let store = store(&fake, dir.path());
    store.list().unwrap();
    let res = store.read("a", 105, 10);
    assert!(res.is_err());
}

#[test]
fn failed_raw_url_read_is_a_store_error_no_clone() {
    let fake = Fake::with(vec![
        ok(&pages(&[vec![three_parts()]])),
        fail("dial tcp: no such host"),
    ]);
    let dir = tempfile::tempdir().unwrap();
    let store = store(&fake, dir.path());
    store.list().unwrap();
    let res = store.read("a", 0, 10);
    assert!(matches!(res, Err(StoreError::Unreachable(_))));
    assert!(!fake.calls().iter().any(|c| c.contains("clone")));
}

#[test]
fn fits_uses_the_file_count_of_the_listing() {
    let names: Vec<String> = (0..248).map(|i| format!("data.{i:03}")).collect();
    let mut files: Vec<(&str, u64)> = names.iter().map(|n| (n.as_str(), 1)).collect();
    files.push(("index", 1));
    let item = gist("a", "agr:v2 TAG", "t1", &files);
    let fake = Fake::with(vec![ok(&pages(&[vec![item]]))]);
    let dir = tempfile::tempdir().unwrap();
    let store = store(&fake, dir.path());
    let listing = store.list().unwrap();
    assert_eq!(listing.packs.len(), 1);
    let info = listing.packs[0].clone();
    assert!(store.fits(&info, &[1]));
    assert!(!store.fits(&info, &[9 * 1024 * 1024]));
}

fn cache_store<'a>(fake: &'a Fake, base: &Path) -> GistStore<&'a Fake> {
    let file = base.join("cache").join("index-cache.json");
    GistStore::with_runner(fake, base.to_path_buf(), Some(file))
}

fn api_calls_for(fake: &Fake, id: &str) -> usize {
    let want = format!("gh api gists/{id}");
    fake.calls().iter().filter(|c| **c == want).count()
}

fn index_body(updated: &str, text: &str) -> String {
    let file =
        serde_json::json!({ "size": 3, "raw_url": "u", "truncated": false, "content": text });
    let body = serde_json::json!({ "id": "a", "updated_at": updated, "files": { "index": file } });
    body.to_string()
}

fn listed(updated: &str) -> String {
    pages(&[vec![gist("a", "agr:v2 TAG", updated, &[("index", 3)])]])
}

/// Lists and reads the index once through a store of its own, as one `agr` run would.
fn run_once(dir: &Path, updated: &str, text: &str) -> (Fake, String) {
    let fake = Fake::with(vec![ok(&listed(updated)), ok(&index_body(updated, text))]);
    let store = cache_store(&fake, dir);
    store.list().unwrap();
    let index = store.read_index("a").unwrap();
    (fake, index)
}

#[test]
fn sealed_index_cache_hit_skips_the_api() {
    let dir = tempfile::tempdir().unwrap();
    run_once(dir.path(), "t1", "idx");
    let (fake, index) = run_once(dir.path(), "t1", "idx");
    assert_eq!(api_calls_for(&fake, "a"), 0);
    assert_eq!(index, "idx");
}

#[test]
fn changed_updated_at_refetches() {
    let dir = tempfile::tempdir().unwrap();
    run_once(dir.path(), "t1", "idx");
    let (fake, index) = run_once(dir.path(), "t2", "idx2");
    assert_eq!(api_calls_for(&fake, "a"), 1);
    assert_eq!(index, "idx2");
}

#[test]
fn cache_survives_corrupt_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("cache").join("index-cache.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "{not json").unwrap();
    let (_, text) = run_once(dir.path(), "t1", "idx");
    assert_eq!(text, "idx");
    let rewritten = std::fs::read_to_string(&file).unwrap();
    assert!(
        serde_json::from_str::<serde_json::Value>(&rewritten).is_ok(),
        "{rewritten}"
    );
}

#[test]
fn read_index_hits_the_cache_after_a_listing() {
    let dir = tempfile::tempdir().unwrap();
    run_once(dir.path(), "t1", "idx");
    let fake = Fake::with(vec![ok(&listed("t1")), ok(&listed("t1"))]);
    let store = cache_store(&fake, dir.path());
    store.list().unwrap();
    assert_eq!(store.read_index("a").unwrap(), "idx");
    store.list().unwrap();
    assert_eq!(store.read_index("a").unwrap(), "idx");
    assert_eq!(api_calls_for(&fake, "a"), 0);
}
