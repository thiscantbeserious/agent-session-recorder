use super::pace::tests::FakeClock;
use super::pace::Clock;
use super::parts::{part_name, part_names};
use super::{
    parse_list, Arc, BackendError, GistBackend, Output, Path, PathBuf, RemoteEntry, Runner,
    SyncBackend, MANIFEST_FILE, STUB_BODY, STUB_FILE,
};
use crate::sync::backend::{pull_bytes, push_bytes};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::Duration;

type Files = Vec<(String, Vec<u8>)>;

/// Whole-blob shorthands over the streaming trait.
pub(super) trait Bytes: SyncBackend + Sized {
    fn push_b(&self, id: &str, e: Option<&str>, m: &str, b: &[u8]) -> Result<(), BackendError> {
        push_bytes(self, id, e, m, b)
    }
    fn pull_b(&self, id: &str) -> Result<(Option<String>, Vec<u8>), BackendError> {
        pull_bytes(self, id)
    }
}
impl<T: SyncBackend> Bytes for T {}

#[derive(Default)]
pub(super) struct Fake {
    pub(super) calls: RefCell<Vec<String>>,
    pub(super) replies: RefCell<VecDeque<std::io::Result<Output>>>,
    pub(super) pushed: RefCell<Option<Files>>,
    pub(super) stub_seen: RefCell<Option<String>>,
    pub(super) clone_dirs: RefCell<Vec<PathBuf>>,
    pub(super) envs: RefCell<Vec<Vec<(String, String)>>>,
    pub(super) pushed_manifest: RefCell<Option<String>>,
    pub(super) stamps: RefCell<Vec<(String, Duration)>>,
    pub(super) clock: Option<Arc<FakeClock>>,
    pub(super) remote: Files,
}

/// Every `data.*` file of a clone, sorted by name.
pub(super) fn data_files(dir: &Path) -> Files {
    let mut files: Vec<_> = part_names(dir)
        .unwrap()
        .into_iter()
        .map(|n| {
            let body = std::fs::read(dir.join(&n)).unwrap();
            (n, body)
        })
        .collect();
    files.sort();
    files
}

pub(super) fn remote_parts(n: usize) -> Files {
    (0..n).map(|i| (part_name(i), vec![i as u8; 3])).collect()
}

pub(super) fn ok(stdout: &str) -> std::io::Result<Output> {
    Ok(Output {
        code: Some(0),
        stdout: stdout.into(),
        stderr: String::new(),
    })
}

pub(super) fn fail(stderr: &str) -> std::io::Result<Output> {
    Ok(Output {
        code: Some(1),
        stdout: String::new(),
        stderr: stderr.into(),
    })
}

impl Fake {
    pub(super) fn with(replies: Vec<std::io::Result<Output>>) -> Self {
        Self {
            replies: RefCell::new(replies.into()),
            ..Self::default()
        }
    }

    pub(super) fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    pub(super) fn side_effects(&self, program: &str, args: &[String], cwd: Option<&Path>) {
        if program == "gh" && args.get(1).map(String::as_str) == Some("create") {
            let stub = std::fs::read_to_string(&args[2]).unwrap();
            *self.stub_seen.borrow_mut() = Some(stub);
        }
        if program == "gh" && args.get(1).map(String::as_str) == Some("clone") {
            let dir = PathBuf::from(&args[3]);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(STUB_FILE), STUB_BODY).unwrap();
            for (name, body) in &self.remote {
                std::fs::write(dir.join(name), body).unwrap();
            }
            self.clone_dirs.borrow_mut().push(dir);
        }
        if program == "git" && args.iter().any(|a| a == "push") {
            *self.pushed.borrow_mut() = Some(data_files(cwd.unwrap()));
            let manifest = std::fs::read_to_string(cwd.unwrap().join(MANIFEST_FILE)).ok();
            *self.pushed_manifest.borrow_mut() = manifest;
        }
    }
}

impl Runner for &Fake {
    fn run(
        &self,
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
        env: &[(&str, String)],
    ) -> std::io::Result<Output> {
        self.envs.borrow_mut().push(
            env.iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        );
        let call = format!("{program} {}", args.join(" "));
        if let Some(clock) = &self.clock {
            self.stamps.borrow_mut().push((call.clone(), clock.mono()));
        }
        self.calls.borrow_mut().push(call);
        self.side_effects(program, args, cwd);
        self.replies
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| ok(""))
    }
}

pub(super) fn backend<'a>(fake: &'a Fake, base: &Path) -> GistBackend<&'a Fake> {
    GistBackend::with_runner(fake, base.to_path_buf())
}

pub(super) fn scrub(calls: &[String], base: &Path) -> Vec<String> {
    let base = base.to_string_lossy();
    calls
        .iter()
        .map(|c| {
            let c = c.replace(base.as_ref(), "<tmp>");
            let pid = format!("agr-sync-{}-", std::process::id());
            let mut out = String::new();
            for part in c.split(&pid) {
                if out.is_empty() {
                    out.push_str(part);
                } else {
                    out.push_str("agr-sync-N");
                    out.push_str(part.trim_start_matches(|ch: char| ch.is_ascii_digit()));
                }
            }
            out
        })
        .collect()
}

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
            created_at: super::parse::NO_CREATED_AT.into(),
        }]
    );
}

#[test]
fn create_makes_a_pending_stub_gist_and_nothing_else() {
    let fake = Fake::with(vec![ok("https://gist.github.com/me/abc123\n")]);
    let base = tempfile::tempdir().unwrap();
    let id = backend(&fake, base.path()).create().unwrap();
    assert_eq!(id, "abc123");
    assert_eq!(fake.stub_seen.borrow().as_deref(), Some("\n"));
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
            || c == "git push --no-verify"
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
    use super::Scratch;
    use std::os::unix::fs::PermissionsExt;
    let base = tempfile::tempdir().unwrap();
    let scratch = Scratch::create(base.path()).unwrap();
    let mode = std::fs::metadata(&scratch.0).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
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

pub(super) fn source_of(
    bytes: &'static [u8],
) -> impl Fn(&mut dyn std::io::Write) -> std::io::Result<()> {
    move |w| w.write_all(bytes)
}
