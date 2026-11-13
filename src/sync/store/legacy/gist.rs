//! Old per-session gists over `gh` and `git`: listing, one pull (manifest plus parts) and delete.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use super::{parse, LegacyGist, LegacyRead};
use crate::files::scratch::Scratch;
use crate::sync::store::gist::pace::{Clock, Pacer, SystemClock};
use crate::sync::store::gist::parse::map_failure;
use crate::sync::store::gist::runner::{Output, ProcessRunner, Runner};
use crate::sync::store::gist::{io_result, strings};
use crate::sync::store::StoreError;

const LIST_ENDPOINT: &str = "gists?per_page=100";
const MANIFEST_FILE: &str = "manifest";
const PART_PREFIX: &str = "data.";

/// Secret gists of the old layout through `gh` and `git`.
pub struct GistLegacy<R: Runner = ProcessRunner> {
    runner: R,
    temp_base: PathBuf,
    pacer: Mutex<Pacer>,
}

impl GistLegacy<ProcessRunner> {
    pub fn new() -> Self {
        Self::with_runner(ProcessRunner::default(), std::env::temp_dir())
    }
}

impl Default for GistLegacy<ProcessRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: Runner> GistLegacy<R> {
    pub fn with_runner(runner: R, temp_base: PathBuf) -> Self {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock::new());
        Self {
            runner,
            temp_base,
            pacer: Mutex::new(Pacer::new(clock)),
        }
    }

    /// Replace the clock the delete pacing follows.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.pacer = Mutex::new(Pacer::new(clock));
        self
    }

    fn run_ok(
        &self,
        tool: &'static str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> Result<Output, StoreError> {
        let out = io_result(tool, self.runner.run(tool, &strings(args), cwd, &[]))?;
        if out.success() {
            return Ok(out);
        }
        Err(map_failure(tool, &out.stderr, out.code))
    }

    fn clone_gist(&self, id: &str, scratch: &Scratch) -> Result<PathBuf, StoreError> {
        let dir = scratch.join("clone");
        let target = dir.to_string_lossy().into_owned();
        // HTTPS with gh as the credential helper, never SSH, see the gist store.
        let url = format!("https://gist.github.com/{id}.git");
        let args = [
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
        ];
        self.run_ok("git", &args, None)?;
        Ok(dir)
    }
}

/// Sorted `data.*` names of a clone, which must run `data.000` up to `data.N` without a gap.
fn part_names(repo: &Path, id: &str) -> Result<Vec<String>, StoreError> {
    let read =
        std::fs::read_dir(repo).map_err(|e| StoreError::Other(format!("read clone: {e}")))?;
    let mut names: Vec<String> = read
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with(PART_PREFIX))
        .collect();
    names.sort();
    let bad = |why: String| StoreError::Other(format!("gist {id} {why}"));
    if names.is_empty() {
        return Err(bad("has no data parts".into()));
    }
    for (i, name) in names.iter().enumerate() {
        let want = format!("{PART_PREFIX}{i:03}");
        if *name != want {
            return Err(bad(format!("expected {want} but found {name}")));
        }
    }
    Ok(names)
}

impl<R: Runner> LegacyRead for GistLegacy<R> {
    fn list_all(&self) -> Result<Vec<LegacyGist>, StoreError> {
        let out = self.run_ok("gh", &["api", LIST_ENDPOINT, "--paginate"], None)?;
        parse::parse_all(&out.stdout)
    }

    /// Manifest and parts come from one clone, so they belong together. The reader owns the
    /// clone and removes it when dropped.
    fn pull(&self, id: &str) -> Result<(Option<String>, Box<dyn Read>), StoreError> {
        let scratch = crate::sync::store::scratch(&self.temp_base)?;
        let repo = self.clone_gist(id, &scratch)?;
        let manifest = match std::fs::read_to_string(repo.join(MANIFEST_FILE)) {
            Ok(text) => Some(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(StoreError::Other(format!("read manifest: {e}"))),
        };
        let names = part_names(&repo, id)?;
        let files = names.iter().map(|n| repo.join(n)).collect();
        let reader = crate::sync::store::gist::parts::PartReader::new(files, Box::new(scratch));
        Ok((manifest, Box::new(reader)))
    }

    fn delete(&self, id: &str) -> Result<(), StoreError> {
        self.pacer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .wait();
        self.run_ok("gh", &["gist", "delete", id, "--yes"], None)?;
        Ok(())
    }
}
