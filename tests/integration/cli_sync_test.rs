//! `agr sync ...` setup, missing tools, key and token handling against the fake backend.

use crate::helpers::cli_sync::*;
use std::fs;
#[cfg(unix)]
use std::process::Command;
use tempfile::TempDir;

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
fn test_backend_dir_is_announced_on_stderr() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    let out = m.run(&["status"]);
    let line = format!("Using test sync backend at {}", remote.path().display());
    assert!(out.stderr.contains(&line), "{}", out.stderr);
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

/// Only `git` off the PATH (gh is there), push names git and changes nothing, list and status run.
#[cfg(unix)]
#[test]
fn missing_git_alone_fails_push_with_the_git_text_and_breaks_nothing_else() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "text");
    let bin = fake_bin(&[("gh", "if [ \"$1\" = api ]; then echo '[]'; fi\nexit 0")]);
    for args in [["list"], ["status"]] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_agr"));
        cmd.args(args)
            .env("NO_COLOR", "1")
            .env("HOME", m.home.path())
            .env("XDG_CONFIG_HOME", m.home.path().join(".config"))
            .env("PATH", bin.path())
            .env_remove("AGR_SYNC_TEST_BACKEND_DIR");
        let out = m.exec(cmd, None);
        assert_eq!(out.code, 0, "{args:?}: {}", out.all());
    }
    let before = tree(&m.storage);
    let push = gist_cmd(&m, &bin, &["push", "--yes"]);
    assert_ne!(push.code, 0, "{}", push.all());
    let all = push.all();
    assert!(
        all.contains("git not found, the gist backend needs gh and git"),
        "{all}"
    );
    assert!(all.contains("Install git and gh"), "{all}");
    assert_eq!(before, tree(&m.storage));
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
fn prune_while_locked_is_busy() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "one");
    assert_eq!(m.run(&["push", "--yes"]).code, 0);
    let lock = format!(r#"{{"pid":{},"started":"x"}}"#, std::process::id());
    fs::write(m.storage.join(".agr-sync.lock"), lock).unwrap();
    let out = m.run(&["prune", "--dry-run"]);
    assert!(!out.success(), "{}", out.all());
    assert!(out.all().contains("busy"), "{}", out.all());
}
