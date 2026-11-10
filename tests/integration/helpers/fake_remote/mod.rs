//! In-memory gist remote behind the `Runner` trait for the gist store tests: `gh api`, `gh gist`
//! and the few `git` verbs of an append, with a real worktree per clone. Nothing leaves the process.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use self::git::git_verb;
use agr::sync::testing::{Output, Runner};

mod git;

pub type Files = BTreeMap<String, Vec<u8>>;

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

/// Shared handle: the store owns one, the test keeps another to look inside.
#[derive(Clone, Default)]
pub struct Handle(Rc<FakeRemote>);

impl std::ops::Deref for Handle {
    type Target = FakeRemote;
    fn deref(&self) -> &FakeRemote {
        &self.0
    }
}

#[derive(Default)]
pub struct FakeRemote {
    state: RefCell<State>,
    calls: RefCell<Vec<String>>,
}

pub fn out(code: i32, stdout: &str, stderr: &str) -> Result<Output> {
    Ok(Output {
        code: Some(code),
        stdout: stdout.into(),
        stderr: stderr.into(),
    })
}

/// A commit of another machine on top of the head of `gist`.
fn elsewhere(gist: &mut Gist, edit: impl FnOnce(&mut Files)) {
    let mut next = gist.history.last().unwrap().clone();
    edit(&mut next);
    gist.history.push(next);
}

impl FakeRemote {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Handle {
        Handle(Rc::new(Self::default()))
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    fn note(&self, program: &str, args: &[String]) {
        self.calls
            .borrow_mut()
            .push(format!("{program} {}", args.join(" ")));
    }

    pub fn clear_calls(&self) {
        self.calls.borrow_mut().clear();
    }

    /// A pack gist with `index` and the given data parts, returns its id.
    pub fn seed(&self, index: &str, parts: &[(&str, &[u8])]) -> String {
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
    pub fn fail(&self, verb: &'static str) {
        self.state.borrow_mut().failing.push(verb);
    }

    /// Another machine commits `edit` right before the next push of a clone.
    pub fn race_on_push(&self, edit: impl FnOnce(&mut Files) + 'static) {
        self.state.borrow_mut().race = Some(Box::new(edit));
    }

    /// A commit in the clone at `dir` that never reached the remote, a kill between commit and push.
    pub fn commit_unpushed(&self, dir: &Path, index: &str) {
        let mut state = self.state.borrow_mut();
        let c = state.clones.get_mut(dir).unwrap();
        let mut files = c.local.last().cloned().unwrap_or_default();
        if c.local.is_empty() {
            files = std::fs::read_dir(dir)
                .unwrap()
                .flatten()
                .filter(|e| e.path().is_file())
                .map(|e| {
                    (
                        e.file_name().into_string().unwrap(),
                        std::fs::read(e.path()).unwrap(),
                    )
                })
                .collect();
        }
        files.insert("index".into(), index.as_bytes().to_vec());
        std::fs::write(dir.join("index"), index).unwrap();
        c.local.push(files);
    }

    /// Names changed by every commit a clone made.
    pub fn commit_changes(&self) -> Vec<Vec<String>> {
        self.state.borrow().commits.clone()
    }

    /// Names changed by every accepted push, what travelled to the remote.
    pub fn push_changes(&self) -> Vec<Vec<String>> {
        self.state.borrow().pushes.clone()
    }

    /// Another machine's commit on the gist, `edit` changes the head files.
    pub fn commit_elsewhere(&self, id: &str, edit: impl FnOnce(&mut Files)) {
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

    fn range(&self, args: &[String], dest: &Path) -> Result<Output> {
        let url = args.last().unwrap();
        let mut parts = url.rsplit('/');
        let (name, _raw, id) = (parts.next().unwrap(), parts.next(), parts.next().unwrap());
        let range = args.iter().find_map(|a| a.strip_prefix("Range: bytes="));
        let state = self.state.borrow();
        let body = &state.gists[id].history.last().unwrap()[name];
        let Some((from, to)) = range.and_then(|r| r.split_once('-')) else {
            std::fs::write(dest, body)?;
            return out(0, "", "");
        };
        let (from, to): (usize, usize) = (from.parse().unwrap(), to.parse().unwrap());
        std::fs::write(dest, &body[from..=to])?;
        out(0, "", "")
    }
}

impl Runner for Handle {
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
