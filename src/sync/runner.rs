//! Process runner behind a small trait, the only place that spawns `gh` and `git`.

use std::path::Path;
use std::process::{Command, Stdio};

/// Finished process, stdout and stderr as lossy text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

/// Runs one command line, never through a shell.
pub trait Runner {
    fn run(&self, program: &str, args: &[String], cwd: Option<&Path>) -> std::io::Result<Output>;
}

/// Real processes, no stdin and no prompts so a missing credential fails instead of hanging.
pub struct ProcessRunner;

impl Runner for ProcessRunner {
    fn run(&self, program: &str, args: &[String], cwd: Option<&Path>) -> std::io::Result<Output> {
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GH_PROMPT_DISABLED", "1");
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }
        let out = command.output()?;
        Ok(Output {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

/// Start `exe args` detached, null stdio, own process group on unix, never waited on.
pub fn spawn_detached(exe: &Path, args: &[String]) -> std::io::Result<()> {
    let mut command = Command::new(exe);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    command.spawn().map(drop)
}
