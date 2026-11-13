//! Fake `gh` and `git` runner and legacy gist reader helpers.

use agr::sync::testing::gist::{Clock, FakeClock, GistLegacy, Output, Runner};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

pub type Files = Vec<(String, Vec<u8>)>;

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
    /// A server that ignores `Range` and answers 200 with the whole part.
    pub ignore_range: bool,
}

/// Every `data.*` file of a clone, sorted by name.
pub fn data_files(dir: &Path) -> Files {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("data."))
        .map(|n| {
            let body = std::fs::read(dir.join(&n)).unwrap();
            (n, body)
        })
        .collect();
    files.sort();
    files
}

pub fn remote_parts(n: usize) -> Files {
    (0..n)
        .map(|i| (format!("data.{i:03}"), vec![i as u8; 3]))
        .collect()
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
            std::fs::write(dir.join(".keep"), "agr\n").unwrap();
            for (name, body) in &self.remote {
                std::fs::write(dir.join(name), body).unwrap();
            }
            self.clone_dirs.borrow_mut().push(dir);
        }
        if program == "git" && args.iter().any(|a| a == "push") {
            *self.pushed.borrow_mut() = Some(data_files(cwd.unwrap()));
            let manifest = std::fs::read_to_string(cwd.unwrap().join("manifest")).ok();
            *self.pushed_manifest.borrow_mut() = manifest;
        }
    }
}

impl Fake {
    /// The `remote` bytes a `gh api [-H "Range: bytes=a-b"] <raw_url>` reads, the file is named like
    /// the last url segment, without a `Range` it is the whole file.
    fn raw_body(&self, program: &str, args: &[String]) -> Option<Vec<u8>> {
        let range = args.iter().find_map(|a| a.strip_prefix("Range: bytes="));
        let name = args.last()?.rsplit('/').next()?;
        let (_, body) = self.remote.iter().find(|(n, _)| n == name)?;
        let wanted = match range {
            Some(r) if !self.ignore_range => {
                let (from, to) = r.split_once('-')?;
                body[from.parse::<usize>().ok()?..=to.parse::<usize>().ok()?].to_vec()
            }
            _ => body.clone(),
        };
        (program == "gh").then_some(wanted)
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

    fn run_to_file(
        &self,
        program: &str,
        args: &[String],
        env: &[(&str, String)],
        dest: &Path,
    ) -> std::io::Result<Output> {
        let out = self.run(program, args, None, env)?;
        if let (true, Some(body)) = (out.success(), self.raw_body(program, args)) {
            std::fs::write(dest, body)?;
        }
        Ok(out)
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

pub fn backend<'a>(fake: &'a Fake, base: &Path) -> GistLegacy<&'a Fake> {
    GistLegacy::with_runner(fake, base.to_path_buf())
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
