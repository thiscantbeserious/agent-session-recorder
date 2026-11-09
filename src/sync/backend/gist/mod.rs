//! Gist backend, `gh` for create, list and delete, `git` for the binary blob.

pub(super) mod pace;
pub(super) mod parse;
pub(super) mod parts;
mod progress;
pub(super) mod runner;
mod scratch;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use self::pace::{Clock, Pacer, SystemClock};
use self::parse::{check_auth, id_from_create_output, map_failure, parse_list, DESC_PREFIX};
use self::parts::{check_sequence, part_names, PartReader, PartWriter, PART_SIZE};
use self::progress::{receiving_percent, writing_percent};
use self::runner::{Output, ProcessRunner, Runner};
use self::scratch::Scratch;
use crate::sync::backend::{BackendError, BlobSource, RemoteEntry, SyncBackend};

/// Neutral dotfile, only there because `gh gist create` refuses an empty gist.
const STUB_FILE: &str = ".keep";
/// gh refuses a file that is blank after trimming, so the stub carries a fixed word, no metadata.
const STUB_BODY: &str = "agr\n";
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

/// Receives the percent of the running upload or download, never a path or a name.
pub type ProgressSink = Arc<dyn Fn(u8) + Send + Sync>;

/// Secret gists through `gh` and `git`, one gist per session.
pub struct GistBackend<R: Runner = ProcessRunner> {
    /// Set by the app layer, `None` means nobody listens.
    progress: Option<ProgressSink>,
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
        Self::with_runner(ProcessRunner::default(), std::env::temp_dir())
    }
}

impl Default for GistBackend<ProcessRunner> {
    fn default() -> Self {
        Self::new()
    }
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(ToString::to_string).collect()
}

fn io_result(tool: &'static str, result: std::io::Result<Output>) -> Result<Output, BackendError> {
    match result {
        Ok(out) => Ok(out),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(BackendError::NotInstalled(tool)),
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Err(BackendError::Other(format!(
            "{tool} stalled, nothing came back in time, retry with `agr sync push`"
        ))),
        Err(e) => Err(BackendError::Other(format!("{tool}: {}", e.kind()))),
    }
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
            progress: None,
            runner,
            temp_base,
            part_size: PART_SIZE,
            pacer: Mutex::new(Pacer::new(clock.clone())),
            clock,
        }
    }

    /// Report the percent of `git push` and `git clone` progress to `sink`.
    pub fn with_progress(mut self, sink: ProgressSink) -> Self {
        self.progress = Some(sink);
        self
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
        io_result(tool, self.runner.run(tool, &strings(args), cwd, env))
    }

    /// Like `run_ok`, the percent that `parse` finds in the stderr lines goes to the progress
    /// sink, when there is one.
    fn run_progress(
        &self,
        args: &[&str],
        cwd: Option<&Path>,
        parse: fn(&str) -> Option<u8>,
    ) -> Result<Output, BackendError> {
        let Some(sink) = &self.progress else {
            return self.run_ok("git", args, cwd);
        };
        let mut on_line = |line: &str| parse(line).iter().for_each(|p| sink(*p));
        let out = self
            .runner
            .run_watched("git", &strings(args), cwd, &[], &mut on_line);
        let out = io_result("git", out)?;
        if out.success() {
            return Ok(out);
        }
        Err(map_failure("git", &out.stderr, out.code))
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

    fn clone_gist(
        &self,
        id: &str,
        scratch: &Scratch,
        parse: fn(&str) -> Option<u8>,
    ) -> Result<PathBuf, BackendError> {
        let dir = scratch.join("clone");
        let target = dir.to_string_lossy().into_owned();
        // HTTPS with gh as the credential helper, never SSH: `gh gist clone` follows the
        // user's git_protocol and an unknown host key would prompt and stall the run.
        let url = format!("https://gist.github.com/{id}.git");
        self.run_progress(
            &[
                "-c",
                "credential.helper=",
                "-c",
                "credential.helper=!gh auth git-credential",
                "clone",
                "--depth",
                "1",
                "--progress",
                &url,
                &target,
            ],
            None,
            parse,
        )?;
        Ok(dir)
    }

    /// Stream the blob into the parts, returns every touched file name.
    fn stage_parts(
        &self,
        repo: &Path,
        manifest: &str,
        blob: BlobSource,
    ) -> Result<Vec<String>, BackendError> {
        let mut writer = PartWriter::new(repo, self.part_size);
        blob(&mut writer).map_err(|e| BackendError::Other(format!("write part: {e}")))?;
        let mut names = writer
            .finish()
            .map_err(|e| BackendError::Other(format!("write part: {e}")))?;
        std::fs::write(repo.join(MANIFEST_FILE), manifest)
            .map_err(|e| BackendError::Other(format!("write manifest: {e}")))?;
        names.push(MANIFEST_FILE.to_string());
        Ok(names)
    }

    fn commit_and_push(
        &self,
        repo: &Path,
        manifest: &str,
        blob: BlobSource,
    ) -> Result<(), BackendError> {
        let names = self.stage_parts(repo, manifest, blob)?;
        let mut add = vec!["add", "--all", "--"];
        add.extend(names.iter().map(String::as_str));
        self.run_ok("git", &add, Some(repo))?;
        // A user's global hooks must not run in the scratch clone, point them at nothing.
        let no_hooks = format!(
            "core.hooksPath={}",
            repo.with_file_name("no-hooks").display()
        );
        let mut commit = GIT_IDENTITY.to_vec();
        commit.extend(["-c", &no_hooks, "commit", "-m", "agr sync"]);
        let date = format!("@{} +0000", self.clock.unix_secs()?);
        let mut env = vec![
            ("GIT_AUTHOR_DATE", date.clone()),
            ("GIT_COMMITTER_DATE", date),
        ];
        env.extend(IDENTITY_ENV.iter().map(|(k, v)| (*k, v.to_string())));
        self.run_ok_env("git", &commit, Some(repo), &env)?;
        // `--progress` keeps stderr talking off a terminal, the runner's stall timer needs it.
        // Plain push, never forced and never retried, a rejection is `Changed(None)`.
        self.pace();
        self.run_progress(
            &[
                "-c",
                "credential.helper=",
                "-c",
                "credential.helper=!gh auth git-credential",
                "push",
                "--no-verify",
                "--progress",
            ],
            Some(repo),
            writing_percent,
        )?;
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
        blob: BlobSource,
    ) -> Result<(), BackendError> {
        let scratch = Scratch::create(&self.temp_base)?;
        let repo = self.clone_gist(id, &scratch, |_| None)?;
        let current = read_manifest(&repo)?;
        if current.as_deref() != expected {
            return Err(BackendError::Changed(current));
        }
        self.commit_and_push(&repo, manifest, blob)
    }

    fn describe(&self, id: &str, token: &str) -> Result<(), BackendError> {
        self.set_description(id, token)
    }

    /// Manifest and parts come from one clone, so they belong together. The reader owns the
    /// clone and removes it when dropped.
    fn pull(&self, id: &str) -> Result<(Option<String>, Box<dyn Read>), BackendError> {
        let scratch = Scratch::create(&self.temp_base)?;
        let repo = self.clone_gist(id, &scratch, receiving_percent)?;
        let manifest = read_manifest(&repo)?;
        let names =
            part_names(&repo).map_err(|e| BackendError::Other(format!("read clone: {e}")))?;
        check_sequence(&names).map_err(|e| BackendError::Other(format!("gist {id} {e}")))?;
        let files = names.iter().map(|n| repo.join(n)).collect();
        Ok((
            manifest,
            Box::new(PartReader::new(files, Box::new(scratch))),
        ))
    }

    fn delete(&self, id: &str) -> Result<(), BackendError> {
        self.pace();
        self.run_ok("gh", &["gist", "delete", id, "--yes"], None)?;
        Ok(())
    }
}

#[cfg(test)]
mod fake;
#[cfg(test)]
mod pull_tests;
#[cfg(test)]
mod push_tests;
