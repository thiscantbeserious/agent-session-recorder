//! Auto-migrate offer and status savings line through the real binary, isolated HOME

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

    fn state_file(&self) -> PathBuf {
        self.home.path().join(".config/agr/state.toml")
    }

    /// Run without a TTY (stdin null, stdout piped), returns stdout.
    fn run(&self, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_agr"))
            .args(args)
            .env("NO_COLOR", "1")
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.home.path().join(".config"))
            .stdin(Stdio::null())
            .output()
            .expect("Failed to execute agr");
        String::from_utf8_lossy(&out.stdout).to_string()
    }
}

#[test]
fn status_shows_plain_count_and_projected_savings() {
    let env = Env::new();
    env.plain("claude", "a.cast");
    let stdout = env.run(&["status"]);
    assert!(stdout.contains("1 plain, ~"), "stdout: {stdout:?}");
    assert!(
        stdout.contains("saved by agr compress"),
        "stdout: {stdout:?}"
    );
}

#[test]
fn status_without_plain_sessions_has_no_savings_line() {
    let env = Env::new();
    let stdout = env.run(&["status"]);
    assert!(
        !stdout.contains("saved by agr compress"),
        "stdout: {stdout:?}"
    );
}

#[test]
fn no_tty_never_prompts_rewrites_or_writes_state() {
    let env = Env::new();
    let plain = env.plain("claude", "a.cast");
    for args in [&["status"][..], &["list"], &["ls"], &["cleanup"]] {
        let stdout = env.run(args);
        assert!(
            !stdout.contains("Compress 1 session(s)?"),
            "{args:?}: {stdout:?}"
        );
        assert!(plain.exists(), "{args:?} must not rewrite");
        assert!(
            !env.state_file().exists(),
            "{args:?} must not consume state"
        );
    }
}
