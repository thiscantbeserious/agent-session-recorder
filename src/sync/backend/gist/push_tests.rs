use super::fake::{backend, fail, ok, remote_parts, scrub, source_of, Bytes, Fake};
use super::pace::tests::FakeClock;
use super::parts::part_name;
use super::{BackendError, SyncBackend, MANIFEST_FILE};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn push_clones_commits_and_pushes_without_touching_the_description() {
    let fake = Fake::default();
    let base = tempfile::tempdir().unwrap();
    backend(&fake, base.path())
        .push_b("abc123", None, "MANIFEST", b"sealed")
        .unwrap();
    let calls = scrub(&fake.calls(), base.path());
    assert_eq!(
        calls,
        [
            "git -c credential.helper= -c credential.helper=!gh auth git-credential clone --depth 1 --progress https://gist.github.com/abc123.git <tmp>/agr-sync-N/clone",
            "git add --all -- data.000 manifest",
            "git -c user.name=agr -c user.email=agr@localhost -c commit.gpgsign=false -c core.hooksPath=<tmp>/agr-sync-N/no-hooks commit -m agr sync",
            "git push --no-verify --progress",
        ]
    );
    assert_eq!(
        fake.pushed.borrow().clone().unwrap(),
        [("data.000".to_string(), b"sealed".to_vec())]
    );
}

fn with_manifest(manifest: &str) -> Fake {
    Fake {
        remote: vec![(MANIFEST_FILE.to_string(), manifest.as_bytes().to_vec())],
        ..Fake::default()
    }
}

#[test]
fn a_stale_expected_is_changed_and_nothing_is_written() {
    let base = tempfile::tempdir().unwrap();
    let cases = [
        (with_manifest("CUR"), Some("stale"), Some("CUR")),
        (with_manifest("CUR"), None, Some("CUR")),
        (Fake::default(), Some("stale"), None),
    ];
    for (fake, expected, current) in cases {
        let got = backend(&fake, base.path()).push_b("abc123", expected, "NEW", b"x");
        let want = BackendError::Changed(current.map(str::to_string));
        assert_eq!(got, Err(want));
        let calls = scrub(&fake.calls(), base.path());
        assert_eq!(calls.len(), 1, "{calls:?}");
        assert!(
            calls[0].contains(" clone --depth 1 --progress https://gist.github.com/"),
            "{calls:?}"
        );
        assert!(fake.pushed.borrow().is_none());
    }
}

#[test]
fn a_matching_expected_pushes_blob_and_manifest() {
    let fake = with_manifest("CUR");
    let base = tempfile::tempdir().unwrap();
    backend(&fake, base.path())
        .push_b("abc123", Some("CUR"), "NEW", b"blob")
        .unwrap();
    assert_eq!(fake.pushed_manifest.borrow().as_deref(), Some("NEW"));
    assert_eq!(
        fake.pushed.borrow().clone().unwrap(),
        [("data.000".to_string(), b"blob".to_vec())]
    );
}

#[test]
fn push_never_forces_and_a_rejection_is_not_retried() {
    let base = tempfile::tempdir().unwrap();
    for stderr in [
        "! [rejected] main -> main (fetch first)",
        "error: failed to push some refs (non-fast-forward)",
        "hint: Updates were rejected, fetch first",
    ] {
        let fake = Fake::with(vec![ok(""), ok(""), ok(""), fail(stderr)]);
        let got = backend(&fake, base.path()).push_b("abc123", None, "M", b"x");
        assert_eq!(got, Err(BackendError::Changed(None)), "{stderr}");
        let calls = fake.calls();
        let pushes: Vec<_> = calls.iter().filter(|c| c.contains("push")).collect();
        assert_eq!(
            pushes,
            ["git push --no-verify --progress"],
            "one push, no retry"
        );
        let forced = |a: &str| a == "--force" || a == "-f" || a.starts_with('+');
        assert!(!calls.iter().flat_map(|c| c.split(' ')).any(forced));
    }
}

#[test]
fn the_manifest_is_staged_and_committed_with_the_parts() {
    let fake = Fake::default();
    let base = tempfile::tempdir().unwrap();
    backend(&fake, base.path())
        .push_b("abc123", None, "M", b"x")
        .unwrap();
    let calls = fake.calls();
    let adds: Vec<_> = calls.iter().filter(|c| c.starts_with("git add")).collect();
    assert_eq!(adds, ["git add --all -- data.000 manifest"]);
    assert_eq!(calls.iter().filter(|c| c.contains(" commit ")).count(), 1);
    assert_eq!(fake.pushed_manifest.borrow().as_deref(), Some("M"));
}

#[test]
fn pull_returns_the_manifest_from_the_same_clone() {
    let mut fake = with_manifest("CUR");
    fake.remote.push((part_name(0), b"blob".to_vec()));
    let base = tempfile::tempdir().unwrap();
    let got = backend(&fake, base.path()).pull_b("abc123");
    assert_eq!(got, Ok((Some("CUR".to_string()), b"blob".to_vec())));
    assert_eq!(fake.calls().len(), 1, "one clone");
}

#[test]
fn rejected_git_push_is_changed_none() {
    let fake = Fake::with(vec![
        ok(""),
        ok(""),
        ok(""),
        fail("! [rejected] main -> main (fetch first)"),
    ]);
    let base = tempfile::tempdir().unwrap();
    let got = backend(&fake, base.path()).push_b("abc123", None, "M", b"x");
    assert_eq!(got, Err(BackendError::Changed(None)));
}

#[test]
fn temp_dirs_are_removed_on_success_and_on_error() {
    let base = tempfile::tempdir().unwrap();
    let fake = Fake::default();
    backend(&fake, base.path())
        .push_b("abc123", None, "T", b"x")
        .unwrap();
    let failing = Fake::with(vec![ok(""), fail("boom")]);
    assert!(backend(&failing, base.path())
        .push_b("abc123", None, "T", b"x")
        .is_err());
    assert_eq!(std::fs::read_dir(base.path()).unwrap().count(), 0);
}

#[test]
fn failed_push_keeps_the_gist_and_never_deletes() {
    let fake = Fake::with(vec![
        ok(""),
        ok(""),
        ok(""),
        fail("fatal: Authentication failed"),
    ]);
    let base = tempfile::tempdir().unwrap();
    let err = backend(&fake, base.path())
        .push_b("abc123", None, "T", b"x")
        .unwrap_err();
    assert!(err.to_string().contains("gh auth setup-git"), "{err}");
    assert!(!fake.calls().iter().any(|c| c.contains("delete")));
}

#[test]
fn large_blob_is_split_into_parts_in_one_commit() {
    let fake = Fake::default();
    let base = tempfile::tempdir().unwrap();
    let blob: Vec<u8> = (0..25u8).collect();
    backend(&fake, base.path())
        .with_part_size(10)
        .push_b("abc123", None, "T", &blob)
        .unwrap();
    let pushed = fake.pushed.borrow().clone().unwrap();
    let names: Vec<_> = pushed.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["data.000", "data.001", "data.002"]);
    assert_eq!(pushed[2].1, &blob[20..]);
    let calls = fake.calls();
    let adds: Vec<_> = calls.iter().filter(|c| c.starts_with("git add")).collect();
    assert_eq!(
        adds,
        ["git add --all -- data.000 data.001 data.002 manifest"]
    );
    assert_eq!(calls.iter().filter(|c| c.contains("commit")).count(), 1);
}

#[test]
fn only_git_commit_gets_utc_dates_from_the_clock() {
    let fake = Fake::default();
    let base = tempfile::tempdir().unwrap();
    backend(&fake, base.path())
        .with_clock(Arc::new(FakeClock::new(Duration::ZERO, 1_700_000_000)))
        .push_b("abc123", None, "T", b"x")
        .unwrap();
    let date = "@1700000000 +0000".to_string();
    let want: Vec<(String, String)> = [
        ("GIT_AUTHOR_DATE", date.as_str()),
        ("GIT_COMMITTER_DATE", date.as_str()),
        ("GIT_AUTHOR_NAME", "agr"),
        ("GIT_AUTHOR_EMAIL", "agr@localhost"),
        ("GIT_COMMITTER_NAME", "agr"),
        ("GIT_COMMITTER_EMAIL", "agr@localhost"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let calls = fake.calls();
    let envs = fake.envs.borrow();
    assert!(!calls.is_empty());
    for (call, env) in calls.iter().zip(envs.iter()) {
        if call.contains(" commit ") {
            assert_eq!(env, &want, "{call}");
        } else {
            assert!(env.is_empty(), "{call}: {env:?}");
        }
    }
    assert!(calls.iter().any(|c| c.contains("commit.gpgsign=false")));
    assert!(calls.iter().any(|c| c.contains("user.name=agr")));
    assert!(calls.iter().any(|c| c.contains("user.email=agr@localhost")));
}

#[test]
fn shrinking_push_removes_stale_parts_in_the_same_commit() {
    let fake = Fake {
        remote: remote_parts(4),
        ..Fake::default()
    };
    let base = tempfile::tempdir().unwrap();
    backend(&fake, base.path())
        .with_part_size(10)
        .push_b("abc123", None, "T", b"tiny")
        .unwrap();
    let pushed = fake.pushed.borrow().clone().unwrap();
    assert_eq!(pushed, [("data.000".to_string(), b"tiny".to_vec())]);
    let calls = fake.calls();
    let adds: Vec<_> = calls.iter().filter(|c| c.starts_with("git add")).collect();
    assert_eq!(
        adds,
        ["git add --all -- data.000 data.001 data.002 data.003 manifest"]
    );
}

#[test]
fn the_source_is_not_called_on_a_stale_expected() {
    let fake = with_manifest("CUR");
    let base = tempfile::tempdir().unwrap();
    let calls = std::cell::Cell::new(0);
    let source = |_: &mut dyn std::io::Write| {
        calls.set(calls.get() + 1);
        Ok(())
    };
    let got = backend(&fake, base.path()).push("abc123", Some("old"), "N", &source);
    assert_eq!(got, Err(BackendError::Changed(Some("CUR".into()))));
    assert_eq!(calls.get(), 0);
    assert!(fake.pushed.borrow().is_none());
}

#[test]
fn a_source_error_commits_and_pushes_nothing_and_cleans_up() {
    let fake = Fake::default();
    let base = tempfile::tempdir().unwrap();
    let broken = |w: &mut dyn std::io::Write| {
        w.write_all(b"half")?;
        Err(std::io::Error::other("file vanished"))
    };
    let got = backend(&fake, base.path()).push("abc123", None, "N", &broken);
    assert!(
        matches!(&got, Err(e) if e.to_string().contains("file vanished")),
        "{got:?}"
    );
    let calls = fake.calls();
    assert_eq!(calls.len(), 1, "only the clone: {calls:?}");
    assert!(fake.pushed.borrow().is_none());
    assert_eq!(std::fs::read_dir(base.path()).unwrap().count(), 0);
}

#[test]
fn an_empty_blob_pushes_data_000() {
    let fake = Fake::default();
    let base = tempfile::tempdir().unwrap();
    backend(&fake, base.path())
        .push("abc123", None, "N", &source_of(b""))
        .unwrap();
    assert_eq!(
        fake.pushed.borrow().clone().unwrap(),
        [("data.000".to_string(), Vec::new())]
    );
}

#[test]
fn a_growing_push_overwrites_and_adds_parts() {
    let fake = Fake {
        remote: remote_parts(1),
        ..Fake::default()
    };
    let base = tempfile::tempdir().unwrap();
    backend(&fake, base.path())
        .with_part_size(4)
        .push("abc123", None, "N", &source_of(b"0123456789"))
        .unwrap();
    let pushed = fake.pushed.borrow().clone().unwrap();
    let sizes: Vec<_> = pushed.iter().map(|(n, b)| (n.as_str(), b.len())).collect();
    assert_eq!(sizes, [("data.000", 4), ("data.001", 4), ("data.002", 2)]);
}
