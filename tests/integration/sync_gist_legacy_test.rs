//! `list_all` of the gist backend, the read side of `agr sync migrate`, against the fake `gh`.

use crate::helpers::gist_fake::{backend, fail, ok, Fake};
use agr::sync::testing::LegacyRead;

const ARGS: &str = "gh api gists?per_page=100 --paginate";

#[test]
fn two_concatenated_pages_with_files_sizes_and_created_at() {
    let page1 = r#"[{"id":"a","description":"agr:v1 T","created_at":"2026-01-01T00:00:00Z","files":{"manifest":{"size":9},".keep":{"size":4}}}]"#;
    let page2 = r#"[{"id":"b","description":"notes","created_at":"2026-01-02T00:00:00Z","files":{"a.txt":{"size":1}}}]"#;
    let fake = Fake::with(vec![ok(&format!("{page1}{page2}"))]);
    let base = tempfile::tempdir().unwrap();
    let got = backend(&fake, base.path()).list_all().unwrap();
    assert_eq!(fake.calls(), [ARGS]);
    let ids: Vec<_> = got.iter().map(|g| g.id.as_str()).collect();
    assert_eq!(ids, ["a", "b"]);
    assert_eq!(got[0].description, "agr:v1 T");
    assert_eq!(got[0].created_at, "2026-01-01T00:00:00Z");
    assert_eq!(
        got[0].files,
        vec![(".keep".to_string(), 4), ("manifest".to_string(), 9)]
    );
    assert_eq!(got[1].files, vec![("a.txt".to_string(), 1)]);
}

#[test]
fn missing_description_and_created_at_read_as_empty() {
    let fake = Fake::with(vec![ok(r#"[{"id":"a","description":null},{"id":"b"}]"#)]);
    let base = tempfile::tempdir().unwrap();
    let got = backend(&fake, base.path()).list_all().unwrap();
    assert_eq!(got.len(), 2);
    assert!(got.iter().all(|g| g.description.is_empty()));
    assert!(got
        .iter()
        .all(|g| g.created_at.is_empty() && g.files.is_empty()));
}

#[test]
fn a_malformed_second_page_is_an_error_without_a_partial_list() {
    let fake = Fake::with(vec![ok(r#"[{"id":"a"}][{"id":"#)]);
    let base = tempfile::tempdir().unwrap();
    assert!(backend(&fake, base.path()).list_all().is_err());
}

#[test]
fn a_failing_gh_is_an_error() {
    let fake = Fake::with(vec![fail("HTTP 502")]);
    let base = tempfile::tempdir().unwrap();
    assert!(backend(&fake, base.path()).list_all().is_err());
}

#[test]
fn listing_past_3000_gists_is_not_capped() {
    let page = |from: usize| {
        let all: Vec<String> = (from..from + 100)
            .map(|n| format!(r#"{{"id":"g{n}","description":"notes"}}"#))
            .collect();
        format!("[{}]", all.join(","))
    };
    let pages: String = (0..32).map(|p| page(p * 100)).collect();
    let fake = Fake::with(vec![ok(&pages)]);
    let base = tempfile::tempdir().unwrap();
    let got = backend(&fake, base.path()).list_all().unwrap();
    assert_eq!(got.len(), 3200);
}
