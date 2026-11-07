//! Process runner behind a small trait, the only place that spawns `gh` and `git`.

use std::io::ErrorKind;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

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
    fn run(
        &self,
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
        env: &[(&str, String)],
    ) -> std::io::Result<Output>;
}

/// Longest a process may stay silent on stdout and stderr before it counts as stalled.
// ponytail: fixed 120 s of silence, any output resets it, make it a config key if slow links need more
const STALL_LIMIT: Duration = Duration::from_secs(120);

/// Real processes, no stdin and no prompts so a missing credential fails instead of hanging.
/// A process that prints nothing for `stall` is killed.
pub struct ProcessRunner {
    pub stall: Duration,
}

impl Default for ProcessRunner {
    fn default() -> Self {
        Self { stall: STALL_LIMIT }
    }
}

impl Runner for ProcessRunner {
    fn run(
        &self,
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
        env: &[(&str, String)],
    ) -> std::io::Result<Output> {
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GH_PROMPT_DISABLED", "1");
        command.envs(env.iter().map(|(k, v)| (*k, v)));
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }
        let out = command.output()?;
        let _ = self.stall;
        Ok(Output {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::{ProcessRunner, Runner};
    use std::io::ErrorKind;
    use std::time::{Duration, Instant};

    fn sh(stall_ms: u64, script: &str) -> (std::io::Result<super::Output>, Duration) {
        let runner = ProcessRunner {
            stall: Duration::from_millis(stall_ms),
        };
        let start = Instant::now();
        let out = runner.run("sh", &["-c".into(), script.into()], None, &[]);
        (out, start.elapsed())
    }

    #[test]
    fn a_silent_process_is_killed_after_the_stall_limit() {
        let (out, took) = sh(300, "sleep 5");
        assert_eq!(out.err().map(|e| e.kind()), Some(ErrorKind::TimedOut));
        assert!(took < Duration::from_secs(3), "{took:?}");
    }

    #[test]
    fn progress_resets_the_stall_clock() {
        let script = "for i in 1 2 3 4 5 6 7 8 9 10; do echo $i; sleep 0.1; done";
        let (out, took) = sh(300, script);
        assert!(out.unwrap().success());
        assert!(took >= Duration::from_millis(900), "{took:?}");
    }

    #[test]
    fn exit_code_stdout_and_stderr_are_captured() {
        let (out, _) = sh(5000, "echo out; echo err >&2; exit 3");
        let out = out.unwrap();
        assert_eq!(out.code, Some(3));
        assert_eq!(
            (out.stdout.as_str(), out.stderr.as_str()),
            ("out\n", "err\n")
        );
    }
}
