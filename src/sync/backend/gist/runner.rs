//! Process runner behind a small trait, the only place that spawns `gh` and `git`.

use std::io::{ErrorKind, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(100);
/// How long to wait for a pipe to close after the child exited.
const GRACE: Duration = Duration::from_secs(2);

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

    /// Like `run`, stdout goes byte for byte into the file `dest`, `Output.stdout` stays empty.
    /// The default is `Unsupported`, the lossy text of `run` would corrupt binary parts.
    fn run_to_file(
        &self,
        program: &str,
        args: &[String],
        env: &[(&str, String)],
        dest: &Path,
    ) -> std::io::Result<Output> {
        let _ = (program, args, env, dest);
        Err(ErrorKind::Unsupported.into())
    }

    /// Like `run`, `on_line` also sees every line the process prints on stderr while it runs.
    /// `\r` and `\n` both end a line, git redraws its progress with `\r`. The default ignores them.
    fn run_watched(
        &self,
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
        env: &[(&str, String)],
        on_line: &mut dyn FnMut(&str),
    ) -> std::io::Result<Output> {
        let _ = on_line;
        self.run(program, args, cwd, env)
    }
}

/// Cuts a byte stream into lines at `\r` and `\n`, an unfinished line waits for more bytes.
#[derive(Default)]
struct Lines {
    tail: Vec<u8>,
}

impl Lines {
    /// Calls `f` for every line the chunk completes, empty lines are skipped.
    fn push(&mut self, chunk: &[u8], f: &mut dyn FnMut(&str)) {
        for byte in chunk {
            if matches!(byte, b'\r' | b'\n') {
                self.finish(f);
            } else {
                self.tail.push(*byte);
            }
        }
    }

    /// The last line when the stream ended without a line break.
    fn finish(&mut self, f: &mut dyn FnMut(&str)) {
        if !self.tail.is_empty() {
            f(&String::from_utf8_lossy(&self.tail));
            self.tail.clear();
        }
    }
}

/// Longest a process may stay silent on stdout and stderr before it counts as stalled.
// ponytail: fixed 120 s of silence, any output resets it, make it a config key if slow links need more
const STALL_LIMIT: Duration = Duration::from_secs(120);

/// Where `run_to_file` puts stdout, a file in real runs.
type Sink = Box<dyn Write + Send>;

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
        self.run_watched(program, args, cwd, env, &mut |_| {})
    }

    fn run_to_file(
        &self,
        program: &str,
        args: &[String],
        env: &[(&str, String)],
        dest: &Path,
    ) -> std::io::Result<Output> {
        let file = std::fs::File::create(dest)?;
        let child = spawn(program, args, None, env)?;
        watch_child(
            child,
            program,
            self.stall,
            &mut |_| {},
            Some(Box::new(file)),
        )
    }

    fn run_watched(
        &self,
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
        env: &[(&str, String)],
        on_line: &mut dyn FnMut(&str),
    ) -> std::io::Result<Output> {
        let child = spawn(program, args, cwd, env)?;
        watch_child(child, program, self.stall, on_line, None)
    }
}

/// Spawns `program` with piped output, for the runner tests only.
#[doc(hidden)]
pub fn spawn(
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    env: &[(&str, String)],
) -> std::io::Result<Child> {
    {
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
        command.spawn()
    }
}

/// Drain both pipes in threads that stamp every read, kill the child once nothing was read
/// for `stall`. New stderr bytes go to `on_line` on every poll and once more after the exit.
/// With a `sink` stdout is written into it as it arrives and the output text stays empty.
#[doc(hidden)]
pub fn watch_child(
    mut child: Child,
    program: &str,
    stall: Duration,
    on_line: &mut dyn FnMut(&str),
    sink: Option<Sink>,
) -> std::io::Result<Output> {
    let last = Arc::new(Mutex::new(Instant::now()));
    let stdout = drain(child.stdout.take(), &last, sink);
    let failed = Arc::clone(&stdout.failed);
    let stderr = drain(child.stderr.take(), &last, None);
    let mut watcher = Watcher::default();
    let status = loop {
        watcher.feed(&stderr.bytes, on_line);
        if let Some(status) = child.try_wait()? {
            break status;
        }
        // the drain stopped at a failed write, that is the cause, not the silence that follows
        if let Some(e) = failed.lock().unwrap_or_else(PoisonError::into_inner).take() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
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
    let (stdout, stderr_text) = (
        finish(stdout),
        finish_watched(stderr, &mut watcher, on_line),
    );
    if let Some(e) = failed.lock().unwrap_or_else(PoisonError::into_inner).take() {
        return Err(e);
    }
    Ok(Output {
        code: status.code(),
        stdout,
        stderr: stderr_text,
    })
}

/// How far into the stderr bytes the lines were cut.
#[derive(Default)]
struct Watcher {
    seen: usize,
    lines: Lines,
}

impl Watcher {
    fn feed(&mut self, bytes: &Mutex<Vec<u8>>, on_line: &mut dyn FnMut(&str)) {
        let all = bytes.lock().unwrap_or_else(PoisonError::into_inner);
        self.lines.push(&all[self.seen..], on_line);
        self.seen = all.len();
    }
}

/// `finish`, then the bytes that came after the last poll and the unterminated last line.
fn finish_watched(drain: Drain, watcher: &mut Watcher, on_line: &mut dyn FnMut(&str)) -> String {
    let _ = drain.eof.recv_timeout(GRACE);
    watcher.feed(&drain.bytes, on_line);
    watcher.lines.finish(on_line);
    let bytes = drain.bytes.lock().unwrap_or_else(PoisonError::into_inner);
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Bytes read so far and a signal that the pipe hit EOF.
struct Drain {
    bytes: Arc<Mutex<Vec<u8>>>,
    eof: mpsc::Receiver<()>,
    /// The write error of the sink, the drain stops at the first one.
    failed: Arc<Mutex<Option<std::io::Error>>>,
}

/// Text read, waits `GRACE` for EOF, a grandchild that kept the pipe open is left behind.
fn finish(drain: Drain) -> String {
    let _ = drain.eof.recv_timeout(GRACE);
    let bytes = drain.bytes.lock().unwrap_or_else(PoisonError::into_inner);
    String::from_utf8_lossy(&bytes).into_owned()
}

fn drain<R: Read + Send + 'static>(
    pipe: Option<R>,
    last: &Arc<Mutex<Instant>>,
    mut sink: Option<Sink>,
) -> Drain {
    let last = Arc::clone(last);
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let (done, eof) = mpsc::channel();
    let failed = Arc::new(Mutex::new(None));
    let sink_failed = Arc::clone(&failed);
    let shared = Arc::clone(&bytes);
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let Some(mut pipe) = pipe else { return };
        loop {
            let n = match pipe.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            };
            *last.lock().unwrap_or_else(PoisonError::into_inner) = Instant::now();
            if let Some(file) = sink.as_mut() {
                if let Err(e) = file.write_all(&buf[..n]) {
                    *sink_failed.lock().unwrap_or_else(PoisonError::into_inner) = Some(e);
                    break;
                }
                continue;
            }
            let mut all = shared.lock().unwrap_or_else(PoisonError::into_inner);
            all.extend_from_slice(&buf[..n]);
        }
        let _ = done.send(());
    });
    Drain { bytes, eof, failed }
}

#[cfg(test)]
mod lines_tests {
    use super::Lines;

    fn cut(chunks: &[&[u8]]) -> Vec<String> {
        let (mut lines, mut got) = (Lines::default(), Vec::new());
        for chunk in chunks {
            lines.push(chunk, &mut |l| got.push(l.to_string()));
        }
        lines.finish(&mut |l| got.push(l.to_string()));
        got
    }

    #[test]
    fn carriage_returns_and_newlines_both_end_a_line() {
        let got =
            cut(&[b"Writing objects:  10% (1/9)\rWriting objects: 100% (9/9), done.\nTotal 9\r\n"]);
        assert_eq!(
            got,
            [
                "Writing objects:  10% (1/9)",
                "Writing objects: 100% (9/9), done.",
                "Total 9"
            ]
        );
    }

    #[test]
    fn a_line_split_over_chunks_is_joined_and_the_last_one_is_flushed() {
        assert_eq!(
            cut(&[b"Writing obj", b"ects:  4", b"5% (1/3)\rtail"]),
            ["Writing objects:  45% (1/3)", "tail"]
        );
    }

    #[test]
    fn a_multibyte_char_cut_in_two_chunks_survives() {
        let bytes = "ä\r".as_bytes();
        assert_eq!(cut(&[&bytes[..1], &bytes[1..]]), ["ä"]);
    }
}

#[cfg(test)]
mod drain_tests {
    use super::{drain, finish};
    use std::io::{ErrorKind, Read};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    /// "ab", then one `Interrupted`, then "cd", like a signal arriving mid-read.
    struct Hiccup(u8);

    impl Read for Hiccup {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            self.0 += 1;
            let chunk: &[u8] = match self.0 {
                1 => b"ab",
                2 => return Err(ErrorKind::Interrupted.into()),
                3 => b"cd",
                _ => b"",
            };
            out[..chunk.len()].copy_from_slice(chunk);
            Ok(chunk.len())
        }
    }

    #[test]
    fn an_interrupted_read_does_not_end_the_drain() {
        let last = Arc::new(Mutex::new(Instant::now()));
        assert_eq!(finish(drain(Some(Hiccup(0)), &last, None)), "abcd");
    }
}
