//! The `git` verbs of the fake gist remote: clone, fetch, merge, commit and push on the clones.

use std::collections::BTreeSet;
use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};

use super::{elsewhere, out, Checkout, FakeRemote, Files, State};
use crate::sync::backend::gist::runner::Output;

/// The git subcommand, the `-c key=value` pairs in front of it skipped.
pub(super) fn git_verb(args: &[String]) -> &str {
    let mut rest = args.iter().map(String::as_str);
    while let Some(arg) = rest.next() {
        match arg {
            "-c" => drop(rest.next()),
            _ if !arg.starts_with('-') => return arg,
            _ => {}
        }
    }
    ""
}

pub(super) fn changed(old: &Files, new: &Files) -> Vec<String> {
    let names = old.keys().chain(new.keys());
    let names = names.filter(|n| old.get(*n) != new.get(*n)).cloned();
    names.collect::<BTreeSet<_>>().into_iter().collect()
}

fn worktree(dir: &Path) -> Files {
    let entries = std::fs::read_dir(dir).unwrap().flatten();
    let read = |e: std::fs::DirEntry| {
        (
            e.file_name().into_string().unwrap(),
            std::fs::read(e.path()),
        )
    };
    entries
        .filter(|e| e.path().is_file())
        .map(read)
        .map(|(n, b)| (n, b.unwrap()))
        .collect()
}

fn checkout(dir: &Path, files: &Files) {
    std::fs::create_dir_all(dir).unwrap();
    for old in worktree(dir).keys() {
        std::fs::remove_file(dir.join(old)).unwrap();
    }
    std::fs::create_dir_all(dir).unwrap();
    for (name, body) in files {
        std::fs::write(dir.join(name), body).unwrap();
    }
}

impl FakeRemote {
    pub(super) fn git(&self, args: &[String], cwd: Option<&Path>) -> Result<Output> {
        let verb = git_verb(args);
        let mut state = self.state.borrow_mut();
        let dir = cwd.map(Path::to_path_buf);
        match verb {
            "clone" => self.clone_into(&mut state, args),
            "fetch" => self.fetch(&mut state, &dir.unwrap()),
            "merge" => self.merge(&mut state, &dir.unwrap()),
            "ls-tree" => self.ls_tree(&state, &dir.unwrap()),
            "show" => self.show(&state, &dir.unwrap(), args.last().unwrap()),
            "add" => out(0, "", ""),
            "commit" => self.commit(&mut state, &dir.unwrap()),
            "push" => self.push(&mut state, &dir.unwrap()),
            _ => Err(Error::new(ErrorKind::Unsupported, args.join(" "))),
        }
    }

    fn fetch(&self, state: &mut State, dir: &Path) -> Result<Output> {
        if state.failing.contains(&"fetch") {
            return out(1, "", "fatal: fetch broke");
        }
        let c = state.clones.get_mut(dir).unwrap();
        c.fetched = state.gists[&c.id].history.len() - 1;
        out(0, "", "")
    }

    /// Fast-forward only: the worktree is replaced by `FETCH_HEAD`.
    fn merge(&self, state: &mut State, dir: &Path) -> Result<Output> {
        let c = state.clones.get_mut(dir).unwrap();
        c.base = c.fetched;
        checkout(dir, &state.gists[&c.id].history[c.base]);
        out(0, "", "")
    }

    fn clone_into(&self, state: &mut State, args: &[String]) -> Result<Output> {
        if state.failing.contains(&"clone") {
            return out(1, "", "fatal: clone broke");
        }
        let n = args.len();
        let (url, target) = (&args[n - 2], PathBuf::from(&args[n - 1]));
        let id = url.trim_end_matches(".git").rsplit('/').next().unwrap();
        let Some(gist) = state.gists.get(id) else {
            return out(128, "", "remote: Repository not found.");
        };
        let at = gist.history.len() - 1;
        checkout(&target, &gist.history[at]);
        let (id, local) = (id.to_string(), Vec::new());
        let clone = Checkout {
            id,
            base: at,
            fetched: at,
            local,
        };
        state.clones.insert(target, clone);
        out(0, "", "")
    }

    fn ls_tree(&self, state: &State, dir: &Path) -> Result<Output> {
        let c = &state.clones[dir];
        let tree = &state.gists[&c.id].history[c.base];
        let row =
            |(n, b): (&String, &Vec<u8>)| format!("100644 blob {:040} {:>7}\t{n}\n", 0, b.len());
        out(0, &tree.iter().map(row).collect::<String>(), "")
    }

    fn show(&self, state: &State, dir: &Path, spec: &str) -> Result<Output> {
        let c = &state.clones[dir];
        let at = [c.base, c.fetched][usize::from(spec.starts_with("FETCH_HEAD"))];
        let body = &state.gists[&c.id].history[at][spec.rsplit(':').next().unwrap()];
        out(0, &String::from_utf8_lossy(body), "")
    }

    fn commit(&self, state: &mut State, dir: &Path) -> Result<Output> {
        let snapshot = worktree(dir);
        let c = state.clones.get_mut(dir).unwrap();
        let before = match c.local.last() {
            Some(last) => last.clone(),
            None => state.gists[&c.id].history[c.base].clone(),
        };
        let names = changed(&before, &snapshot);
        state.clones.get_mut(dir).unwrap().local.push(snapshot);
        state.commits.push(names);
        out(0, "", "")
    }

    fn push(&self, state: &mut State, dir: &Path) -> Result<Output> {
        if let Some(edit) = state.race.take() {
            let id = state.clones[dir].id.clone();
            elsewhere(state.gists.get_mut(&id).unwrap(), edit);
        }
        let c = &state.clones[dir];
        let (id, head) = (c.id.clone(), state.gists[&c.id].history.len() - 1);
        if c.base != head {
            let msg =
                " ! [rejected]        HEAD -> main (fetch first)\nerror: failed to push some refs";
            return out(1, "", msg);
        }
        let first = state.gists[&id].history[head].clone();
        let local = std::mem::take(&mut state.clones.get_mut(dir).unwrap().local);
        let names = changed(&first, local.last().unwrap_or(&first));
        let gist = state.gists.get_mut(&id).unwrap();
        gist.history.extend(local);
        let new_head = gist.history.len() - 1;
        let c = state.clones.get_mut(dir).unwrap();
        (c.base, c.fetched) = (new_head, new_head);
        state.pushes.push(names);
        out(0, "", "")
    }
}
