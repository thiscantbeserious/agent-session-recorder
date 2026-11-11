//! `agr sync delete` against the fake backend.

use crate::helpers::cli_sync::*;
use std::fs;

#[test]
fn delete_removes_the_remote_and_keeps_the_local_file() {
    let remote = remote();
    let m = Machine::gist(remote.path());
    let local = m.session("claude", "a.cast.gz", "one");
    m.run(&["push", "--yes"]);
    let before = fingerprint(remote.path());
    let dry = m.run(&["delete", "--dry-run", "claude/a.cast.gz"]);
    assert_eq!(dry.code, 0, "{}", dry.all());
    assert_eq!(remote_files(remote.path(), "manifest").len(), 1);
    assert_eq!(fingerprint(remote.path()), before, "dry run, no append");
    let no_yes = m.run(&["delete", "claude/a.cast.gz"]);
    assert!(
        no_yes.stdout.contains("No changes made."),
        "{}",
        no_yes.stdout
    );
    assert_eq!(remote_files(remote.path(), "manifest").len(), 1);
    assert_eq!(fingerprint(remote.path()), before, "no yes, no append");
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
fn delete_then_push_on_a_fresh_machine_keeps_one_writer_and_one_open_pack() {
    let remote = remote();
    let a = Machine::gist(remote.path());
    a.session("claude", "a.cast.gz", "one");
    assert_eq!(a.run(&["push", "--yes"]).code, 0);
    let b = Machine::gist(remote.path());
    assert_eq!(b.run(&["key", "import", &phrase_of(&a)]).code, 0);
    let del = b.run(&["delete", "--yes", "claude/a.cast.gz"]);
    assert_eq!(del.code, 0, "{}", del.all());
    let writer = writer_of(&b);
    assert!(
        writer.is_some(),
        "delete stored no writer id: {}",
        b.state()
    );
    b.session("claude", "b.cast.gz", "two");
    assert_eq!(b.run(&["push", "--yes"]).code, 0);
    assert_eq!(writer_of(&b), writer);
    let packs = fs::read_dir(remote.path()).unwrap().count();
    assert_eq!(packs, 2, "delete and push of one machine share a pack");
}

#[test]
fn delete_dry_run_and_declined_prompt_store_no_writer_id() {
    let remote = remote();
    let a = Machine::gist(remote.path());
    a.session("claude", "a.cast.gz", "one");
    assert_eq!(a.run(&["push", "--yes"]).code, 0);
    let b = Machine::gist(remote.path());
    assert_eq!(b.run(&["key", "import", &phrase_of(&a)]).code, 0);
    let before = fingerprint(remote.path());
    assert_eq!(b.run(&["delete", "--dry-run", "claude/a.cast.gz"]).code, 0);
    assert_eq!(b.run(&["delete", "claude/a.cast.gz"]).code, 0);
    assert_eq!(writer_of(&b), None, "{}", b.state());
    assert_eq!(fingerprint(remote.path()), before, "no append");
}

mod patterns {
    use super::*;

    #[test]
    fn preview_is_collected_before_any_prompt() {
        let remote = remote();
        let (_a, b) = pushed_pair(remote.path(), 2);
        let before = fingerprint(remote.path());
        let out = b.run(&["delete", "claude/s*"]);
        let (count, ask) = (out.stdout.find("2 sessions"), out.stdout.find("--yes"));
        assert!(
            count.unwrap() < ask.unwrap(),
            "preview first: {}",
            out.stdout
        );
        assert!(!out.stdout.contains("zz-local"), "{}", out.stdout);
        let verbose = b.run(&["delete", "-v", "--dry-run", "claude/s*"]);
        for name in ["claude/s00.cast.gz", "claude/s01.cast.gz"] {
            assert!(verbose.stdout.contains(name), "{}", verbose.stdout);
        }
        assert_eq!(fingerprint(remote.path()), before, "no append");
        assert_eq!(writer_of(&b), None, "{}", b.state());
    }
}

mod prompts {
    use super::*;

    #[test]
    fn over_ten_without_tty_needs_force() {
        let remote = remote();
        let (_a, b) = pushed_pair(remote.path(), 11);
        let before = fingerprint(remote.path());
        let out = b.run(&["delete", "--yes", "claude/s*"]);
        assert!(!out.success(), "{}", out.all());
        assert!(out.stderr.contains("--force"), "{}", out.stderr);
        assert_eq!(fingerprint(remote.path()), before, "no append");
        assert_eq!(writer_of(&b), None, "{}", b.state());
        let one = b.run(&["delete", "claude/s00*"]);
        assert!(one.stdout.contains("--yes"), "{}", one.stdout);
        assert_eq!(fingerprint(remote.path()), before, "one match needs --yes");
        let forced = b.run(&["delete", "--yes", "--force", "claude/s*"]);
        assert!(forced.stdout.contains("Deleted: 11"), "{}", forced.all());
    }

    #[test]
    fn no_remote_match_says_so_and_exits_non_zero() {
        let remote = remote();
        let (_a, b) = pushed_pair(remote.path(), 2);
        let before = fingerprint(remote.path());
        let out = b.run(&["delete", "--yes", "x*", "claude/zz-local.cast.gz"]);
        assert!(!out.success(), "{}", out.all());
        assert!(
            out.stderr.contains("no remote session matches"),
            "{}",
            out.stderr
        );
        assert_eq!(fingerprint(remote.path()), before, "no append");
        assert!(b.storage.join("claude/zz-local.cast.gz").exists());
        assert_eq!(writer_of(&b), None, "{}", b.state());
    }

    #[test]
    fn deleting_the_same_pattern_twice_fails_the_second_time_without_an_append() {
        let remote = remote();
        let (_a, b) = pushed_pair(remote.path(), 2);
        let first = b.run(&["delete", "--yes", "claude/s*"]);
        assert_eq!(first.code, 0, "{}", first.all());
        let before = fingerprint(remote.path());
        let again = b.run(&["delete", "--yes", "claude/s*"]);
        assert!(!again.success(), "{}", again.all());
        assert!(
            again.stderr.contains("no remote session matches"),
            "{}",
            again.stderr
        );
        assert_eq!(fingerprint(remote.path()), before, "no append");
    }

    #[test]
    fn pattern_delete_is_one_tombstone_append() {
        let remote = remote();
        let (a, b) = pushed_pair(remote.path(), 11);
        let before = fingerprint(remote.path());
        let out = b.run(&["delete", "--yes", "--force", "claude/s*"]);
        assert_eq!(out.code, 0, "{}", out.all());
        let after = fingerprint(remote.path());
        let moved = after.iter().filter(|f| !before.contains(f)).count();
        assert_eq!(moved, 1, "one append rewrites only the manifest");
        let status = a.run(&["status", "-v"]);
        let dead = status
            .stdout
            .lines()
            .filter(|l| l.contains("claude/s"))
            .count();
        assert_eq!(dead, 11, "{}", status.stdout);
        assert!(a.storage.join("claude/s00.cast.gz").exists());
        assert!(b.storage.join("claude/zz-local.cast.gz").exists());
    }
}

mod env_var {
    use super::*;

    #[test]
    fn assume_tty_env_does_not_turn_a_piped_run_interactive() {
        let remote = remote();
        let (_a, b) = pushed_pair(remote.path(), 11);
        let before = fingerprint(remote.path());
        let mut cmd = b.base(&["delete", "claude/s*"]);
        cmd.env("AGR_ASSUME_TTY", "1");
        let out = b.exec(cmd, Some("y\n11\n"));
        assert!(!out.success(), "{}", out.all());
        assert_eq!(fingerprint(remote.path()), before, "no append");
    }
}

mod migrate {
    use crate::helpers::cli_sync::*;

    #[test]
    fn without_yes_and_without_a_terminal_nothing_is_written() {
        let remote = remote();
        let m = Machine::gist(remote.path());
        m.session("claude", "a.cast.gz", "one");
        m.run(&["push", "--yes"]);
        let before = fingerprint(remote.path());
        let out = m.run(&["migrate"]);
        assert!(out.success(), "{}", out.all());
        assert!(out.stdout.contains("No changes made."), "{}", out.stdout);
        assert!(out.stdout.contains("0 sessions to move"), "{}", out.stdout);
        assert_eq!(
            fingerprint(remote.path()),
            before,
            "zero appends, zero deletes"
        );
    }

    #[test]
    fn yes_on_a_pack_remote_without_old_gists_changes_nothing() {
        let remote = remote();
        let m = Machine::gist(remote.path());
        m.session("claude", "a.cast.gz", "one");
        m.run(&["push", "--yes"]);
        let before = fingerprint(remote.path());
        let out = m.run(&["migrate", "--yes"]);
        assert!(out.success(), "{}", out.all());
        assert_eq!(fingerprint(remote.path()), before);
    }

    #[test]
    fn a_declined_run_writes_nothing_and_keeps_the_v1_index() {
        let remote = remote();
        let m = Machine::gist(remote.path());
        m.session("claude", "a.cast.gz", "one");
        m.run(&["push", "--yes"]);
        let index = m.storage.join(".agr-sync.json");
        let v1 = r#"{"version":1,"entries":{"claude/a.cast.gz":{"sha256":"h","remote_id":"gid","synced_at":"t","gen":3,"token":"tok","pending":false,"size":7,"mtime_ns":9}}}"#;
        std::fs::write(&index, v1).unwrap();
        let before = fingerprint(remote.path());
        // ponytail: a piped stdin is no terminal, the "n" is never read, the typed no is in sync_confirm_test
        let out = m.run_stdin(&["migrate"], "n\n");
        assert!(out.success(), "{}", out.all());
        assert!(out.stdout.contains("No changes made."), "{}", out.stdout);
        assert_eq!(
            fingerprint(remote.path()),
            before,
            "zero appends, zero deletes"
        );
        assert_eq!(std::fs::read_to_string(&index).unwrap(), v1);
    }

    #[test]
    fn yes_converts_only_the_index_of_the_configured_storage_dir() {
        let remote = remote();
        let m = Machine::gist(remote.path());
        m.session("claude", "a.cast.gz", "one");
        m.run(&["push", "--yes"]);
        let index = m.storage.join(".agr-sync.json");
        let v1 = r#"{"version":1,"entries":{"claude/a.cast.gz":{"sha256":"h","remote_id":"gid","synced_at":"t","gen":3,"token":"tok","pending":false,"size":7,"mtime_ns":9}}}"#;
        std::fs::write(&index, v1).unwrap();
        let out = m.run(&["migrate", "--yes"]);
        assert!(out.success(), "{}", out.all());
        let text = std::fs::read_to_string(&index).unwrap();
        assert!(text.contains(r#""version":2"#), "{text}");
        assert!(!m.remote.join(".agr-sync.json").exists());
    }

    #[test]
    fn a_corrupt_v1_index_aborts_the_command() {
        let remote = remote();
        let m = Machine::gist(remote.path());
        m.session("claude", "a.cast.gz", "one");
        m.run(&["push", "--yes"]);
        let index = m.storage.join(".agr-sync.json");
        std::fs::write(&index, "{not json").unwrap();
        let out = m.run(&["migrate", "--yes"]);
        assert!(!out.success(), "{}", out.all());
        assert!(out.stderr.contains("is not valid"), "{}", out.stderr);
        assert_eq!(std::fs::read_to_string(&index).unwrap(), "{not json");
    }

    #[test]
    fn a_failed_run_leaves_no_scratch_dir() {
        let remote = remote();
        let m = Machine::gist(remote.path());
        m.session("claude", "a.cast.gz", "one");
        m.run(&["push", "--yes"]);
        std::fs::write(m.storage.join(".agr-sync.json"), "{not json").unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let mut cmd = m.base(&["migrate", "--yes"]);
        cmd.env("TMPDIR", tmp.path()).env("TEMP", tmp.path());
        let out = m.exec(cmd, None);
        assert!(!out.success(), "{}", out.all());
        let left: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().flatten().collect();
        assert!(left.is_empty(), "scratch left behind: {left:?}");
    }
}
