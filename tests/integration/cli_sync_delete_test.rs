//! `agr sync delete` against the fake backend.

use crate::helpers::cli_sync::*;
use std::fs;

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
    assert_eq!(b.run(&["delete", "--dry-run", "claude/a.cast.gz"]).code, 0);
    assert_eq!(b.run(&["delete", "claude/a.cast.gz"]).code, 0);
    assert_eq!(writer_of(&b), None, "{}", b.state());
}

mod patterns {
    use super::*;

    #[test]
    fn preview_is_collected_before_any_prompt() {
        let remote = remote();
        let (_a, b) = pushed_pair(remote.path(), 2);
        let before = fingerprint(remote.path());
        let out = b.run_tty(&["delete", "claude/s*"], "n\n");
        let (count, ask) = (out.stdout.find("2 sessions"), out.stdout.find("Delete"));
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
    fn one_prompt_up_to_ten_matches() {
        let remote = remote();
        let (_a, b) = pushed_pair(remote.path(), 10);
        let before = fingerprint(remote.path());
        let out = b.run_tty(&["delete", "claude/s*"], "n\n");
        assert!(!out.stdout.contains("Type"), "{}", out.stdout);
        assert!(out.stdout.contains("No changes made."), "{}", out.stdout);
        assert_eq!(fingerprint(remote.path()), before, "no append");
        assert_eq!(writer_of(&b), None, "{}", b.state());
    }

    #[test]
    fn over_ten_matches_asks_to_type_the_count() {
        let remote = remote();
        let (_a, b) = pushed_pair(remote.path(), 11);
        let before = fingerprint(remote.path());
        let out = b.run_tty(&["delete", "claude/s*"], "y\n7\n");
        assert!(out.stdout.contains("Type 11 to confirm"), "{}", out.stdout);
        assert!(out.stdout.contains("No changes made."), "{}", out.stdout);
        assert_eq!(fingerprint(remote.path()), before, "wrong count, no append");
        assert_eq!(writer_of(&b), None, "{}", b.state());
        let right = b.run_tty(&["delete", "claude/s*"], "y\n11\n");
        assert!(right.stdout.contains("Deleted: 11"), "{}", right.all());
    }

    #[test]
    fn yes_skips_only_the_first_prompt() {
        let remote = remote();
        let (_a, b) = pushed_pair(remote.path(), 11);
        let before = fingerprint(remote.path());
        let out = b.run_tty(&["delete", "--yes", "claude/s*"], "3\n");
        assert!(out.stdout.contains("Type 11 to confirm"), "{}", out.stdout);
        assert_eq!(fingerprint(remote.path()), before, "wrong count, no append");
        assert_eq!(writer_of(&b), None, "{}", b.state());
    }

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
