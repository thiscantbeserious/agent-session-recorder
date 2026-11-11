//! Gist backend behaviour against a fake `gh` and `git` runner.

use crate::helpers::gist_fake::{backend, fail, ok, scrub, Bytes, Fake};
use agr::sync::testing::gist::{parse_list, FakeClock, NO_CREATED_AT, STUB_BODY};
use agr::sync::{BackendError, RemoteEntry, SyncBackend};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn check_runs_the_exact_probes_and_reads_the_scope_line() {
    let fake = Fake::with(vec![
        ok("git version 2.50"),
        ok("gh version 2.96"),
        ok("  - Token scopes: 'gist', 'repo'\n"),
    ]);
    let base = tempfile::tempdir().unwrap();
    assert_eq!(backend(&fake, base.path()).check(), Ok(()));
    assert_eq!(
        fake.calls(),
        ["git --version", "gh --version", "gh auth status"]
    );
}

#[test]
fn check_maps_missing_tool_login_and_scope() {
    let base = tempfile::tempdir().unwrap();
    let missing = Fake::with(vec![Err(std::io::ErrorKind::NotFound.into())]);
    assert_eq!(
        backend(&missing, base.path()).check(),
        Err(BackendError::NotInstalled("git"))
    );
    let no_gh = Fake::with(vec![ok(""), Err(std::io::ErrorKind::NotFound.into())]);
    assert_eq!(
        backend(&no_gh, base.path()).check(),
        Err(BackendError::NotInstalled("gh"))
    );
    let out = Fake::with(vec![
        ok(""),
        ok(""),
        fail("You are not logged into any GitHub hosts"),
    ]);
    assert_eq!(
        backend(&out, base.path()).check(),
        Err(BackendError::NotLoggedIn)
    );
    let scope = Fake::with(vec![ok(""), ok(""), ok("  - Token scopes: 'repo'")]);
    assert_eq!(
        backend(&scope, base.path()).check(),
        Err(BackendError::MissingScope)
    );
}

#[test]
fn list_calls_gh_api_with_pagination_and_parses() {
    let fake = Fake::with(vec![ok(r#"[{"id":"a1","description":"agr:v1 T"}]"#)]);
    let base = tempfile::tempdir().unwrap();
    let got = backend(&fake, base.path()).list().unwrap();
    assert_eq!(fake.calls(), ["gh api gists?per_page=100 --paginate"]);
    assert_eq!(
        got,
        vec![RemoteEntry {
            id: "a1".into(),
            meta_token: "T".into(),
            created_at: NO_CREATED_AT.into(),
        }]
    );
}

#[test]
fn the_stub_is_not_blank_as_gh_requires() {
    // gh rejects files whose content is empty after strings.TrimSpace ("a gist file
    // cannot be blank", cli/cli pkg/cmd/gist/create/create.go detectEmptyFiles).
    assert!(
        !STUB_BODY.trim().is_empty(),
        "stub {STUB_BODY:?} is blank to gh"
    );
}

#[test]
fn create_makes_a_pending_stub_gist_and_nothing_else() {
    let fake = Fake::with(vec![ok("https://gist.github.com/me/abc123\n")]);
    let base = tempfile::tempdir().unwrap();
    let id = backend(&fake, base.path()).create().unwrap();
    assert_eq!(id, "abc123");
    assert_eq!(fake.stub_seen.borrow().as_deref(), Some(STUB_BODY));
    assert_eq!(
        scrub(&fake.calls(), base.path()),
        ["gh gist create <tmp>/agr-sync-N/.keep --desc agr:pending"]
    );
}

#[test]
fn every_mutating_call_waits_one_second_on_the_pacer() {
    let clock = Arc::new(FakeClock::new(Duration::from_secs(10), 1_700_000_000));
    let fake = Fake {
        clock: Some(clock.clone()),
        ..Fake::with(vec![ok("https://gist.github.com/me/abc123")])
    };
    let base = tempfile::tempdir().unwrap();
    let b = backend(&fake, base.path()).with_clock(clock.clone());
    b.create().unwrap();
    b.push_b("abc123", None, "M", b"x").unwrap();
    b.describe("abc123", "T").unwrap();
    b.delete("abc123").unwrap();
    let mutating = |c: &str| {
        c.starts_with("gh gist create")
            || c == "git -c credential.helper= -c credential.helper=!gh auth git-credential push --no-verify --progress"
            || c.contains("PATCH")
            || c.contains("delete")
    };
    let stamps = fake.stamps.borrow();
    let times: Vec<_> = stamps.iter().filter(|(c, _)| mutating(c)).collect();
    assert_eq!(times.len(), 4, "{stamps:?}");
    for pair in times.windows(2) {
        let gap = pair[1].1 - pair[0].1;
        assert!(gap >= Duration::from_secs(1), "{pair:?}");
    }
    assert_eq!(clock.slept().len(), 3);
}

#[test]
fn describe_patches_the_description_with_the_token() {
    let fake = Fake::default();
    let base = tempfile::tempdir().unwrap();
    backend(&fake, base.path())
        .describe("abc123", "NEW")
        .unwrap();
    assert_eq!(
        fake.calls(),
        ["gh api -X PATCH gists/abc123 -f description=agr:v1 NEW"]
    );
}

#[test]
fn a_pending_gist_is_not_listed_and_no_call_carries_the_token() {
    let fake = Fake::with(vec![ok("https://gist.github.com/me/abc123")]);
    let base = tempfile::tempdir().unwrap();
    backend(&fake, base.path()).create().unwrap();
    let calls = fake.calls();
    let desc = calls[0].split(" --desc ").nth(1).unwrap().to_string();
    let listing = format!(r#"[{{"id":"abc123","description":"{desc}"}}]"#);
    assert_eq!(parse_list(&listing).unwrap(), vec![]);
}

#[cfg(unix)]
#[test]
fn scratch_dir_is_private() {
    use agr::sync::testing::gist::Scratch;
    use std::os::unix::fs::PermissionsExt;
    let base = tempfile::tempdir().unwrap();
    let scratch = Scratch::create(base.path()).unwrap();
    let mode = std::fs::metadata(&scratch.0).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[cfg(unix)]
#[test]
fn scratch_at_is_private() {
    use agr::sync::testing::gist::Scratch;
    use std::os::unix::fs::PermissionsExt;
    let base = tempfile::tempdir().unwrap();
    let scratch = Scratch::at(&base.path().join("one")).unwrap();
    let mode = std::fs::metadata(&scratch.0).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[test]
fn scratch_at_refuses_an_existing_dir() {
    use agr::sync::testing::gist::Scratch;
    let base = tempfile::tempdir().unwrap();
    let result = Scratch::at(base.path());
    assert!(result.is_err());
    assert!(base.path().exists(), "the refused dir is not removed");
}

#[test]
fn delete_uses_yes_flag() {
    let fake = Fake::default();
    let base = tempfile::tempdir().unwrap();
    backend(&fake, base.path()).delete("abc123").unwrap();
    assert_eq!(fake.calls(), ["gh gist delete abc123 --yes"]);
}

#[test]
fn nonzero_exit_maps_through_stderr_samples() {
    let fake = Fake::with(vec![fail("could not resolve host: api.github.com")]);
    let base = tempfile::tempdir().unwrap();
    let err = backend(&fake, base.path()).list().unwrap_err();
    assert!(matches!(err, BackendError::Unreachable(_)));
}

#[test]
fn a_stalled_call_says_so_and_names_the_retry() {
    let fake = Fake::with(vec![Err(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "gh printed nothing",
    ))]);
    let base = tempfile::tempdir().unwrap();
    let err = backend(&fake, base.path()).list().unwrap_err();
    let text = err.to_string();
    assert!(
        text.contains("stalled") && text.contains("agr sync push"),
        "{text}"
    );
}
