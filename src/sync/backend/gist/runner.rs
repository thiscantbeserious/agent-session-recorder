//! Process runner behind a small trait, the only place that spawns `gh` and `git`.

use std::io::{ErrorKind, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(100);

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
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let child = command.spawn()?;
        supervise(child, program, self.stall)
    }
}

/// Drain both pipes in threads that stamp every read, kill the child once nothing was read
/// for `stall`.
fn supervise(mut child: Child, program: &str, stall: Duration) -> std::io::Result<Output> {
    let last = Arc::new(Mutex::new(Instant::now()));
    let stdout = drain(child.stdout.take(), &last);
    let stderr = drain(child.stderr.take(), &last);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        let silent = last
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .elapsed();
        if silent >= stall {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                ErrorKind::TimedOut,
                format!("{program} printed nothing for {} s", stall.as_secs_f32()),
            ));
        }
        std::thread::sleep(POLL);
    };
    let text = |h: thread::JoinHandle<Vec<u8>>| {
        String::from_utf8_lossy(&h.join().unwrap_or_default()).into_owned()
    };
    Ok(Output {
        code: status.code(),
        stdout: text(stdout),
        stderr: text(stderr),
    })
}

fn drain<R: Read + Send + 'static>(
    pipe: Option<R>,
    last: &Arc<Mutex<Instant>>,
) -> thread::JoinHandle<Vec<u8>> {
    let last = Arc::clone(last);
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut buf = [0u8; 8192];
        let Some(mut pipe) = pipe else { return bytes };
        while let Ok(n) = pipe.read(&mut buf) {
            if n == 0 {
                break;
            }
            *last.lock().unwrap_or_else(PoisonError::into_inner) = Instant::now();
            bytes.extend_from_slice(&buf[..n]);
        }
        bytes
    })
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
