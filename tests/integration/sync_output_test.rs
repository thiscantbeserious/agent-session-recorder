//! Output of every sync command goes through `sync::format`, tested once here: compact summary
//! by default, the full list with `-v`, the loading and live line only on a terminal.

use crate::helpers::pack_engine::{node, opts, Node, CAST};
use agr::sync::format::{
    compact_preview_lines, compact_status_lines, compact_summary_lines, preview_lines,
    status_lines, Live,
};
use agr::sync::testing::MemoryStore;
use agr::sync::{Key, Options, Report};
use std::io::Write;
use std::sync::{Arc, Mutex};

const LOADING: &str = "Listing remote sessions...";

fn thirty() -> Node {
    let n = node();
    for i in 0..30 {
        n.session(
            "claude",
            &format!("s{i}.cast.gz"),
            &CAST.replace("secret", &i.to_string()),
        );
    }
    n
}

fn dry() -> Options {
    Options {
        dry_run: true,
        ..opts()
    }
}

fn listed(lines: &[String]) -> usize {
    lines.iter().filter(|l| l.contains("claude/s")).count()
}

#[test]
fn status_compact_and_full_list_with_v() {
    let (n, store, key) = (thirty(), MemoryStore::new(), Key::generate());
    let report = n.engine(&store, &key, "w1").status(&[]).unwrap();
    let (short, long) = (
        compact_status_lines(&report, "memory"),
        status_lines(&report, "memory"),
    );
    assert!(short.len() < 10, "{short:?}");
    assert_eq!(listed(&long), 30, "{long:?}");
}

#[test]
fn push_and_pull_print_compact_summary_and_full_list_with_v() {
    let (a, store, key) = (thirty(), MemoryStore::new(), Key::generate());
    let plan = a.engine(&store, &key, "w1").push(&dry()).unwrap();
    assert!(compact_preview_lines(&plan, "push").len() < 10);
    assert_eq!(listed(&preview_lines(&plan, "push")), 30);
    let done = a.engine(&store, &key, "w1").push(&opts()).unwrap();
    assert!(compact_summary_lines(&done, "push").len() < 10);
    let b = node();
    let plan = b.engine(&store, &key, "w2").pull(&dry()).unwrap();
    assert!(compact_preview_lines(&plan, "pull").len() < 10);
    assert_eq!(listed(&preview_lines(&plan, "pull")), 30);
}

#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<u8>>>);

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Sink {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

/// Pushes 30 sessions through a `Live` with the given (tty, colour) flags, returns what it wrote.
fn drawn(flags: (bool, bool)) -> (String, Report) {
    let (n, store, key) = (thirty(), MemoryStore::new(), Key::generate());
    let sink = Sink::default();
    let live = Live::with("uploading", "pushed", flags, Box::new(sink.clone()));
    let report = n
        .engine(&store, &key, "w1")
        .push_with(&opts(), 30, &mut live.observer())
        .unwrap();
    live.end();
    (sink.text(), report)
}

#[test]
fn loading_and_live_line_only_on_a_tty() {
    let (tty_out, _) = drawn((true, false));
    assert!(
        tty_out.trim_start_matches('\r').starts_with(LOADING),
        "{tty_out:?}"
    );
    let (plain_out, _) = drawn((false, false));
    assert!(!plain_out.contains(LOADING), "{plain_out:?}");
    assert!(plain_out.is_empty(), "{plain_out:?}");
}

#[test]
fn no_color_gives_no_escape_and_no_token_text() {
    let (plain, _) = drawn((true, false));
    assert!(!plain.contains('\x1b'), "{plain:?}");
    assert!(
        !plain.contains("gho_") && !plain.contains("Token"),
        "{plain:?}"
    );
    let (coloured, _) = drawn((true, true));
    assert!(
        coloured.contains('\x1b'),
        "the colour flag must reach the final lines"
    );
}

#[test]
fn live_line_per_append() {
    let (n, store, key) = (thirty(), MemoryStore::new(), Key::generate());
    let sink = Sink::default();
    let live = Live::with("uploading", "pushed", (true, false), Box::new(sink.clone()));
    n.engine(&store, &key, "w1")
        .push_with(&opts(), 30, &mut live.observer())
        .unwrap();
    let live_lines = sink.text().matches(", uploading 0%").count();
    assert_eq!(live_lines, store.appends(), "{:?}", sink.text());
}

#[test]
fn delete_preview_says_when_a_session_is_a_fork() {
    use crate::helpers::pack_engine::{raw_push, A};
    use agr::sync::format::delete_preview_lines;
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    raw_push(&store, &key, "w2", (A, 1), "one version");
    raw_push(&store, &key, "w3", (A, 1), "another version");
    n.session("claude", "b.cast.gz", CAST);
    raw_push(&store, &key, "w2", ("claude/b.cast.gz", 1), "plain");
    let plan = n.engine(&store, &key, "w1").status(&[]).unwrap();
    let lines = delete_preview_lines(&plan);
    let fork = lines.iter().find(|l| l.contains(A)).unwrap();
    let plain = lines.iter().find(|l| l.contains("b.cast.gz")).unwrap();
    assert!(fork.contains("fork"), "{lines:?}");
    assert!(!plain.contains("fork"), "{lines:?}");
}

#[test]
fn delete_preview_names_the_overwrite_push_that_restores() {
    use agr::sync::format::delete_preview_lines;
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    n.session("claude", "b.cast.gz", CAST);
    crate::helpers::pack_engine::raw_push(&store, &key, "w2", ("claude/b.cast.gz", 1), "plain");
    let plan = n.engine(&store, &key, "w1").status(&[]).unwrap();
    let text = delete_preview_lines(&plan).join("\n");
    assert!(text.contains("push --overwrite"), "{text}");
}

fn pushed_with_fork() -> Report {
    use agr::sync::{Decision, Item, Outcome};
    let item = Item {
        path: "a".into(),
        decision: Decision::Push,
        outcome: Outcome::Done(1),
        size: 0,
        on_remote: true,
        on_local: true,
        local_hash: None,
        remote_hash: None,
        remote_size: 0,
    };
    Report {
        items: vec![item],
        pushed: 1,
        forks: vec!["a".into()],
        ..Report::default()
    }
}

#[test]
fn summaries_show_forks_a_push_left_and_the_overwrite_hint() {
    let r = pushed_with_fork();
    for out in [
        agr::sync::format::summary_lines(&r).join("\n"),
        compact_summary_lines(&r, "push").join("\n"),
    ] {
        assert!(out.contains("Fork after push: a"), "{out}");
        assert!(out.contains("agr sync push --overwrite"), "{out}");
    }
}

fn delete_item(path: &str, decision: agr::sync::Decision, on_local: bool) -> agr::sync::Item {
    agr::sync::Item {
        path: path.into(),
        decision,
        outcome: agr::sync::Outcome::Planned,
        size: 1,
        on_remote: true,
        on_local,
        local_hash: None,
        remote_hash: None,
        remote_size: 1,
    }
}

fn report_of(items: Vec<agr::sync::Item>) -> Report {
    Report {
        items,
        ..Report::default()
    }
}

#[test]
fn delete_preview_of_remote_only_sessions_says_they_cannot_be_restored() {
    use agr::sync::format::{compact_delete_preview_lines, delete_preview_lines};
    use agr::sync::Decision;
    let r = report_of(vec![delete_item("a/r.cast.gz", Decision::Pull, false)]);
    for lines in [delete_preview_lines(&r), compact_delete_preview_lines(&r)] {
        let text = lines.join("\n");
        assert!(
            text.contains("cannot be restored by `agr sync push`"),
            "{text}"
        );
        assert!(!text.contains("uploads the session again"), "{text}");
    }
}

#[test]
fn delete_preview_with_a_local_copy_keeps_the_upload_line() {
    use agr::sync::format::{compact_delete_preview_lines, delete_preview_lines};
    use agr::sync::Decision;
    let r = report_of(vec![
        delete_item("a/r.cast.gz", Decision::Pull, false),
        delete_item("a/l.cast.gz", Decision::Push, true),
    ]);
    for lines in [delete_preview_lines(&r), compact_delete_preview_lines(&r)] {
        let text = lines.join("\n");
        assert!(text.contains("uploads the session again"), "{text}");
    }
}

#[test]
fn compact_delete_preview_keeps_the_fork_count_and_the_hint() {
    use agr::sync::format::compact_delete_preview_lines;
    use agr::sync::Decision;
    let r = report_of(vec![
        delete_item("a/f.cast.gz", Decision::Fork, true),
        delete_item("a/p.cast.gz", Decision::Push, true),
    ]);
    let text = compact_delete_preview_lines(&r).join("\n");
    assert!(
        text.contains("1 fork(s), every version is deleted, see -v"),
        "{text}"
    );
    assert!(text.contains("Would delete 2 sessions"), "{text}");
}

/// 12 sessions of about 15 KiB packed, a 64 KiB target gives several pack appends.
fn heavy(n: &Node) {
    let mut x: u64 = 7;
    for i in 0..12 {
        let noise: String = (0..30_000)
            .map(|_| {
                x = x
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                char::from(b'a' + (x >> 59) as u8)
            })
            .collect();
        n.session(
            "claude",
            &format!("h{i}.cast.gz"),
            &CAST.replace("secret", &noise),
        );
    }
}

/// The live heads drawn by a push of `heavy` sessions, with `plain` skipped plain casts around.
fn pack_heads(plain: usize) -> Vec<String> {
    use agr::sync::Engine;
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    heavy(&n);
    for i in 0..plain {
        let dir = n.dir().join("claude");
        std::fs::write(dir.join(format!("p{i}.cast")), CAST).unwrap();
    }
    let sink = Sink::default();
    let live = Live::with("uploading", "pushed", (true, false), Box::new(sink.clone()));
    let engine = Engine::new(&store, &key, &n.storage, "w1", 64 * 1024);
    engine
        .push_with(&opts(), 12 + plain, &mut live.observer())
        .unwrap();
    let text = sink.text();
    text.split('\r')
        .filter(|s| s.contains(", uploading 0%"))
        .map(|s| s.trim().to_string())
        .collect()
}

#[test]
fn live_head_counts_pack_appends_not_rows() {
    let heads = pack_heads(0);
    assert!(heads.len() >= 3, "{heads:?}");
    for (i, h) in heads.iter().enumerate() {
        assert!(h.starts_with(&format!("[pack {}] ", i + 1)), "{heads:?}");
        assert!(h.contains(" session"), "{heads:?}");
    }
    assert_eq!(sessions_of(&heads), 12, "{heads:?}");
}

#[test]
fn skipped_rows_do_not_count_toward_the_pack_progress() {
    let heads = pack_heads(5);
    assert_eq!(heads.len(), pack_heads(0).len(), "{heads:?}");
    assert_eq!(sessions_of(&heads), 12, "{heads:?}");
}

/// Sessions the heads announce together, a head without a count is one session.
fn sessions_of(heads: &[String]) -> usize {
    let count = |h: &String| {
        let word = h.split(' ').nth(2).unwrap_or("");
        word.parse::<usize>().unwrap_or(1)
    };
    heads.iter().map(count).sum()
}
