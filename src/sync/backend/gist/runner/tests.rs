use super::{ProcessRunner, Runner};
use std::io::ErrorKind;
use std::path::Path;
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

#[test]
fn run_watched_reports_stderr_lines_as_they_arrive_and_keeps_the_output() {
    let script = "printf 'Writing objects:  10%% (1/9)\\rWriting objects: 100%% (9/9), done.\\n' >&2; echo out";
    let runner = ProcessRunner::default();
    let mut seen = Vec::new();
    let args = ["-c".to_string(), script.to_string()];
    let out = runner
        .run_watched("sh", &args, None, &[], &mut |l| seen.push(l.to_string()))
        .unwrap();
    assert_eq!(
        seen,
        [
            "Writing objects:  10% (1/9)",
            "Writing objects: 100% (9/9), done."
        ]
    );
    assert_eq!(out.stdout, "out\n");
    assert!(out.stderr.contains("done."), "{}", out.stderr);
}

#[test]
fn run_to_file_streams_every_byte_value_unchanged() {
    let dest = tempfile::tempdir().unwrap().keep().join("out");
    let script = "i=0; while [ $i -lt 256 ]; do printf \"\\\\$(printf %03o $i)\"; i=$((i+1)); done";
    let args = ["-c".to_string(), script.to_string()];
    let out = ProcessRunner::default().run_to_file("sh", &args, &[], &dest);
    assert!(out.unwrap().stdout.is_empty());
    assert_eq!(
        std::fs::read(dest).unwrap(),
        (0..=255u8).collect::<Vec<u8>>()
    );
}

#[test]
fn a_grandchild_holding_the_pipe_does_not_block_the_result() {
    let (out, took) = sh(5000, "(sleep 30 &) ; echo done");
    assert_eq!(out.unwrap().stdout, "done\n");
    assert!(took < Duration::from_secs(10), "{took:?}");
}

/// A sink that refuses every write, like a full disk.
struct Full;

impl std::io::Write for Full {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("disk full"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn run_to_file_reports_a_failed_write() {
    let args = ["-c".to_string(), "echo data".to_string()];
    let child = super::spawn("sh", &args, None, &[]).unwrap();
    let stall = Duration::from_secs(5);
    let res = super::supervise(child, "sh", stall, &mut |_| {}, Some(Box::new(Full)));
    assert!(res.is_err());
}

/// Implements only `run`, so `run_to_file` is the trait default.
struct TextOnly;

impl Runner for TextOnly {
    fn run(
        &self,
        _: &str,
        _: &[String],
        _: Option<&Path>,
        _: &[(&str, String)],
    ) -> std::io::Result<super::Output> {
        Ok(super::Output {
            code: Some(0),
            stdout: "lossy".into(),
            stderr: String::new(),
        })
    }
}

#[test]
fn default_run_to_file_is_unsupported() {
    let dest = tempfile::tempdir().unwrap().keep().join("out");
    let err = TextOnly.run_to_file("x", &[], &[], &dest).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unsupported);
}
