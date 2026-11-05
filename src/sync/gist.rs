//! Gist backend, `gh` for create, list and delete, `git` for the binary blob.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::backend::{BackendError, RemoteEntry, SyncBackend};
use super::gist_parse::{check_auth, id_from_create_output, map_failure, parse_list, DESC_PREFIX};
use super::runner::{Output, ProcessRunner, Runner};

const STUB_FILE: &str = "agr-session.json";
const STUB_BODY: &str = "{\"format\":1}\n";
const BLOB_FILE: &str = "session.agr";
const LIST_ENDPOINT: &str = "gists?per_page=100";
const GIT_IDENTITY: [&str; 4] = ["-c", "user.name=agr", "-c", "user.email=agr@localhost"];

static CLONE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Secret gists through `gh` and `git`, one gist per session.
pub struct GistBackend<R: Runner = ProcessRunner> {
    runner: R,
    temp_base: PathBuf,
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
        std::fs::create_dir(&dir).map_err(|e| BackendError::Other(format!("temp dir: {e}")))?;
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

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(ToString::to_string).collect()
}

impl<R: Runner> GistBackend<R> {
    pub fn with_runner(runner: R, temp_base: PathBuf) -> Self {
        Self { runner, temp_base }
    }

    fn run(
        &self,
        tool: &'static str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> Result<Output, BackendError> {
        match self.runner.run(tool, &strings(args), cwd) {
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
        let out = self.run(tool, args, cwd)?;
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

    fn commit_and_push(&self, repo: &Path, blob: &[u8]) -> Result<(), BackendError> {
        std::fs::write(repo.join(BLOB_FILE), blob)
            .map_err(|e| BackendError::Other(format!("write blob: {e}")))?;
        self.run_ok("git", &["add", BLOB_FILE], Some(repo))?;
        let mut commit = GIT_IDENTITY.to_vec();
        commit.extend(["commit", "-m", "agr sync"]);
        self.run_ok("git", &commit, Some(repo))?;
        self.run_ok("git", &["push"], Some(repo))?;
        Ok(())
    }

    fn create_gist(&self, token: &str) -> Result<String, BackendError> {
        let scratch = Scratch::create(&self.temp_base)?;
        let stub = scratch.join(STUB_FILE);
        std::fs::write(&stub, STUB_BODY)
            .map_err(|e| BackendError::Other(format!("write stub: {e}")))?;
        let desc = format!("{DESC_PREFIX}{token}");
        let stub = stub.to_string_lossy().into_owned();
        let out = self.run_ok("gh", &["gist", "create", &stub, "--desc", &desc], None)?;
        id_from_create_output(&out.stdout)
            .ok_or_else(|| BackendError::Other("gh did not print a gist url".into()))
    }

    fn set_description(&self, id: &str, token: &str) -> Result<(), BackendError> {
        let desc = format!("description={DESC_PREFIX}{token}");
        let endpoint = format!("gists/{id}");
        self.run_ok("gh", &["api", "-X", "PATCH", &endpoint, "-f", &desc], None)?;
        Ok(())
    }
}

impl<R: Runner> SyncBackend for GistBackend<R> {
    fn name(&self) -> &'static str {
        "gist"
    }

    fn check(&self) -> Result<(), BackendError> {
        self.run("git", &["--version"], None)?;
        self.run("gh", &["--version"], None)?;
        let out = self.run("gh", &["auth", "status"], None)?;
        if !out.success() {
            return Err(BackendError::NotLoggedIn);
        }
        check_auth(&format!("{}\n{}", out.stdout, out.stderr))
    }

    fn list(&self) -> Result<Vec<RemoteEntry>, BackendError> {
        let out = self.run_ok("gh", &["api", LIST_ENDPOINT, "--paginate"], None)?;
        parse_list(&out.stdout)
    }

    fn push(
        &self,
        id: Option<&str>,
        meta_token: &str,
        blob: &[u8],
    ) -> Result<String, BackendError> {
        let created = id.is_none();
        let id = match id {
            Some(id) => id.to_string(),
            None => self.create_gist(meta_token)?,
        };
        let scratch = Scratch::create(&self.temp_base)?;
        let sent = self
            .clone_gist(&id, &scratch)
            .and_then(|repo| self.commit_and_push(&repo, blob));
        match (sent, created) {
            (Ok(()), true) => Ok(id),
            (Ok(()), false) => {
                // Blob first, so a crash in between leaves a hash mismatch that pull rejects.
                self.set_description(&id, meta_token)?;
                Ok(id)
            }
            (Err(e), true) => {
                let _ = self.delete(&id);
                Err(e)
            }
            (Err(e), false) => Err(e),
        }
    }

    fn pull(&self, id: &str) -> Result<Vec<u8>, BackendError> {
        let scratch = Scratch::create(&self.temp_base)?;
        let repo = self.clone_gist(id, &scratch)?;
        std::fs::read(repo.join(BLOB_FILE))
            .map_err(|_| BackendError::Other(format!("gist {id} has no {BLOB_FILE}")))
    }

    fn delete(&self, id: &str) -> Result<(), BackendError> {
        self.run_ok("gh", &["gist", "delete", id, "--yes"], None)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    #[derive(Default)]
    struct Fake {
        calls: RefCell<Vec<String>>,
        replies: RefCell<VecDeque<std::io::Result<Output>>>,
        pushed_blob: RefCell<Option<Vec<u8>>>,
        stub_seen: RefCell<Option<String>>,
        clone_dirs: RefCell<Vec<PathBuf>>,
        remote_blob: Option<Vec<u8>>,
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
                if let Some(blob) = &self.remote_blob {
                    std::fs::write(dir.join(BLOB_FILE), blob).unwrap();
                }
                self.clone_dirs.borrow_mut().push(dir);
            }
            if program == "git" && args.last().map(String::as_str) == Some("push") {
                let blob = std::fs::read(cwd.unwrap().join(BLOB_FILE)).unwrap();
                *self.pushed_blob.borrow_mut() = Some(blob);
            }
        }
    }

    impl Runner for &Fake {
        fn run(
            &self,
            program: &str,
            args: &[String],
            cwd: Option<&Path>,
        ) -> std::io::Result<Output> {
            self.calls
                .borrow_mut()
                .push(format!("{program} {}", args.join(" ")));
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
                meta_token: "T".into()
            }]
        );
    }

    #[test]
    fn push_new_creates_stub_gist_then_clones_commits_pushes() {
        let fake = Fake::with(vec![ok("https://gist.github.com/me/abc123\n")]);
        let base = tempfile::tempdir().unwrap();
        let id = backend(&fake, base.path())
            .push(None, "TOKEN", b"sealed")
            .unwrap();
        assert_eq!(id, "abc123");
        assert_eq!(fake.stub_seen.borrow().as_deref(), Some("{\"format\":1}\n"));
        let calls = scrub(&fake.calls(), base.path());
        assert_eq!(
            calls,
            [
                "gh gist create <tmp>/agr-sync-N/agr-session.json --desc agr:v1 TOKEN",
                "gh gist clone abc123 <tmp>/agr-sync-N/clone -- --depth 1",
                "git add session.agr",
                "git -c user.name=agr -c user.email=agr@localhost commit -m agr sync",
                "git push",
            ]
        );
        assert_eq!(fake.pushed_blob.borrow().as_deref(), Some(&b"sealed"[..]));
    }

    #[test]
    fn push_existing_clones_replaces_and_updates_the_description() {
        let fake = Fake::default();
        let base = tempfile::tempdir().unwrap();
        let id = backend(&fake, base.path())
            .push(Some("abc123"), "NEW", b"v2")
            .unwrap();
        assert_eq!(id, "abc123");
        let calls = scrub(&fake.calls(), base.path());
        assert_eq!(
            calls[0],
            "gh gist clone abc123 <tmp>/agr-sync-N/clone -- --depth 1"
        );
        assert_eq!(
            calls.last().unwrap(),
            "gh api -X PATCH gists/abc123 -f description=agr:v1 NEW"
        );
        assert!(!calls.iter().any(|c| c.contains("gist create")));
        assert_eq!(fake.pushed_blob.borrow().as_deref(), Some(&b"v2"[..]));
    }

    #[test]
    fn temp_dirs_are_removed_on_success_and_on_error() {
        let base = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        backend(&fake, base.path())
            .push(Some("abc123"), "T", b"x")
            .unwrap();
        let failing = Fake::with(vec![ok(""), fail("boom")]);
        assert!(backend(&failing, base.path())
            .push(Some("abc123"), "T", b"x")
            .is_err());
        assert_eq!(std::fs::read_dir(base.path()).unwrap().count(), 0);
    }

    #[test]
    fn failed_push_of_a_new_gist_deletes_the_empty_gist() {
        let fake = Fake::with(vec![
            ok("https://gist.github.com/me/abc123"),
            ok(""),
            ok(""),
            ok(""),
            fail("fatal: Authentication failed"),
        ]);
        let base = tempfile::tempdir().unwrap();
        let err = backend(&fake, base.path())
            .push(None, "T", b"x")
            .unwrap_err();
        assert!(err.to_string().contains("gh auth setup-git"), "{err}");
        assert_eq!(fake.calls().last().unwrap(), "gh gist delete abc123 --yes");
    }

    #[test]
    fn pull_clones_and_reads_the_blob() {
        let fake = Fake {
            remote_blob: Some(vec![0, 159, 146, 150]),
            ..Fake::default()
        };
        let base = tempfile::tempdir().unwrap();
        let got = backend(&fake, base.path()).pull("abc123").unwrap();
        assert_eq!(got, vec![0, 159, 146, 150]);
        let calls = scrub(&fake.calls(), base.path());
        assert_eq!(
            calls,
            ["gh gist clone abc123 <tmp>/agr-sync-N/clone -- --depth 1"]
        );
        assert_eq!(std::fs::read_dir(base.path()).unwrap().count(), 0);
    }

    #[test]
    fn pull_without_blob_file_errors() {
        let fake = Fake::default();
        let base = tempfile::tempdir().unwrap();
        assert!(matches!(
            backend(&fake, base.path()).pull("abc123"),
            Err(BackendError::Other(_))
        ));
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
