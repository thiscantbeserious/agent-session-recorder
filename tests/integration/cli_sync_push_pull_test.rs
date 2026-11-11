//! `agr sync push` and `pull` against the fake backend.

use crate::helpers::cli_sync::*;
use std::fs;
use std::process::Command;

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

#[test]
fn push_dry_run_and_declined_prompt_store_no_writer_id() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    m.session("claude", "a.cast.gz", "text");
    assert_eq!(m.run(&["push", "--dry-run"]).code, 0);
    assert_eq!(writer_of(&m), None, "{}", m.state());
    assert_eq!(m.run(&["push"]).code, 0);
    assert_eq!(writer_of(&m), None, "{}", m.state());
    assert_eq!(m.run(&["push", "--yes"]).code, 0);
    assert!(writer_of(&m).is_some(), "{}", m.state());
}

/// An empty or garbage lock file counts as held, so the Busy text names the file to remove.
#[test]
fn busy_text_names_the_lock_file_for_an_empty_or_garbage_lock() {
    for content in ["", "garbage"] {
        let remote = remote();
        let m = Machine::gist(remote.path());
        m.session("claude", "a.cast.gz", "one");
        let lock = m.storage.join(".agr-sync.lock");
        std::fs::write(&lock, content).unwrap();
        for args in [&["push", "--yes"][..], &["migrate", "--yes"]] {
            let out = m.run(args);
            assert!(!out.success(), "{}", out.all());
            let all = out.all();
            assert!(all.contains("Another sync is running"), "{all}");
            assert!(all.contains(&lock.display().to_string()), "{args:?}: {all}");
        }
    }
}
