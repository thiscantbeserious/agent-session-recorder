//! Append to a pack gist through the cached clone of the own open pack, store side only.
//! The store reports `Moved(current index)` and drops its clone, the one retry is the pack layer's.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::sync::PoisonError;

use super::api::{be, INDEX_FILE};
use super::cache::PackCache;
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

/// Writes the planned parts one after the other, a segment may span or share parts.
struct Cut<'a> {
    dir: &'a Path,
    todo: std::vec::IntoIter<(String, u64)>,
    file: Option<File>,
    room: u64,
    written: u64,
}

impl Write for Cut<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        while self.room == 0 {
            let (name, size) = self
                .todo
                .next()
                .ok_or_else(|| io::Error::other("wrote past the planned parts"))?;
            self.file = Some(File::create(self.dir.join(name))?);
            self.room = size;
        }
        let take = buf.len().min(self.room as usize);
        let n = self
            .file
            .as_mut()
            .expect("opened above")
            .write(&buf[..take])?;
        (self.room, self.written) = (self.room - n as u64, self.written + n as u64);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.as_mut().map_or(Ok(()), Write::flush)
    }
}

fn other(e: impl std::fmt::Display) -> StoreError {
    StoreError::Other(e.to_string())
}

/// `data.*` names and sizes out of `git ls-tree -l`, one `mode blob sha size<TAB>name` line each.
fn parse_tree(text: &str) -> Vec<(String, u64)> {
    let rows = text
        .lines()
        .filter_map(|l| l.split_once('\t').map(|(a, name)| (a, name)));
    let sized =
        rows.filter_map(|(a, name)| Some((name, a.split_whitespace().nth(3)?.parse().ok()?)));
    let data = sized.filter(|(name, _)| name.starts_with("data."));
    data.map(|(name, size)| (name.to_string(), size)).collect()
}

impl<R: Runner> GistStore<R> {
    pub(super) fn append_pack(&self, id: &str, req: &Request) -> Result<(), StoreError> {
        let cache = self.pack_cache(id)?;
        let res = self.append_in(&cache, id, req);
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

    /// Stub: a fresh clone every time, tried twice.
    fn sync_clone(&self, cache: &PackCache, id: &str) -> Result<(), StoreError> {
        cache.remove();
        cache.prepare()?;
        let (url, target) = (
            format!("https://gist.github.com/{id}.git"),
            cache.dir().to_string_lossy(),
        );
        let mut args = CRED.to_vec();
        args.extend(["clone", "--progress", &url, &target]);
        self.git(&args, None, &[])
            .or_else(|_| self.git(&args, None, &[]))
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
        self.wait_turn(id)?;
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
            return self.note_push(id);
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

    /// Waits out the 10 s per gist (persisted, other processes count) and the 1 s of the pacer.
    fn wait_turn(&self, id: &str) -> Result<(), StoreError> {
        let last = self.last_push();
        let wait = last
            .as_ref()
            .map(|l| l.wait_for(id, self.clock.unix_secs().unwrap_or(0)));
        wait.filter(|w| !w.is_zero())
            .iter()
            .for_each(|w| self.clock.sleep(*w));
        self.pacer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .wait();
        Ok(())
    }

    fn note_push(&self, id: &str) -> Result<(), StoreError> {
        let now = self.clock.unix_secs()?;
        self.last_push().iter().for_each(|l| l.put(id, now));
        Ok(())
    }

    pub(super) fn delete_pack(&self, id: &str) -> Result<(), StoreError> {
        self.forget_clone(id);
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(id);
        self.pacer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .wait();
        self.run_ok("gh", &["gist", "delete", id, "--yes"])
            .map(drop)
    }
}
