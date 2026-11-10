//! Append to a pack gist through the cached clone of the own open pack, store side only.
//! The store reports `Moved(current index)` and drops its clone, the one retry is the pack layer's.

use std::io::Write;
use std::path::Path;
use std::sync::PoisonError;

use super::api::{be, INDEX_FILE};
use super::cache::{parse_tree, Cut, PackCache};
use super::index_cache::IndexCache;
use super::layout::{cut_parts, next_names};
use super::{GistStore, Segment, StoreError};
use crate::sync::backend::gist::parse::map_failure;
use crate::sync::backend::gist::progress::writing_percent;
use crate::sync::backend::gist::runner::{Output, Runner};
use crate::sync::backend::gist::{io_result, strings, GIT_IDENTITY, IDENTITY_ENV};
use crate::sync::backend::BackendError;

/// HTTPS with `gh` as the credential helper, never SSH, an unknown host key would prompt and stall.
const CRED: [&str; 4] = [
    "-c",
    "credential.helper=",
    "-c",
    "credential.helper=!gh auth git-credential",
];

/// What one append is asked to do, all of it checked against HEAD of the clone first.
pub(super) struct Request<'a> {
    pub expected_len: u64,
    pub expected_index: &'a str,
    pub segments: &'a [Segment<'a>],
    pub index: &'a str,
}

/// The `index` blob and the data parts at HEAD of the clone.
struct Head {
    index: String,
    len: u64,
    parts: Vec<String>,
}

fn other(e: impl std::fmt::Display) -> StoreError {
    StoreError::Other(e.to_string())
}

impl<R: Runner> GistStore<R> {
    pub(super) fn append_pack(&self, id: &str, req: &Request) -> Result<(), StoreError> {
        let cache = self.pack_cache(id)?;
        let res = self.append_in(&cache, id, req);
        if res.is_err() {
            // Any failure can leave the clone ahead of the remote or dirty, the next append clones again.
            cache.remove();
        }
        if matches!(res, Err(StoreError::Moved(_))) {
            self.cache_file
                .as_deref()
                .map(IndexCache::new)
                .iter()
                .for_each(|c| c.drop_entry(id));
        }
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(id);
        res
    }

    fn append_in(&self, cache: &PackCache, id: &str, req: &Request) -> Result<(), StoreError> {
        let dir = cache.dir();
        self.sync_clone(cache, id)?;
        let head = self.read_head(dir)?;
        if head.len != req.expected_len || head.index != req.expected_index {
            return Err(StoreError::Moved(head.index));
        }
        self.write_parts(dir, &head.parts, req)?;
        self.commit(dir)?;
        self.push(dir, id)
    }

    fn git(
        &self,
        args: &[&str],
        cwd: Option<&Path>,
        env: &[(&str, String)],
    ) -> Result<Output, StoreError> {
        let out = io_result("git", self.runner.run("git", &strings(args), cwd, env)).map_err(be)?;
        match out.success() {
            true => Ok(out),
            false => Err(be(map_failure("git", &out.stderr, out.code))),
        }
    }

    /// The cached clone when it is there and fast-forwards, a fresh clone once otherwise.
    fn sync_clone(&self, cache: &PackCache, id: &str) -> Result<(), StoreError> {
        let dir = cache.dir();
        if dir.exists() && self.fast_forward(dir).is_ok() {
            return Ok(());
        }
        cache.remove();
        cache.prepare()?;
        let (url, target) = (
            format!("https://gist.github.com/{id}.git"),
            dir.to_string_lossy(),
        );
        let mut args = CRED.to_vec();
        args.extend(["clone", "--progress", &url, &target]);
        self.git(&args, None, &[]).map(drop)
    }

    fn fast_forward(&self, dir: &Path) -> Result<(), StoreError> {
        let mut fetch = CRED.to_vec();
        fetch.extend(["fetch", "--progress"]);
        self.git(&fetch, Some(dir), &[])?;
        self.git(&["merge", "--ff-only", "FETCH_HEAD"], Some(dir), &[])
            .map(drop)
    }

    fn read_head(&self, dir: &Path) -> Result<Head, StoreError> {
        let tree = self.git(&["ls-tree", "-l", "HEAD"], Some(dir), &[])?;
        let data = parse_tree(&tree.stdout);
        let index = self
            .git(&["show", &format!("HEAD:{INDEX_FILE}")], Some(dir), &[])?
            .stdout;
        let len = data.iter().map(|(_, size)| size).sum();
        Ok(Head {
            index,
            len,
            parts: data.into_iter().map(|(name, _)| name).collect(),
        })
    }

    /// New parts behind the last one, then the new index. Nothing existing is touched.
    fn write_parts(
        &self,
        dir: &Path,
        existing: &[String],
        req: &Request,
    ) -> Result<(), StoreError> {
        let lens: Vec<u64> = req.segments.iter().map(|s| s.len).collect();
        let sizes = cut_parts(&lens);
        let old: Vec<&str> = existing.iter().map(String::as_str).collect();
        let names = next_names(&old, sizes.len()).map_err(|e| other(format!("pack {e}")))?;
        let todo = names.into_iter().zip(sizes).collect::<Vec<_>>().into_iter();
        let mut cut = Cut {
            dir,
            todo,
            file: None,
            room: 0,
            written: 0,
        };
        for seg in req.segments {
            let before = cut.written;
            (seg.write)(&mut cut).map_err(other)?;
            if cut.written - before != seg.len {
                return Err(other(format!(
                    "a segment wrote {} bytes, not {}",
                    cut.written - before,
                    seg.len
                )));
            }
        }
        cut.flush().map_err(other)?;
        std::fs::write(dir.join(INDEX_FILE), req.index).map_err(other)
    }

    /// One commit with the new parts and the new index, hooks off, identity and dates forced.
    fn commit(&self, dir: &Path) -> Result<(), StoreError> {
        self.git(&["add", "--all"], Some(dir), &[])?;
        let no_hooks = format!(
            "core.hooksPath={}",
            dir.with_file_name("no-hooks").display()
        );
        let mut commit = GIT_IDENTITY.to_vec();
        commit.extend(["-c", &no_hooks, "commit", "-m", "agr sync"]);
        let date = format!("@{} +0000", self.clock.unix_secs()?);
        let mut env = vec![
            ("GIT_AUTHOR_DATE", date.clone()),
            ("GIT_COMMITTER_DATE", date),
        ];
        env.extend(IDENTITY_ENV.iter().map(|(k, v)| (*k, v.to_string())));
        self.git(&commit, Some(dir), &env).map(drop)
    }

    /// Plain `git push`, never forced. A rejection is `Moved` with the index the remote has now.
    fn push(&self, dir: &Path, id: &str) -> Result<(), StoreError> {
        self.wait_turn(id);
        let mut args = CRED.to_vec();
        args.extend(["push", "--no-verify", "--progress"]);
        let sink = self.progress.as_ref();
        let mut on_line = |line: &str| {
            sink.iter()
                .for_each(|s| writing_percent(line).iter().for_each(|p| s(*p)))
        };
        let run = self
            .runner
            .run_watched("git", &strings(&args), Some(dir), &[], &mut on_line);
        let out = io_result("git", run).map_err(be)?;
        if out.success() {
            self.note_push(id);
            return Ok(());
        }
        match map_failure("git", &out.stderr, out.code) {
            BackendError::Changed(_) => Err(self.fetched_index(dir)),
            e => Err(be(e)),
        }
    }

    /// `Moved` with `FETCH_HEAD:index`, a failing fetch or read is that error and never a `Moved`.
    fn fetched_index(&self, dir: &Path) -> StoreError {
        let mut fetch = CRED.to_vec();
        fetch.extend(["fetch", "--progress"]);
        let read = |_| {
            self.git(
                &["show", &format!("FETCH_HEAD:{INDEX_FILE}")],
                Some(dir),
                &[],
            )
        };
        match self.git(&fetch, Some(dir), &[]).and_then(read) {
            Ok(out) => StoreError::Moved(out.stdout),
            Err(e) => e,
        }
    }
}
