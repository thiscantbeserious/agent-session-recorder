//! Fake `gh` and `git` runner and gist backend helpers.

use agr::sync::testing::gist::{
    part_name, part_names, Clock, FakeClock, Output, Runner, MANIFEST_FILE, STUB_BODY, STUB_FILE,
};
use agr::sync::testing::{pull_bytes, push_bytes, GistBackend};
use agr::sync::{BackendError, SyncBackend};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

pub type Files = Vec<(String, Vec<u8>)>;

/// Whole-blob shorthands over the streaming trait.
pub trait Bytes: SyncBackend + Sized {
    fn push_b(&self, id: &str, e: Option<&str>, m: &str, b: &[u8]) -> Result<(), BackendError> {
        push_bytes(self, id, e, m, b)
    }
    fn pull_b(&self, id: &str) -> Result<(Option<String>, Vec<u8>), BackendError> {
        pull_bytes(self, id)
    }
}
impl<T: SyncBackend> Bytes for T {}

#[derive(Default)]
pub struct Fake {
    pub calls: RefCell<Vec<String>>,
    pub replies: RefCell<VecDeque<std::io::Result<Output>>>,
    pub pushed: RefCell<Option<Files>>,
    pub stub_seen: RefCell<Option<String>>,
    pub clone_dirs: RefCell<Vec<PathBuf>>,
    pub envs: RefCell<Vec<Vec<(String, String)>>>,
    pub pushed_manifest: RefCell<Option<String>>,
    pub stamps: RefCell<Vec<(String, Duration)>>,
    pub clock: Option<Arc<FakeClock>>,
    pub remote: Files,
    /// Stderr lines `run_watched` hands to the watcher of a `git push` or `git clone`.
    pub watch: Vec<String>,
}

/// Every `data.*` file of a clone, sorted by name.
pub fn data_files(dir: &Path) -> Files {
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

pub fn remote_parts(n: usize) -> Files {
    (0..n).map(|i| (part_name(i), vec![i as u8; 3])).collect()
}

pub fn ok(stdout: &str) -> std::io::Result<Output> {
    Ok(Output {
        code: Some(0),
        stdout: stdout.into(),
        stderr: String::new(),
    })
}

pub fn fail(stderr: &str) -> std::io::Result<Output> {
    Ok(Output {
        code: Some(1),
        stdout: String::new(),
        stderr: stderr.into(),
    })
}

impl Fake {
    pub fn with(replies: Vec<std::io::Result<Output>>) -> Self {
        Self {
            replies: RefCell::new(replies.into()),
            ..Self::default()
        }
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    pub fn side_effects(&self, program: &str, args: &[String], cwd: Option<&Path>) {
        if program == "gh" && args.get(1).map(String::as_str) == Some("create") {
            let stub = std::fs::read_to_string(&args[2]).unwrap();
            *self.stub_seen.borrow_mut() = Some(stub);
        }
        if program == "git" && args.iter().any(|a| a == "clone") {
            let dir = PathBuf::from(args.last().unwrap());
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
    fn run_watched(
        &self,
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
        env: &[(&str, String)],
        on_line: &mut dyn FnMut(&str),
    ) -> std::io::Result<Output> {
        let out = self.run(program, args, cwd, env);
        self.watch.iter().for_each(|line| on_line(line));
        out
    }

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

pub fn backend<'a>(fake: &'a Fake, base: &Path) -> GistBackend<&'a Fake> {
    GistBackend::with_runner(fake, base.to_path_buf())
}

pub fn scrub(calls: &[String], base: &Path) -> Vec<String> {
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

pub fn source_of(bytes: &'static [u8]) -> impl Fn(&mut dyn std::io::Write) -> std::io::Result<()> {
    move |w| w.write_all(bytes)
}
