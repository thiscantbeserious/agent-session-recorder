//! `agr compress` end to end with an isolated HOME and config

use super::helpers::storage::create_codec_session;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tempfile::TempDir;

const CAST: &str =
    "{\"version\":3,\"term\":{\"cols\":80,\"rows\":24}}\n[0.5,\"o\",\"hello hello hello hello\"]\n";

struct Env {
    home: TempDir,
    storage: PathBuf,
}

impl Env {
    fn new() -> Self {
        let home = TempDir::new().unwrap();
        let storage = home.path().join("sessions");
        let cfg_dir = home.path().join(".config/agr");
        fs::create_dir_all(&cfg_dir).unwrap();
        let toml = format!("[storage]\ndirectory = \"{}\"\n", storage.display());
        fs::write(cfg_dir.join("config.toml"), toml).unwrap();
        Env { home, storage }
    }

    fn plain(&self, agent: &str, name: &str) -> PathBuf {
        create_codec_session(&self.storage, agent, name, CAST)
    }

    fn run(&self, args: &[&str]) -> (String, i32) {
        let out = Command::new(env!("CARGO_BIN_EXE_agr"))
            .arg("compress")
            .args(args)
            .env("NO_COLOR", "1")
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.home.path().join(".config"))
            .stdin(Stdio::null())
            .output()
            .expect("Failed to execute agr");
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        (stdout, out.status.code().unwrap_or(-1))
    }
}

#[test]
fn yes_compresses_and_prints_report() {
    let env = Env::new();
    let plain = env.plain("claude", "a.cast");
    let (stdout, code) = env.run(&["--yes"]);
    assert_eq!(code, 0, "stdout: {stdout:?}");
    assert!(!plain.exists(), "plain file should be gone");
    assert!(plain.with_extension("cast.gz").exists());
    assert!(stdout.contains("Compressed: 1"), "stdout: {stdout:?}");
}

#[test]
fn without_yes_and_no_tty_prints_preview_and_changes_nothing() {
    let env = Env::new();
    let plain = env.plain("claude", "a.cast");
    let (stdout, code) = env.run(&[]);
    assert_eq!(code, 0, "stdout: {stdout:?}");
    assert!(plain.exists(), "plain file must stay");
    assert!(!plain.with_extension("cast.gz").exists());
    assert!(
        stdout.contains("Sessions to compress: 1"),
        "stdout: {stdout:?}"
    );
    assert!(stdout.contains("Current size:"), "stdout: {stdout:?}");
    assert!(stdout.contains("Projected size:"), "stdout: {stdout:?}");
    assert!(stdout.contains("No changes made."), "stdout: {stdout:?}");
}

#[test]
fn agent_filter_only_touches_that_agent() {
    let env = Env::new();
    let claude = env.plain("claude", "a.cast");
    let codex = env.plain("codex", "b.cast");
    let (stdout, _) = env.run(&["--yes", "--agent", "claude"]);
    assert!(!claude.exists(), "stdout: {stdout:?}");
    assert!(codex.exists(), "stdout: {stdout:?}");
}

#[test]
fn nothing_to_compress_says_so() {
    let env = Env::new();
    let (stdout, code) = env.run(&["--yes"]);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("No plain sessions to compress."),
        "stdout: {stdout:?}"
    );
}

#[test]
fn second_run_is_idempotent() {
    let env = Env::new();
    env.plain("claude", "a.cast");
    env.run(&["--yes"]);
    let (stdout, _) = env.run(&["--yes"]);
    assert!(
        stdout.contains("No plain sessions to compress."),
        "stdout: {stdout:?}"
    );
}

#[test]
fn dry_run_prints_preview_and_changes_nothing_even_with_yes() {
    let env = Env::new();
    let plain = env.plain("claude", "a.cast");
    let (stdout, code) = env.run(&["--dry-run", "--yes"]);
    assert_eq!(code, 0, "stdout: {stdout:?}");
    assert!(stdout.contains("Sessions to compress: 1"), "{stdout:?}");
    assert!(
        !stdout.contains("Non-interactive"),
        "never prompts: {stdout:?}"
    );
    assert!(plain.exists());
    assert!(!plain.with_extension("cast.gz").exists());
}

#[test]
fn no_tty_run_prints_no_progress_on_stderr() {
    let env = Env::new();
    env.plain("claude", "a.cast");
    let out = Command::new(env!("CARGO_BIN_EXE_agr"))
        .args(["compress", "--yes"])
        .env("NO_COLOR", "1")
        .env("HOME", env.home.path())
        .env("XDG_CONFIG_HOME", env.home.path().join(".config"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("Compressing"), "{stderr:?}");
}

fn run_cmd(env: &Env, args: &[&str]) -> (String, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_agr"))
        .args(args)
        .env("NO_COLOR", "1")
        .env("HOME", env.home.path())
        .env("XDG_CONFIG_HOME", env.home.path().join(".config"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn status_shows_orphan_line_and_cleanup_yes_removes_it() {
    let env = Env::new();
    env.plain("claude", "a.cast");
    let orphan = env.storage.join("claude/.gone.cast.bak");
    fs::write(&orphan, "12345").unwrap();
    let (out, _) = run_cmd(&env, &["status"]);
    assert!(
        out.contains("1 orphaned backup(s), 5 B, remove with agr cleanup"),
        "{out}"
    );
    let (out, _) = run_cmd(&env, &["cleanup"]);
    assert!(orphan.exists(), "no tty and no --yes must keep it: {out}");
    run_cmd(&env, &["cleanup", "--yes"]);
    assert!(!orphan.exists());
    let (out, _) = run_cmd(&env, &["status"]);
    assert!(!out.contains("orphaned"), "{out}");
}

#[test]
fn compress_yes_reduces_status_total_and_drops_plain_line() {
    let env = Env::new();
    let big = format!(
        "{CAST}{}",
        "[0.5,\"o\",\"hello hello hello hello\"]\n".repeat(2000)
    );
    create_codec_session(&env.storage, "claude", "big.cast", &big);
    let (before, _) = run_cmd(&env, &["status"]);
    assert!(before.contains("1 plain"), "{before}");
    run_cmd(&env, &["compress", "--yes"]);
    let (after, _) = run_cmd(&env, &["status"]);
    assert!(!after.contains("plain,"), "{after}");
    let size = |s: &str| s.split("Sessions: ").nth(1).map(str::to_string);
    assert_ne!(size(&before), size(&after));
    assert!(after.contains("on disk"), "{after}");
}
