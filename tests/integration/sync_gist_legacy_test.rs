//! `list_all` of the gist backend, the read side of `agr sync migrate`, against the fake `gh`.

use crate::helpers::gist_fake::{backend, fail, ok, remote_parts, scrub, Fake};
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

#[test]
fn the_truncated_flag_of_a_gist_is_read() {
    let json = r#"[{"id":"a","truncated":true},{"id":"b"}]"#;
    let fake = Fake::with(vec![ok(json)]);
    let base = tempfile::tempdir().unwrap();
    let got = backend(&fake, base.path()).list_all().unwrap();
    assert_eq!(
        got.iter().map(|g| g.truncated).collect::<Vec<_>>(),
        [true, false]
    );
}

const CLONE: &str = "git -c credential.helper= -c credential.helper=!gh auth git-credential clone --depth 1 --progress https://gist.github.com/abc123.git <tmp>/agr-sync-N/clone";

fn read_all(mut r: Box<dyn std::io::Read>) -> Vec<u8> {
    let mut all = Vec::new();
    r.read_to_end(&mut all).unwrap();
    all
}

#[test]
fn pull_clones_once_and_joins_the_parts_in_order() {
    let fake = Fake {
        remote: vec![
            ("data.001".into(), b"world".to_vec()),
            ("data.000".into(), b"hello ".to_vec()),
            ("manifest".into(), b"CUR".to_vec()),
        ],
        ..Fake::default()
    };
    let base = tempfile::tempdir().unwrap();
    let (manifest, reader) = backend(&fake, base.path()).pull("abc123").unwrap();
    assert_eq!(manifest.as_deref(), Some("CUR"));
    assert_eq!(read_all(reader), b"hello world");
    assert_eq!(scrub(&fake.calls(), base.path()), [CLONE]);
}

#[test]
fn the_pull_reader_owns_the_clone_until_it_is_dropped() {
    let fake = Fake {
        remote: remote_parts(2),
        ..Fake::default()
    };
    let base = tempfile::tempdir().unwrap();
    let (_, reader) = backend(&fake, base.path()).pull("abc123").unwrap();
    assert_eq!(std::fs::read_dir(base.path()).unwrap().count(), 1);
    assert_eq!(read_all(reader), [0, 0, 0, 1, 1, 1]);
    assert_eq!(std::fs::read_dir(base.path()).unwrap().count(), 0);
}

#[test]
fn a_pull_with_a_missing_or_absent_part_errors_and_cleans_up() {
    let base = tempfile::tempdir().unwrap();
    let mut remote = remote_parts(3);
    remote.remove(1);
    let fake = Fake {
        remote,
        ..Fake::default()
    };
    let err = backend(&fake, base.path()).pull("abc123").err().unwrap();
    assert!(err.to_string().contains("data.001"), "{err}");
    assert!(backend(&Fake::default(), base.path())
        .pull("abc123")
        .is_err());
    assert_eq!(std::fs::read_dir(base.path()).unwrap().count(), 0);
}

#[test]
fn a_pull_with_a_manifest_and_no_data_parts_errors_and_cleans_up() {
    let fake = Fake {
        remote: vec![("manifest".into(), b"CUR".to_vec())],
        ..Fake::default()
    };
    let base = tempfile::tempdir().unwrap();
    let err = backend(&fake, base.path()).pull("abc123").err().unwrap();
    assert!(err.to_string().contains("has no data parts"), "{err}");
    assert_eq!(std::fs::read_dir(base.path()).unwrap().count(), 0);
}

#[test]
fn delete_uses_yes_flag_and_waits_on_the_pacer() {
    let fake = Fake::default();
    let base = tempfile::tempdir().unwrap();
    backend(&fake, base.path()).delete("abc123").unwrap();
    assert_eq!(fake.calls(), ["gh gist delete abc123 --yes"]);
}

#[cfg(unix)]
#[test]
fn scratch_dir_is_private() {
    use agr::files::scratch::Scratch;
    use std::os::unix::fs::PermissionsExt;
    let base = tempfile::tempdir().unwrap();
    let scratch = Scratch::create(base.path()).unwrap();
    let mode = std::fs::metadata(&scratch.0).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[cfg(unix)]
#[test]
fn scratch_at_is_private() {
    use agr::files::scratch::Scratch;
    use std::os::unix::fs::PermissionsExt;
    let base = tempfile::tempdir().unwrap();
    let scratch = Scratch::at(&base.path().join("one")).unwrap();
    let mode = std::fs::metadata(&scratch.0).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[test]
fn scratch_at_refuses_an_existing_dir() {
    use agr::files::scratch::Scratch;
    let base = tempfile::tempdir().unwrap();
    let result = Scratch::at(base.path());
    assert!(result.is_err());
    assert!(base.path().exists(), "the refused dir is not removed");
}
