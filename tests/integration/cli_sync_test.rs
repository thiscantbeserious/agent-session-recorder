//! `agr sync ...` end to end against the file based fake backend (AGR_SYNC_TEST_BACKEND_DIR).
//! Nothing here talks to GitHub, the fake `gh` scripts only exist to prove what agr spawns.

use agr::files::codec;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;

struct Out {
    stdout: String,
    stderr: String,
    code: i32,
}

impl Out {
    fn success(&self) -> bool {
        self.code == 0
    }

    fn all(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

struct Machine {
    home: TempDir,
    storage: PathBuf,
    remote: PathBuf,
}

impl Machine {
    fn new(remote: &Path, config: &str) -> Self {
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

    fn gist(remote: &Path) -> Self {
        Self::new(remote, "[sync]\nbackend = \"gist\"\n")
    }

    fn session(&self, agent: &str, name: &str, text: &str) -> PathBuf {
        let dir = self.storage.join(agent);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        codec::atomic_write(&path, |w| Ok(w.write_all(text.as_bytes())?)).unwrap();
        path
    }

    fn base(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_agr"));
        cmd.arg("sync")
            .args(args)
            .env("NO_COLOR", "1")
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.home.path().join(".config"))
            .env("AGR_SYNC_TEST_BACKEND_DIR", &self.remote);
        cmd
    }

    fn run(&self, args: &[&str]) -> Out {
        self.exec(self.base(args), None)
    }

    fn run_stdin(&self, args: &[&str], input: &str) -> Out {
        self.exec(self.base(args), Some(input))
    }

    fn exec(&self, mut cmd: Command, input: Option<&str>) -> Out {
        cmd.stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        if let Some(text) = input {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(text.as_bytes())
                .unwrap();
        }
        let out = child.wait_with_output().unwrap();
        Out {
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            code: out.status.code().unwrap_or(-1),
        }
    }

    fn key_path(&self) -> PathBuf {
        self.home.path().join(".config/agr/sync.key")
    }

    fn state(&self) -> String {
        fs::read_to_string(self.home.path().join(".config/agr/state.toml")).unwrap_or_default()
    }
}

/// Files of every pack directory of the directory store whose name ends with `suffix`.
fn remote_files(remote: &Path, suffix: &str) -> Vec<PathBuf> {
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

fn remote() -> TempDir {
    TempDir::new().unwrap()
}

#[test]
fn sync_off_prints_how_to_enable_and_spawns_nothing() {
    let remote = remote();
    let m = Machine::new(remote.path(), "[sync]\nbackend = \"off\"\n");
    let empty_path = TempDir::new().unwrap();
    for sub in ["status", "push", "pull"] {
        let mut cmd = m.base(&[sub]);
        cmd.env("PATH", empty_path.path())
            .env_remove("AGR_SYNC_TEST_BACKEND_DIR");
        let out = m.exec(cmd, None);
        assert_ne!(out.code, 0, "{sub}");
        assert!(
            out.all().contains("[sync] backend = \"gist\""),
            "{sub}: {}",
            out.all()
        );
    }
}

#[cfg(unix)]
#[test]
fn status_without_gh_and_git_names_both_tools_and_the_install_hint() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    let empty_path = TempDir::new().unwrap();
    for sub in ["status", "push", "pull"] {
        let out = gist_cmd(&m, &empty_path, &[sub]);
        assert_eq!(out.code, 1, "{sub}: {}", out.all());
        let all = out.all();
        assert!(all.contains("gh and git"), "{sub}: {all}");
        assert!(all.contains("https://cli.github.com"), "{sub}: {all}");
        assert!(all.contains("gh auth login"), "{sub}: {all}");
        assert!(!all.contains("panicked"), "{sub}: {all}");
    }
}

/// A recording with the default config and no `gh` or `git` on PATH works and prints no sync hint.
#[cfg(unix)]
#[test]
fn recording_without_gh_and_git_prints_no_sync_hint() {
    let Ok(asciinema) = which_asciinema() else {
        return;
    };
    let remote = remote();
    let m = Machine::new(remote.path(), "");
    let bin = TempDir::new().unwrap();
    for (name, target) in [("asciinema", asciinema.as_str()), ("echo", "/bin/echo")] {
        std::os::unix::fs::symlink(target, bin.path().join(name)).unwrap();
    }
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_agr"));
    cmd.args(["record", "echo", "--", "hi"])
        .env("NO_COLOR", "1")
        .env("HOME", m.home.path())
        .env("XDG_CONFIG_HOME", m.home.path().join(".config"))
        .env("PATH", bin.path());
    let out = m.exec(cmd, None);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(!out.all().to_lowercase().contains("sync"), "{}", out.all());
    assert!(!out.all().contains("auto push"), "{}", out.all());
    assert!(m.storage.join("echo").is_dir(), "{}", out.all());
}

#[cfg(unix)]
fn which_asciinema() -> Result<String, ()> {
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
fn fake_bin(scripts: &[(&str, &str)]) -> TempDir {
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
fn gist_cmd(m: &Machine, bin: &TempDir, args: &[&str]) -> Out {
    let mut cmd = m.base(args);
    cmd.env("PATH", bin.path())
        .env_remove("AGR_SYNC_TEST_BACKEND_DIR");
    m.exec(cmd, None)
}

#[cfg(unix)]
#[test]
fn missing_gh_gives_the_install_hint_and_changes_nothing() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "text");
    let bin = fake_bin(&[("git", "exit 0")]);
    let out = gist_cmd(&m, &bin, &["push", "--yes"]);
    assert_ne!(out.code, 0);
    assert!(out.all().contains("gh not found"), "{}", out.all());
    assert!(
        out.all().contains("https://cli.github.com"),
        "{}",
        out.all()
    );
    assert!(!m.key_path().exists());
    assert_eq!(m.state(), "");
}

#[cfg(unix)]
#[test]
fn not_logged_in_gives_the_login_hint_and_changes_nothing() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "text");
    let bin = fake_bin(&[
        ("git", "exit 0"),
        ("gh", "[ \"$1\" = auth ] && exit 1\nexit 0"),
    ]);
    let out = gist_cmd(&m, &bin, &["status"]);
    assert_ne!(out.code, 0);
    assert!(out.all().contains("gh not logged in"), "{}", out.all());
    assert!(out.all().contains("gh auth login"), "{}", out.all());
    assert!(!m.key_path().exists());
}

#[test]
fn no_tty_without_yes_aborts_push_and_pull_but_dry_run_and_status_work() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "text");
    let out = m.run(&["push"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(out.stdout.contains("--yes"), "{}", out.stdout);
    assert!(out.stdout.contains("No changes made."), "{}", out.stdout);
    assert!(remote_files(remote.path(), "manifest").is_empty());
    assert!(!m.key_path().exists());
    assert_eq!(m.state(), "");
    let dry = m.run(&["push", "--dry-run"]);
    assert_eq!(dry.code, 0, "{}", dry.all());
    assert!(
        dry.stdout.contains("Would push 1 session(s)"),
        "{}",
        dry.stdout
    );
    assert!(!dry.stdout.contains("claude/a.cast.gz"), "{}", dry.stdout);
    let full = m.run(&["push", "--dry-run", "-v"]);
    assert!(
        full.stdout.contains("  claude/a.cast.gz ("),
        "{}",
        full.stdout
    );
    assert!(!m.key_path().exists());
    assert!(remote_files(remote.path(), "manifest").is_empty());
    let status = m.run(&["status"]);
    assert_eq!(status.code, 0, "{}", status.all());
    assert!(status.stdout.contains("1 to push"), "{}", status.stdout);
    assert!(!m.key_path().exists());
}

#[test]
fn first_push_warns_stores_ack_makes_a_key_and_uploads_ciphertext() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "secret session text");
    let out = m.run(&["push", "--yes"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(
        out.stdout.contains("anyone with the link"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("agr sync key export"), "{}", out.stdout);
    assert!(out.stdout.contains("Pushed: 1"), "{}", out.stdout);
    assert!(
        m.state().contains("sync_privacy_ack = true"),
        "{}",
        m.state()
    );
    assert!(m.key_path().exists());
    assert_eq!(remote_files(remote.path(), "manifest").len(), 1);
    let parts = remote_files(remote.path(), "part.000000");
    assert_eq!(parts.len(), 1);
    let blob = fs::read(&parts[0]).unwrap();
    assert!(!blob.starts_with(&[0x1f, 0x8b]));
    assert!(!String::from_utf8_lossy(&blob).contains("secret session text"));
    let again = m.run(&["push", "--yes"]);
    assert!(
        again.stdout.contains("Nothing to push."),
        "{}",
        again.stdout
    );
    assert!(
        !again.stdout.contains("anyone with the link"),
        "{}",
        again.stdout
    );
}

#[test]
fn push_aborts_with_import_hint_when_remote_has_sessions_and_no_key() {
    let remote = remote();
    let a = Machine::gist(remote.path());
    a.session("claude", "a.cast.gz", "one");
    assert_eq!(a.run(&["push", "--yes"]).code, 0);
    let b = Machine::gist(remote.path());
    b.session("claude", "b.cast.gz", "two");
    let out = b.run(&["push", "--yes"]);
    assert_ne!(out.code, 0);
    assert!(out.all().contains("agr sync key import"), "{}", out.all());
    assert!(!b.key_path().exists());
    assert_eq!(remote_files(remote.path(), "manifest").len(), 1);
}

#[test]
fn pull_without_a_key_aborts_with_the_import_hint() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    let out = m.run(&["pull", "--yes"]);
    assert_ne!(out.code, 0);
    assert!(out.all().contains("agr sync key import"), "{}", out.all());
}

#[test]
fn key_export_and_import_move_sessions_to_a_second_machine() {
    let remote = remote();
    let a = Machine::gist(remote.path());
    let original = a.session("claude", "a.cast.gz", "one two three");
    assert_eq!(a.run(&["push", "--yes"]).code, 0);
    let export = a.run(&["key", "export"]);
    assert_eq!(export.code, 0, "{}", export.all());
    let phrase = export
        .stdout
        .lines()
        .find(|l| l.trim().starts_with("agr1-"))
        .unwrap()
        .trim()
        .to_string();
    let b = Machine::gist(remote.path());
    let bad = b.run(&["key", "import", "agr1-AAAAA"]);
    assert_ne!(bad.code, 0);
    assert!(!b.key_path().exists());
    let imported = b.run_stdin(&["key", "import"], &format!("{phrase}\n"));
    assert_eq!(imported.code, 0, "{}", imported.all());
    assert!(b.key_path().exists());
    let pulled = b.run(&["pull", "--yes"]);
    assert_eq!(pulled.code, 0, "{}", pulled.all());
    assert!(pulled.stdout.contains("Pulled: 1"), "{}", pulled.stdout);
    let copy = fs::read(b.storage.join("claude/a.cast.gz")).unwrap();
    assert_eq!(copy, fs::read(original).unwrap());
}

#[test]
fn key_import_takes_the_phrase_as_argument_and_guards_a_different_key() {
    let remote = remote();
    let a = Machine::gist(remote.path());
    a.session("claude", "a.cast.gz", "x");
    a.run(&["push", "--yes"]);
    let phrase = a.run(&["key", "export"]).stdout;
    let phrase = phrase
        .lines()
        .find(|l| l.trim().starts_with("agr1-"))
        .unwrap()
        .trim()
        .to_string();
    let b = Machine::gist(remote.path());
    assert_eq!(b.run(&["key", "import", &phrase]).code, 0);
    assert_eq!(
        b.run(&["key", "import", &phrase]).code,
        0,
        "same key is fine"
    );
    let other_remote = TempDir::new().unwrap();
    let c2 = Machine::gist(other_remote.path());
    c2.session("codex", "c.cast.gz", "y");
    c2.run(&["push", "--yes"]);
    let other = c2.run(&["key", "export"]).stdout;
    let other = other
        .lines()
        .find(|l| l.trim().starts_with("agr1-"))
        .unwrap()
        .trim()
        .to_string();
    let refused = b.run(&["key", "import", &other]);
    assert_ne!(refused.code, 0);
    assert!(refused.all().contains("--force"), "{}", refused.all());
    assert_eq!(b.run(&["key", "import", "--force", &other]).code, 0);
}

#[test]
fn status_lists_states_totals_without_colour_and_shows_the_auto_push_error() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "one");
    assert_eq!(m.run(&["push", "--yes"]).code, 0);
    m.session("claude", "b.cast.gz", "two");
    m.session("claude", "plain.cast", "plain");
    let state = m.home.path().join(".config/agr/state.toml");
    fs::write(
        &state,
        "sync_privacy_ack = true\nlast_auto_push_error = \"offline\"\n",
    )
    .unwrap();
    let out = m.run(&["status", "-v"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(!out.all().contains('\u{1b}'));
    assert!(
        out.stdout
            .contains("Backend: directory, 1 remote session(s)"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("claude/a.cast.gz in sync"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("claude/b.cast.gz local only (push)"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("agr compress"), "{}", out.stdout);
    assert!(out.stdout.contains("Totals:"), "{}", out.stdout);
    assert!(
        out.all().contains("Last automatic push failed: offline"),
        "{}",
        out.all()
    );
    let again = m.run(&["status"]);
    assert!(
        !again.all().contains("Last automatic push failed"),
        "{}",
        again.all()
    );
    assert!(!m.state().contains("offline"));
}

#[test]
fn default_status_is_a_compact_summary_and_verbose_lists_sessions() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "one");
    m.session("claude", "plain.cast", "plain");
    let compact = m.run(&["status"]);
    assert_eq!(compact.code, 0, "{}", compact.all());
    for want in [
        "1 to push (",
        "Run agr sync status -v for the per-session list.",
    ] {
        assert!(compact.stdout.contains(want), "{}", compact.stdout);
    }
    assert!(
        !compact.stdout.contains("claude/a.cast.gz"),
        "{}",
        compact.stdout
    );
    let verbose = m.run(&["status", "--verbose"]);
    assert!(verbose
        .stdout
        .contains("claude/a.cast.gz local only (push)"));
    assert!(!verbose.stdout.contains("per-session list"));
}

#[test]
fn conflicts_are_skipped_until_overwrite_and_pull_keeps_a_backup() {
    let remote = remote();
    let a = Machine::gist(remote.path());
    a.session("claude", "a.cast.gz", "base");
    a.run(&["push", "--yes"]);
    let phrase = a.run(&["key", "export"]).stdout;
    let phrase = phrase
        .lines()
        .find(|l| l.trim().starts_with("agr1-"))
        .unwrap()
        .trim()
        .to_string();
    let b = Machine::gist(remote.path());
    b.run(&["key", "import", &phrase]);
    b.run(&["pull", "--yes"]);
    a.session("claude", "a.cast.gz", "changed on a");
    b.session("claude", "a.cast.gz", "changed on b");
    assert_eq!(a.run(&["push", "--yes"]).code, 0);
    let out = b.run(&["push", "--yes"]);
    assert!(out.stdout.contains("Conflicts: 1"), "{}", out.stdout);
    assert!(
        out.stdout.contains("local ") && out.stdout.contains("remote "),
        "{}",
        out.stdout
    );
    let theirs = b.run(&["pull", "--yes", "--overwrite"]);
    assert!(theirs.stdout.contains("Pulled: 1"), "{}", theirs.stdout);
    assert!(b.storage.join("claude/.a.cast.gz.bak").exists());
}

#[test]
fn delete_removes_the_remote_and_keeps_the_local_file() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    let local = m.session("claude", "a.cast.gz", "one");
    m.run(&["push", "--yes"]);
    let dry = m.run(&["delete", "--dry-run", "claude/a.cast.gz"]);
    assert_eq!(dry.code, 0, "{}", dry.all());
    assert_eq!(remote_files(remote.path(), "manifest").len(), 1);
    let no_yes = m.run(&["delete", "claude/a.cast.gz"]);
    assert!(
        no_yes.stdout.contains("No changes made."),
        "{}",
        no_yes.stdout
    );
    assert_eq!(remote_files(remote.path(), "manifest").len(), 1);
    let out = m.run(&["delete", "--yes", "claude/a.cast.gz"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(out.stdout.contains("Deleted: 1"), "{}", out.stdout);
    // a tombstone adds no bytes, the data part stays until prune
    assert_eq!(remote_files(remote.path(), "part.000000").len(), 1);
    let status = m.run(&["status", "-v"]);
    assert!(
        status.stdout.contains("deleted on the remote"),
        "{}",
        status.stdout
    );
    assert!(local.exists());
}

#[test]
fn auto_push_pushes_one_session_quietly_without_prompt_or_privacy_notice() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "one");
    m.session("claude", "b.cast.gz", "two");
    assert_eq!(m.run(&["push", "--yes", "claude/a.cast.gz"]).code, 0);
    let out = m.run(&["push", "--auto", "claude/b.cast.gz"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert_eq!(out.all(), "", "auto push prints nothing");
    assert_eq!(remote_files(remote.path(), "part.000001").len(), 1);
    assert!(!m.state().contains("last_auto_push_error"), "{}", m.state());
}

#[test]
fn auto_push_without_privacy_ack_pushes_nothing_and_stays_quiet() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "one");
    let out = m.run(&["push", "--auto", "claude/a.cast.gz"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(remote_files(remote.path(), "manifest").is_empty());
    assert!(!m.key_path().exists());
    assert_eq!(out.all(), "");
    assert_eq!(m.state(), "");
}

#[cfg(unix)]
#[test]
fn auto_push_failure_is_stored_one_line_and_shown_once_by_status() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "one");
    assert_eq!(m.run(&["push", "--yes"]).code, 0);
    m.session("claude", "b.cast.gz", "two");
    let bin = fake_bin(&[("git", "exit 0"), ("gh", "exit 1")]);
    let mut cmd = m.base(&["push", "--auto", "claude/b.cast.gz"]);
    cmd.env("PATH", bin.path())
        .env_remove("AGR_SYNC_TEST_BACKEND_DIR");
    let out = m.exec(cmd, None);
    assert_eq!(out.code, 0, "{}", out.all());
    let state = m.state();
    let line = state
        .lines()
        .find(|l| l.starts_with("last_auto_push_error"))
        .unwrap();
    assert!(line.contains("gh"), "{line}");
    let status = m.run(&["status"]);
    assert!(
        status.all().contains("Last automatic push failed"),
        "{}",
        status.all()
    );
    assert!(!m.state().contains("last_auto_push_error"));
}

const CAST: &str =
    "{\"version\":3,\"term\":{\"cols\":80,\"rows\":24}}\n[0.5,\"o\",\"a\"]\n[10.0,\"o\",\"b\"]\n";

fn phrase_of(m: &Machine) -> String {
    let out = m.run(&["key", "export"]).stdout;
    let line = out.lines().find(|l| l.trim().starts_with("agr1-")).unwrap();
    line.trim().to_string()
}

#[cfg(unix)]
fn all_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            all_files(&path, found);
        } else {
            found.push(path);
        }
    }
}

#[cfg(unix)]
#[test]
fn a_token_printed_by_gh_never_reaches_output_or_files() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "one");
    let gh = "echo \"  - Token: gho_FAKE123\"; echo \"  - Token scopes: 'gist'\"; \
        if [ \"$1\" = auth ]; then echo \"Token: gho_FAKE123\" >&2; exit 0; fi\n\
        if [ \"$1\" = api ]; then echo '[]'; exit 0; fi\n\
        echo \"HTTP 500 token gho_FAKE123 rejected\" >&2; exit 1";
    let bin = fake_bin(&[("git", "exit 0"), ("gh", gh)]);
    let mut seen = String::new();
    for args in [&["status"][..], &["push", "--yes"], &["pull", "--yes"]] {
        let out = gist_cmd(&m, &bin, args);
        seen.push_str(&out.all());
    }
    let mut files = Vec::new();
    all_files(m.home.path(), &mut files);
    for file in files {
        seen.push_str(&String::from_utf8_lossy(&fs::read(file).unwrap()));
    }
    assert!(!seen.contains("gho_FAKE123"), "{seen}");
}

#[test]
fn pull_without_tty_and_without_yes_aborts_and_changes_nothing() {
    let remote = remote();
    let a = Machine::gist(remote.path());
    a.session("claude", "a.cast.gz", "one");
    assert_eq!(a.run(&["push", "--yes"]).code, 0);
    let b = Machine::gist(remote.path());
    assert_eq!(b.run(&["key", "import", &phrase_of(&a)]).code, 0);
    let out = b.run(&["pull"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(out.stdout.contains("No changes made."), "{}", out.stdout);
    assert!(!b.storage.join("claude/a.cast.gz").exists());
    assert!(!b.storage.join(".agr-sync.json").exists());
    assert_eq!(b.state(), "");
}

#[test]
fn pulled_cast_gz_plays_without_warning_and_is_listed() {
    let remote = remote();
    let a = Machine::gist(remote.path());
    a.session("claude", "a.cast.gz", CAST);
    assert_eq!(a.run(&["push", "--yes"]).code, 0);
    let b = Machine::gist(remote.path());
    b.run(&["key", "import", &phrase_of(&a)]);
    assert_eq!(b.run(&["pull", "--yes"]).code, 0);
    let file = b.storage.join("claude/a.cast.gz");
    let agr = |args: &[&str]| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_agr"));
        cmd.args(args)
            .env("NO_COLOR", "1")
            .env("HOME", b.home.path())
            .env("XDG_CONFIG_HOME", b.home.path().join(".config"));
        b.exec(cmd, None)
    };
    let play = agr(&["play", file.to_str().unwrap()]);
    assert!(
        !play
            .all()
            .contains("does not have .cast or .cast.gz extension"),
        "{}",
        play.all()
    );
    assert!(
        !play.all().to_lowercase().contains("corrupt"),
        "{}",
        play.all()
    );
    let list = agr(&["list"]);
    assert_eq!(list.code, 0, "{}", list.all());
    assert!(list.stdout.contains("a.cast"), "{}", list.stdout);
}

#[test]
fn test_backend_dir_is_announced_on_stderr() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    let out = m.run(&["status"]);
    let line = format!("Using test sync backend at {}", remote.path().display());
    assert!(out.stderr.contains(&line), "{}", out.stderr);
}

#[test]
fn delete_warns_that_a_plain_push_uploads_the_session_again() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "one");
    m.run(&["push", "--yes"]);
    let out = m.run(&["delete", "--yes", "claude/a.cast.gz"]);
    assert!(
        out.stdout
            .contains("A later `agr sync push` uploads the session again."),
        "{}",
        out.stdout
    );
}

#[test]
fn push_prints_no_progress_lines_when_stderr_is_not_a_terminal() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "text");
    m.session("claude", "b.cast.gz", "other");
    let out = m.run(&["push", "--yes"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(out.stdout.contains("Pushed: 2"), "{}", out.stdout);
    assert!(!out.all().contains("[1/2]"), "{}", out.all());
    assert!(!out.all().contains("pushed claude/"), "{}", out.all());
}

#[test]
fn compact_preview_by_default_and_the_full_list_with_dash_v_on_push_pull_and_delete() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "one");
    m.session("claude", "b.cast.gz", "two");
    m.session("claude", "plain.cast", "{}");
    let push = m.run(&["push", "--dry-run"]);
    assert!(
        push.stdout.contains("Would push 2 session(s)"),
        "{}",
        push.stdout
    );
    assert!(
        push.stdout.contains("1 skipped: plain .cast"),
        "{}",
        push.stdout
    );
    assert!(!push.stdout.contains("claude/b.cast.gz"), "{}", push.stdout);
    assert!(m
        .run(&["push", "--dry-run", "-v"])
        .stdout
        .contains("  claude/b.cast.gz ("));
    m.run(&["push", "--yes"]);
    let delete = m.run(&[
        "delete",
        "--dry-run",
        "claude/a.cast.gz",
        "claude/b.cast.gz",
    ]);
    assert!(
        delete.stdout.contains("agr sync delete -v"),
        "{}",
        delete.stdout
    );
    assert!(
        !delete.stdout.contains("  claude/a.cast.gz"),
        "{}",
        delete.stdout
    );
    let full = m.run(&[
        "delete",
        "--dry-run",
        "-v",
        "claude/a.cast.gz",
        "claude/b.cast.gz",
    ]);
    assert!(
        full.stdout.contains("  claude/a.cast.gz"),
        "{}",
        full.stdout
    );
    let other = Machine::gist(remote.path());
    let export = m.run(&["key", "export"]).stdout;
    let phrase = export
        .lines()
        .find(|l| l.trim().starts_with("agr1-"))
        .unwrap();
    other.run_stdin(&["key", "import"], &format!("{}\n", phrase.trim()));
    let pull = other.run(&["pull", "--dry-run"]);
    assert!(
        pull.stdout.contains("Would pull 2 session(s)"),
        "{}",
        pull.stdout
    );
    assert!(!pull.stdout.contains("claude/b.cast.gz"), "{}", pull.stdout);
    assert!(other
        .run(&["pull", "-v", "--dry-run"])
        .stdout
        .contains("  claude/b.cast.gz ("));
}

#[test]
fn no_loading_line_and_no_carriage_return_when_stderr_is_not_a_terminal() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "text");
    for args in [
        &["status"][..],
        &["push", "--dry-run"],
        &["push", "--yes"],
        &["pull", "--dry-run"],
        &["delete", "--dry-run", "claude/a.cast.gz"],
    ] {
        let out = m.run(args);
        assert!(
            !out.all().contains("Listing") && !out.all().contains("Scanning"),
            "{args:?} {}",
            out.all()
        );
        assert!(!out.all().contains('\r'), "{args:?} {:?}", out.all());
    }
}

/// Every file below `dir` with its size, sorted.
#[cfg(unix)]
fn tree(dir: &Path) -> Vec<(PathBuf, u64)> {
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

/// With `gh` and `git` off the PATH only the sync commands notice, everything else runs as before.
#[cfg(unix)]
#[test]
fn missing_gh_or_git_breaks_nothing_outside_sync() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "text");
    let bin = TempDir::new().unwrap();
    let plain = |args: &[&str]| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_agr"));
        cmd.args(args)
            .env("NO_COLOR", "1")
            .env("HOME", m.home.path())
            .env("XDG_CONFIG_HOME", m.home.path().join(".config"))
            .env("PATH", bin.path())
            .env_remove("AGR_SYNC_TEST_BACKEND_DIR");
        m.exec(cmd, None)
    };
    for args in [["list"], ["status"]] {
        let out = plain(&args);
        assert_eq!(out.code, 0, "{args:?}: {}", out.all());
    }
    let before = tree(&m.storage);
    let push = gist_cmd(&m, &bin, &["push", "--yes"]);
    assert!(!push.success(), "{}", push.all());
    let first = push.stderr.lines().next().unwrap_or_default();
    assert!(
        first.ends_with(" not found, the gist backend needs gh and git"),
        "{first}"
    );
    assert_eq!(before, tree(&m.storage));
}
