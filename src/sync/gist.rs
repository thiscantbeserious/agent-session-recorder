//! Gist backend, `gh` for create, list and delete, `git` for the binary blob.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use super::backend::{BackendError, RemoteEntry, SyncBackend};
use super::gist_parse::{check_auth, id_from_create_output, map_failure, parse_list, DESC_PREFIX};
use super::pace::{Clock, Pacer, SystemClock};
use super::parts::{check_sequence, join, part_name, split, stale, PART_PREFIX, PART_SIZE};
use super::runner::{Output, ProcessRunner, Runner};

/// Neutral dotfile, only there because `gh gist create` refuses an empty gist.
const STUB_FILE: &str = ".keep";
const STUB_BODY: &str = "\n";
/// Holds the sealed manifest token the compare-and-swap checks.
const MANIFEST_FILE: &str = "manifest";
/// Placeholder until the blob is pushed, no `agr:v1 ` prefix so `parse_list` skips it.
const PENDING_DESC: &str = "agr:pending";
const LIST_ENDPOINT: &str = "gists?per_page=100";
const GIT_IDENTITY: [&str; 6] = [
    "-c",
    "user.name=agr",
    "-c",
    "user.email=agr@localhost",
    "-c",
    "commit.gpgsign=false",
];
/// Env beats `-c user.*`, so a user's exported `GIT_AUTHOR_*` would leak without these.
const IDENTITY_ENV: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", "agr"),
    ("GIT_AUTHOR_EMAIL", "agr@localhost"),
    ("GIT_COMMITTER_NAME", "agr"),
    ("GIT_COMMITTER_EMAIL", "agr@localhost"),
];

static CLONE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Secret gists through `gh` and `git`, one gist per session.
pub struct GistBackend<R: Runner = ProcessRunner> {
    runner: R,
    temp_base: PathBuf,
    part_size: usize,
    clock: Arc<dyn Clock>,
    /// Behind a mutex so `&self` trait methods can pace, boxed clock so no generic leaks out.
    /// `wait` runs before each mutating call.
    pacer: Mutex<Pacer>,
}

impl GistBackend<ProcessRunner> {
    pub fn new() -> Self {
        Self::with_runner(ProcessRunner, std::env::temp_dir())
    }
}

impl Default for GistBackend<ProcessRunner> {
    fn default() -> Self {
        Self::new()
    }
}

/// Scratch directory under the temp dir, removed on drop (also on error paths).
struct Scratch(PathBuf);

impl Scratch {
    fn create(base: &Path) -> Result<Self, BackendError> {
        let n = CLONE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = base.join(format!("agr-sync-{}-{n}", std::process::id()));
        let builder = private_dir_builder();
        builder
            .create(&dir)
            .map_err(|e| BackendError::Other(format!("temp dir: {e}")))?;
        Ok(Self(dir))
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn private_dir_builder() -> std::fs::DirBuilder {
    #[allow(unused_mut)]
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(ToString::to_string).collect()
}

/// Sorted names of the `data.*` files in a clone.
fn part_files(repo: &Path) -> Result<Vec<String>, BackendError> {
    let dir =
        std::fs::read_dir(repo).map_err(|e| BackendError::Other(format!("read clone: {e}")))?;
    let mut names: Vec<String> = dir
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with(PART_PREFIX))
        .collect();
    names.sort();
    Ok(names)
}

/// The stored manifest of a clone, `None` when the gist has none yet.
fn read_manifest(repo: &Path) -> Result<Option<String>, BackendError> {
    match std::fs::read_to_string(repo.join(MANIFEST_FILE)) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(BackendError::Other(format!("read manifest: {e}"))),
    }
}

impl<R: Runner> GistBackend<R> {
    pub fn with_runner(runner: R, temp_base: PathBuf) -> Self {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock::new());
        Self {
            runner,
            temp_base,
            part_size: PART_SIZE,
            pacer: Mutex::new(Pacer::new(clock.clone())),
            clock,
        }
    }

    /// Replace the clock, pacing and commit dates both follow it.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.pacer = Mutex::new(Pacer::new(clock.clone()));
        self.clock = clock;
        self
    }

    /// Smaller parts, for tests that must not build a 45 MiB blob.
    pub fn with_part_size(mut self, part_size: usize) -> Self {
        self.part_size = part_size;
        self
    }

    /// Block until the next mutating call is allowed.
    fn pace(&self) {
        self.pacer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .wait();
    }

    fn run(
        &self,
        tool: &'static str,
        args: &[&str],
        cwd: Option<&Path>,
        env: &[(&str, String)],
    ) -> Result<Output, BackendError> {
        match self.runner.run(tool, &strings(args), cwd, env) {
            Ok(out) => Ok(out),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(BackendError::NotInstalled(tool))
            }
            Err(e) => Err(BackendError::Other(format!("{tool}: {}", e.kind()))),
        }
    }

    /// Like `run` but a non-zero exit becomes a mapped error.
    fn run_ok(
        &self,
        tool: &'static str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> Result<Output, BackendError> {
        self.run_ok_env(tool, args, cwd, &[])
    }

    fn run_ok_env(
        &self,
        tool: &'static str,
        args: &[&str],
        cwd: Option<&Path>,
        env: &[(&str, String)],
    ) -> Result<Output, BackendError> {
        let out = self.run(tool, args, cwd, env)?;
        if out.success() {
            return Ok(out);
        }
        Err(map_failure(tool, &out.stderr, out.code))
    }

    fn clone_gist(&self, id: &str, scratch: &Scratch) -> Result<PathBuf, BackendError> {
        let dir = scratch.join("clone");
        let target = dir.to_string_lossy().into_owned();
        self.run_ok(
            "gh",
            &["gist", "clone", id, &target, "--", "--depth", "1"],
            None,
        )?;
        Ok(dir)
    }

    /// Write the parts and drop stale higher ones, returns every touched file name.
    fn stage_parts(
        &self,
        repo: &Path,
        manifest: &str,
        blob: &[u8],
    ) -> Result<Vec<String>, BackendError> {
        let parts = split(blob, self.part_size);
        let mut names = Vec::new();
        for (i, part) in parts.iter().enumerate() {
            let name = part_name(i);
            std::fs::write(repo.join(&name), part)
                .map_err(|e| BackendError::Other(format!("write part: {e}")))?;
            names.push(name);
        }
        for old in stale(&part_files(repo)?, parts.len()) {
            std::fs::remove_file(repo.join(&old))
                .map_err(|e| BackendError::Other(format!("remove stale part: {e}")))?;
            names.push(old);
        }
        std::fs::write(repo.join(MANIFEST_FILE), manifest)
            .map_err(|e| BackendError::Other(format!("write manifest: {e}")))?;
        names.push(MANIFEST_FILE.to_string());
        Ok(names)
    }

    fn commit_and_push(
        &self,
        repo: &Path,
        manifest: &str,
        blob: &[u8],
    ) -> Result<(), BackendError> {
        let names = self.stage_parts(repo, manifest, blob)?;
        let mut add = vec!["add", "--all", "--"];
        add.extend(names.iter().map(String::as_str));
        self.run_ok("git", &add, Some(repo))?;
        let mut commit = GIT_IDENTITY.to_vec();
        commit.extend(["commit", "-m", "agr sync"]);
        let date = format!("@{} +0000", self.clock.unix_secs()?);
        let mut env = vec![
            ("GIT_AUTHOR_DATE", date.clone()),
            ("GIT_COMMITTER_DATE", date),
        ];
        env.extend(IDENTITY_ENV.iter().map(|(k, v)| (*k, v.to_string())));
        self.run_ok_env("git", &commit, Some(repo), &env)?;
        // Plain push, never forced and never retried, a rejection is `Changed(None)`.
        self.pace();
        self.run_ok("git", &["push"], Some(repo))?;
        Ok(())
    }

    fn create_gist(&self) -> Result<String, BackendError> {
        let scratch = Scratch::create(&self.temp_base)?;
        let stub = scratch.join(STUB_FILE);
        std::fs::write(&stub, STUB_BODY)
            .map_err(|e| BackendError::Other(format!("write stub: {e}")))?;
        let stub = stub.to_string_lossy().into_owned();
        self.pace();
        let out = self.run_ok(
            "gh",
            &["gist", "create", &stub, "--desc", PENDING_DESC],
            None,
        )?;
        id_from_create_output(&out.stdout)
            .ok_or_else(|| BackendError::Other("gh did not print a gist url".into()))
    }

    fn set_description(&self, id: &str, token: &str) -> Result<(), BackendError> {
        let desc = format!("description={DESC_PREFIX}{token}");
        let endpoint = format!("gists/{id}");
        self.pace();
        self.run_ok("gh", &["api", "-X", "PATCH", &endpoint, "-f", &desc], None)?;
        Ok(())
    }
}

impl<R: Runner> SyncBackend for GistBackend<R> {
    fn name(&self) -> &'static str {
        "gist"
    }

    fn check(&self) -> Result<(), BackendError> {
        self.run("git", &["--version"], None, &[])?;
        self.run("gh", &["--version"], None, &[])?;
        let out = self.run("gh", &["auth", "status"], None, &[])?;
        if !out.success() {
            return Err(BackendError::NotLoggedIn);
        }
        check_auth(&format!("{}\n{}", out.stdout, out.stderr))
    }

    fn list(&self) -> Result<Vec<RemoteEntry>, BackendError> {
        let out = self.run_ok("gh", &["api", LIST_ENDPOINT, "--paginate"], None)?;
        parse_list(&out.stdout)
    }

    fn create(&self) -> Result<String, BackendError> {
        self.create_gist()
    }

    /// Clone, compare the stored manifest to `expected` before writing anything, then one
    /// commit and one plain push.
    fn push(
        &self,
        id: &str,
        expected: Option<&str>,
        manifest: &str,
        blob: &[u8],
    ) -> Result<(), BackendError> {
        let scratch = Scratch::create(&self.temp_base)?;
        let repo = self.clone_gist(id, &scratch)?;
        let current = read_manifest(&repo)?;
        if current.as_deref() != expected {
            return Err(BackendError::Changed(current));
        }
        self.commit_and_push(&repo, manifest, blob)
    }

    fn describe(&self, id: &str, token: &str) -> Result<(), BackendError> {
        self.set_description(id, token)
    }

    /// Manifest and parts come from one clone, so they belong together.
    fn pull(&self, id: &str) -> Result<(Option<String>, Vec<u8>), BackendError> {
        let scratch = Scratch::create(&self.temp_base)?;
        let repo = self.clone_gist(id, &scratch)?;
        let manifest = read_manifest(&repo)?;
        let names = part_files(&repo)?;
        check_sequence(&names).map_err(|e| BackendError::Other(format!("gist {id} {e}")))?;
        let mut parts = Vec::new();
        for name in &names {
            parts.push(std::fs::read(repo.join(name)).map_err(|e| {
                BackendError::Other(format!("gist {id} part {name} unreadable: {e}"))
            })?);
        }
        Ok((manifest, join(&parts)))
    }

    fn delete(&self, id: &str) -> Result<(), BackendError> {
        self.pace();
        self.run_ok("gh", &["gist", "delete", id, "--yes"], None)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::pace::tests::FakeClock;
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::time::Duration;

    type Files = Vec<(String, Vec<u8>)>;

    #[derive(Default)]
    struct Fake {
        calls: RefCell<Vec<String>>,
        replies: RefCell<VecDeque<std::io::Result<Output>>>,
        pushed: RefCell<Option<Files>>,
        stub_seen: RefCell<Option<String>>,
        clone_dirs: RefCell<Vec<PathBuf>>,
        envs: RefCell<Vec<Vec<(String, String)>>>,
        pushed_manifest: RefCell<Option<String>>,
        stamps: RefCell<Vec<(String, Duration)>>,
        clock: Option<Arc<FakeClock>>,
        remote: Files,
    }

    /// Every `data.*` file of a clone, sorted by name.
    fn data_files(dir: &Path) -> Files {
        let mut files: Vec<_> = part_files(dir)
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

    fn remote_parts(n: usize) -> Files {
        (0..n).map(|i| (part_name(i), vec![i as u8; 3])).collect()
    }

    fn ok(stdout: &str) -> std::io::Result<Output> {
        Ok(Output {
            code: Some(0),
            stdout: stdout.into(),
            stderr: String::new(),
        })
    }

    fn fail(stderr: &str) -> std::io::Result<Output> {
        Ok(Output {
            code: Some(1),
            stdout: String::new(),
            stderr: stderr.into(),
        })
    }

    impl Fake {
        fn with(replies: Vec<std::io::Result<Output>>) -> Self {
            Self {
                replies: RefCell::new(replies.into()),
                ..Self::default()
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }

        fn side_effects(&self, program: &str, args: &[String], cwd: Option<&Path>) {
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
            if program == "git" && args.last().map(String::as_str) == Some("push") {
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

    fn backend<'a>(fake: &'a Fake, base: &Path) -> GistBackend<&'a Fake> {
        GistBackend::with_runner(fake, base.to_path_buf())
    }

    fn scrub(calls: &[String], base: &Path) -> Vec<String> {
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
                created_at: super::super::gist_parse::NO_CREATED_AT.into(),
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
    fn push_clones_commits_and_pushes_without_touching_the_description() {
        let fake = Fake::default();
        let base = tempfile::tempdir().unwrap();
        backend(&fake, base.path())
            .push("abc123", None, "MANIFEST", b"sealed")
            .unwrap();
        let calls = scrub(&fake.calls(), base.path());
        assert_eq!(
            calls,
            [
                "gh gist clone abc123 <tmp>/agr-sync-N/clone -- --depth 1",
                "git add --all -- data.000 manifest",
                "git -c user.name=agr -c user.email=agr@localhost -c commit.gpgsign=false commit -m agr sync",
                "git push",
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
            let got = backend(&fake, base.path()).push("abc123", expected, "NEW", b"x");
            let want = BackendError::Changed(current.map(str::to_string));
            assert_eq!(got, Err(want));
            let calls = scrub(&fake.calls(), base.path());
            assert_eq!(calls.len(), 1, "{calls:?}");
            assert!(calls[0].starts_with("gh gist clone"), "{calls:?}");
            assert!(fake.pushed.borrow().is_none());
        }
    }

    #[test]
    fn a_matching_expected_pushes_blob_and_manifest() {
        let fake = with_manifest("CUR");
        let base = tempfile::tempdir().unwrap();
        backend(&fake, base.path())
            .push("abc123", Some("CUR"), "NEW", b"blob")
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
            let got = backend(&fake, base.path()).push("abc123", None, "M", b"x");
            assert_eq!(got, Err(BackendError::Changed(None)), "{stderr}");
            let calls = fake.calls();
            let pushes: Vec<_> = calls.iter().filter(|c| c.contains("push")).collect();
            assert_eq!(pushes, ["git push"], "one push, no retry");
            let forced = |a: &str| a == "--force" || a == "-f" || a.starts_with('+');
            assert!(!calls.iter().flat_map(|c| c.split(' ')).any(forced));
        }
    }

    #[test]
    fn the_manifest_is_staged_and_committed_with_the_parts() {
        let fake = Fake::default();
        let base = tempfile::tempdir().unwrap();
        backend(&fake, base.path())
            .push("abc123", None, "M", b"x")
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
        let got = backend(&fake, base.path()).pull("abc123").unwrap();
        assert_eq!(got, (Some("CUR".to_string()), b"blob".to_vec()));
        assert_eq!(fake.calls().len(), 1, "one clone");
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
        b.push("abc123", None, "M", b"x").unwrap();
        b.describe("abc123", "T").unwrap();
        b.delete("abc123").unwrap();
        let mutating = |c: &str| {
            c.starts_with("gh gist create")
                || c == "git push"
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
    fn rejected_git_push_is_changed_none() {
        let fake = Fake::with(vec![
            ok(""),
            ok(""),
            ok(""),
            fail("! [rejected] main -> main (fetch first)"),
        ]);
        let base = tempfile::tempdir().unwrap();
        let got = backend(&fake, base.path()).push("abc123", None, "M", b"x");
        assert_eq!(got, Err(BackendError::Changed(None)));
    }

    #[test]
    fn temp_dirs_are_removed_on_success_and_on_error() {
        let base = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        backend(&fake, base.path())
            .push("abc123", None, "T", b"x")
            .unwrap();
        let failing = Fake::with(vec![ok(""), fail("boom")]);
        assert!(backend(&failing, base.path())
            .push("abc123", None, "T", b"x")
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
            .push("abc123", None, "T", b"x")
            .unwrap_err();
        assert!(err.to_string().contains("gh auth setup-git"), "{err}");
        assert!(!fake.calls().iter().any(|c| c.contains("delete")));
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
        use std::os::unix::fs::PermissionsExt;
        let base = tempfile::tempdir().unwrap();
        let scratch = Scratch::create(base.path()).unwrap();
        let mode = std::fs::metadata(&scratch.0).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[test]
    fn pull_clones_and_reads_the_blob() {
        let fake = Fake {
            remote: vec![(part_name(0), vec![0, 159, 146, 150])],
            ..Fake::default()
        };
        let base = tempfile::tempdir().unwrap();
        let got = backend(&fake, base.path()).pull("abc123").unwrap();
        assert_eq!(got, (None, vec![0, 159, 146, 150]));
        assert_eq!(fake.calls().len(), 1, "one clone, nothing else");
        let calls = scrub(&fake.calls(), base.path());
        assert_eq!(
            calls,
            ["gh gist clone abc123 <tmp>/agr-sync-N/clone -- --depth 1"]
        );
        assert_eq!(std::fs::read_dir(base.path()).unwrap().count(), 0);
    }

    #[test]
    fn pull_without_data_files_errors() {
        let fake = Fake::default();
        let base = tempfile::tempdir().unwrap();
        assert!(matches!(
            backend(&fake, base.path()).pull("abc123"),
            Err(BackendError::Other(_))
        ));
    }

    #[test]
    fn large_blob_is_split_into_parts_in_one_commit() {
        let fake = Fake::default();
        let base = tempfile::tempdir().unwrap();
        let blob: Vec<u8> = (0..25u8).collect();
        backend(&fake, base.path())
            .with_part_size(10)
            .push("abc123", None, "T", &blob)
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
            .push("abc123", None, "T", b"x")
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
            .push("abc123", None, "T", b"tiny")
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
    fn pull_joins_all_parts_in_order() {
        let fake = Fake {
            remote: vec![
                (part_name(1), b"world".to_vec()),
                (part_name(0), b"hello ".to_vec()),
            ],
            ..Fake::default()
        };
        let base = tempfile::tempdir().unwrap();
        let got = backend(&fake, base.path()).pull("abc123").unwrap();
        assert_eq!(got, (None, b"hello world".to_vec()));
    }

    #[test]
    fn pull_with_a_missing_part_errors() {
        let mut remote = remote_parts(3);
        remote.remove(1);
        let fake = Fake {
            remote,
            ..Fake::default()
        };
        let base = tempfile::tempdir().unwrap();
        let err = backend(&fake, base.path()).pull("abc123").unwrap_err();
        assert!(err.to_string().contains("data.001"), "{err}");
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
}
