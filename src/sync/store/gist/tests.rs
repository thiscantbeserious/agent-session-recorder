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
    for call in calls.iter().filter(|c| c.starts_with("curl ")) {
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
fn failed_raw_url_read_is_a_store_error_no_clone() {
    let fake = Fake::with(vec![
        ok(&pages(&[vec![three_parts()]])),
        fail("curl: (7) Failed to connect"),
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
