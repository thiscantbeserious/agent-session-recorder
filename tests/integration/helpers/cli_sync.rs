//! Shared rig for the `cli_sync_*_test.rs` files: `agr sync ...` against the file based fake backend.

use agr::files::codec;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;

pub struct Out {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

impl Out {
    pub fn success(&self) -> bool {
        self.code == 0
    }

    pub fn all(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

pub struct Machine {
    pub home: TempDir,
    pub storage: PathBuf,
    pub remote: PathBuf,
}

impl Machine {
    pub fn new(remote: &Path, config: &str) -> Self {
        let home = TempDir::new().unwrap();
        let storage = home.path().join("sessions");
        let cfg = home.path().join(".config/agr");
        fs::create_dir_all(&cfg).unwrap();
        let toml = format!("[storage]\ndirectory = \"{}\"\n{config}", storage.display());
        fs::write(cfg.join("config.toml"), toml).unwrap();
        Machine {
            home,
            storage,
            remote: remote.to_path_buf(),
        }
    }

    pub fn gist(remote: &Path) -> Self {
        Self::new(remote, "[sync]\nbackend = \"gist\"\n")
    }

    pub fn session(&self, agent: &str, name: &str, text: &str) -> PathBuf {
        let dir = self.storage.join(agent);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        codec::atomic_write(&path, |w| Ok(w.write_all(text.as_bytes())?)).unwrap();
        path
    }

    pub fn base(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_agr"));
        cmd.arg("sync")
            .args(args)
            .env("NO_COLOR", "1")
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.home.path().join(".config"))
            .env("AGR_SYNC_TEST_BACKEND_DIR", &self.remote);
        cmd
    }

    pub fn run(&self, args: &[&str]) -> Out {
        self.exec(self.base(args), None)
    }

    pub fn run_stdin(&self, args: &[&str], input: &str) -> Out {
        self.exec(self.base(args), Some(input))
    }

    pub fn exec(&self, mut cmd: Command, input: Option<&str>) -> Out {
        cmd.stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        if let Some(text) = input {
            let written = child.stdin.take().unwrap().write_all(text.as_bytes());
            // a child that exits before reading all of its input closes the pipe, that is not a failure
            if let Err(e) = written {
                assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe, "{e}");
            }
        }
        let out = child.wait_with_output().unwrap();
        Out {
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            code: out.status.code().unwrap_or(-1),
        }
    }

    pub fn key_path(&self) -> PathBuf {
        self.home.path().join(".config/agr/sync.key")
    }

    pub fn state(&self) -> String {
        fs::read_to_string(self.home.path().join(".config/agr/state.toml")).unwrap_or_default()
    }
}

/// Files of every pack directory of the directory store whose name ends with `suffix`.
pub fn remote_files(remote: &Path, suffix: &str) -> Vec<PathBuf> {
    let packs: Vec<PathBuf> = fs::read_dir(remote)
        .map(|rd| rd.map(|e| e.unwrap().path()).collect())
        .unwrap_or_default();
    let mut files: Vec<PathBuf> = packs
        .iter()
        .flat_map(|p| fs::read_dir(p).into_iter().flatten().flatten())
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(suffix))
        .collect();
    files.sort();
    files
}

pub fn remote() -> TempDir {
    TempDir::new().unwrap()
}

#[cfg(unix)]
#[allow(clippy::result_unit_err)]
pub fn which_asciinema() -> Result<String, ()> {
    std::env::var_os("PATH")
        .and_then(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join("asciinema"))
                .find(|f| f.is_file())
        })
        .map(|f| f.to_string_lossy().into_owned())
        .ok_or(())
}

#[cfg(unix)]
pub fn fake_bin(scripts: &[(&str, &str)]) -> TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    for (name, body) in scripts {
        let path = dir.path().join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    dir
}

#[cfg(unix)]
pub fn gist_cmd(m: &Machine, bin: &TempDir, args: &[&str]) -> Out {
    let mut cmd = m.base(args);
    cmd.env("PATH", bin.path())
        .env_remove("AGR_SYNC_TEST_BACKEND_DIR");
    m.exec(cmd, None)
}

pub const CAST: &str =
    "{\"version\":3,\"term\":{\"cols\":80,\"rows\":24}}\n[0.5,\"o\",\"a\"]\n[10.0,\"o\",\"b\"]\n";

pub fn phrase_of(m: &Machine) -> String {
    let out = m.run(&["key", "export"]).stdout;
    let line = out.lines().find(|l| l.trim().starts_with("agr1-")).unwrap();
    line.trim().to_string()
}

#[cfg(unix)]
pub fn all_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            all_files(&path, found);
        } else {
            found.push(path);
        }
    }
}

/// Every file below `dir` with its size, sorted.
#[cfg(unix)]
pub fn tree(dir: &Path) -> Vec<(PathBuf, u64)> {
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in fs::read_dir(&d).into_iter().flatten().flatten() {
            let meta = e.metadata().unwrap();
            if meta.is_dir() {
                todo.push(e.path());
            } else {
                out.push((e.path(), meta.len()));
            }
        }
    }
    out.sort();
    out
}

pub fn writer_of(m: &Machine) -> Option<String> {
    let line = m
        .state()
        .lines()
        .find(|l| l.starts_with("writer_id"))?
        .to_string();
    Some(line)
}

/// Every file of the directory store with its bytes, equal before and after means no append.
pub fn fingerprint(remote: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut todo = vec![remote.to_path_buf()];
    while let Some(dir) = todo.pop() {
        for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = e.path();
            if path.is_dir() {
                todo.push(path);
            } else {
                files.push((path.clone(), fs::read(&path).unwrap()));
            }
        }
    }
    files.sort();
    files
}

/// Machine `a` pushed `n` sessions `claude/s00.cast.gz`..., machine `b` holds the key, no writer
/// id and one local only session `claude/zz-local.cast.gz`.
pub fn pushed_pair(remote: &Path, n: usize) -> (Machine, Machine) {
    let a = Machine::gist(remote);
    for i in 0..n {
        a.session("claude", &format!("s{i:02}.cast.gz"), &format!("text {i}"));
    }
    assert_eq!(a.run(&["push", "--yes"]).code, 0);
    let b = Machine::gist(remote);
    assert_eq!(b.run(&["key", "import", &phrase_of(&a)]).code, 0);
    b.session("claude", "zz-local.cast.gz", "only here");
    (a, b)
}
