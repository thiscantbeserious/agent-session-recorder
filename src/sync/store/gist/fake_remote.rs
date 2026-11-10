//! In-memory gist remote behind the `Runner` trait for the gist store tests: `gh api`, `gh gist`
//! and the few `git` verbs of an append, with a real worktree per clone. Nothing leaves the process.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::sync::backend::gist::runner::{Output, Runner};

pub(crate) type Files = BTreeMap<String, Vec<u8>>;

#[derive(Default)]
struct Gist {
    desc: String,
    /// Commits of the branch, the last one is the head.
    history: Vec<Files>,
}

/// A clone: the gist, the commit its worktree started from, what `fetch` found, local commits.
struct Checkout {
    id: String,
    base: usize,
    fetched: usize,
    local: Vec<Files>,
}

#[derive(Default)]
struct State {
    gists: BTreeMap<String, Gist>,
    clones: BTreeMap<PathBuf, Checkout>,
    next: u64,
    /// Names changed per commit and per accepted push.
    commits: Vec<Vec<String>>,
    pushes: Vec<Vec<String>>,
    failing: Vec<&'static str>,
    /// Runs once right before the next push, another machine winning the race.
    race: Option<Race>,
}

type Race = Box<dyn FnOnce(&mut Files)>;

/// Shared handle: the store owns one `Rc`, the test keeps another to look inside.
#[derive(Default)]
pub(crate) struct FakeRemote {
    state: RefCell<State>,
    calls: RefCell<Vec<String>>,
}

pub(crate) fn out(code: i32, stdout: &str, stderr: &str) -> Result<Output> {
    Ok(Output {
        code: Some(code),
        stdout: stdout.into(),
        stderr: stderr.into(),
    })
}

/// The git subcommand, the `-c key=value` pairs in front of it skipped.
fn git_verb(args: &[String]) -> &str {
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

/// A commit of another machine on top of the head of `gist`.
fn elsewhere(gist: &mut Gist, edit: impl FnOnce(&mut Files)) {
    let mut next = gist.history.last().unwrap().clone();
    edit(&mut next);
    gist.history.push(next);
}

fn changed(old: &Files, new: &Files) -> Vec<String> {
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
    pub(crate) fn new() -> Rc<Self> {
        Rc::new(Self::default())
    }

    pub(crate) fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    fn note(&self, program: &str, args: &[String]) {
        self.calls
            .borrow_mut()
            .push(format!("{program} {}", args.join(" ")));
    }

    pub(crate) fn clear_calls(&self) {
        self.calls.borrow_mut().clear();
    }

    /// A pack gist with `index` and the given data parts, returns its id.
    pub(crate) fn seed(&self, index: &str, parts: &[(&str, &[u8])]) -> String {
        let mut files = Files::from([("index".to_string(), index.as_bytes().to_vec())]);
        files.extend(parts.iter().map(|(n, b)| (n.to_string(), b.to_vec())));
        self.add("agr:v2 TAG", files)
    }

    fn add(&self, desc: &str, files: Files) -> String {
        let mut state = self.state.borrow_mut();
        state.next += 1;
        let id = format!("{:032x}", state.next);
        let (desc, history) = (desc.to_string(), vec![files]);
        state.gists.insert(id.clone(), Gist { desc, history });
        id
    }

    /// Makes every `git <verb>` of a clone fail, `clone` or `fetch`.
    pub(crate) fn fail(&self, verb: &'static str) {
        self.state.borrow_mut().failing.push(verb);
    }

    /// Another machine commits `edit` right before the next push of a clone.
    pub(crate) fn race_on_push(&self, edit: impl FnOnce(&mut Files) + 'static) {
        self.state.borrow_mut().race = Some(Box::new(edit));
    }

    /// Names changed by every commit a clone made.
    pub(crate) fn commit_changes(&self) -> Vec<Vec<String>> {
        self.state.borrow().commits.clone()
    }

    /// Names changed by every accepted push, what travelled to the remote.
    pub(crate) fn push_changes(&self) -> Vec<Vec<String>> {
        self.state.borrow().pushes.clone()
    }

    /// Another machine's commit on the gist, `edit` changes the head files.
    pub(crate) fn commit_elsewhere(&self, id: &str, edit: impl FnOnce(&mut Files)) {
        elsewhere(self.state.borrow_mut().gists.get_mut(id).unwrap(), edit);
    }

    fn gist_json(&self, id: &str, full: bool) -> serde_json::Value {
        let state = self.state.borrow();
        let gist = &state.gists[id];
        let head = gist.history.last().unwrap();
        let files: serde_json::Map<_, _> = head
            .iter()
            .map(|(name, body)| {
                let raw = format!("https://gist.githubusercontent.com/u/{id}/raw/{name}");
                let mut file = serde_json::json!({ "size": body.len(), "raw_url": raw });
                if full && name == "index" {
                    file["content"] = String::from_utf8_lossy(body).into_owned().into();
                }
                (name.clone(), file)
            })
            .collect();
        let updated = format!("t{}", gist.history.len());
        serde_json::json!({ "id": id, "description": gist.desc, "updated_at": updated, "files": files })
    }

    fn gh(&self, args: &[String]) -> Result<Output> {
        let word = |i: usize| args.get(i).map_or("", String::as_str);
        match (word(0), word(1)) {
            ("api", "gists?per_page=100") => {
                let ids: Vec<String> = self.state.borrow().gists.keys().cloned().collect();
                let all: Vec<_> = ids.iter().map(|id| self.gist_json(id, false)).collect();
                out(0, &serde_json::Value::from(all).to_string(), "")
            }
            ("api", path) if path.starts_with("gists/") => {
                let id = path.trim_start_matches("gists/");
                match self.state.borrow().gists.contains_key(id) {
                    true => out(0, &self.gist_json(id, true).to_string(), ""),
                    false => out(1, "", "HTTP 404: Not Found"),
                }
            }
            ("gist", "create") => self.create(args),
            ("gist", "delete") => {
                self.state.borrow_mut().gists.remove(word(2));
                out(0, "", "")
            }
            _ => Err(Error::new(ErrorKind::Unsupported, args.join(" "))),
        }
    }

    fn create(&self, args: &[String]) -> Result<Output> {
        let desc = args.iter().skip_while(|a| *a != "--desc").nth(1).unwrap();
        let mut files = Files::new();
        for path in args[2..].iter().take_while(|a| *a != "--desc") {
            let name = path.rsplit('/').next().unwrap().to_string();
            files.insert(name, std::fs::read(path)?);
        }
        let id = self.add(desc, files);
        out(0, &format!("https://gist.github.com/u/{id}\n"), "")
    }

    fn git(&self, args: &[String], cwd: Option<&Path>) -> Result<Output> {
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

    fn range(&self, args: &[String], dest: &Path) -> Result<Output> {
        let url = args.last().unwrap();
        let mut parts = url.rsplit('/');
        let (name, _raw, id) = (parts.next().unwrap(), parts.next(), parts.next().unwrap());
        let range = args
            .iter()
            .find_map(|a| a.strip_prefix("Range: bytes="))
            .unwrap();
        let (from, to) = range.split_once('-').unwrap();
        let (from, to): (usize, usize) = (from.parse().unwrap(), to.parse().unwrap());
        let state = self.state.borrow();
        let body = &state.gists[id].history.last().unwrap()[name];
        std::fs::write(dest, &body[from..=to])?;
        out(0, "", "")
    }
}

impl Runner for Rc<FakeRemote> {
    fn run(
        &self,
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
        _env: &[(&str, String)],
    ) -> Result<Output> {
        self.note(program, args);
        match program {
            "gh" => self.gh(args),
            _ => self.git(args, cwd),
        }
    }

    fn run_to_file(
        &self,
        program: &str,
        args: &[String],
        _env: &[(&str, String)],
        dest: &Path,
    ) -> Result<Output> {
        self.note(program, args);
        self.range(args, dest)
    }

    /// Progress lines like git prints them: the clone and fetch receive, the push writes.
    fn run_watched(
        &self,
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
        env: &[(&str, String)],
        on_line: &mut dyn FnMut(&str),
    ) -> Result<Output> {
        let res = self.run(program, args, cwd, env)?;
        let lines: &[&str] = match git_verb(args) {
            "push" => &[
                "Writing objects:  50% (1/2)",
                "Writing objects: 100% (2/2), done.",
            ],
            "clone" | "fetch" => &["Receiving objects:  30% (1/3)"],
            _ => &[],
        };
        lines.iter().for_each(|l| on_line(l));
        Ok(res)
    }
}
