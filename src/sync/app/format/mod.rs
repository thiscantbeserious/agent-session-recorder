//! Plain text lines for the sync commands, colour is added by the caller via theme helpers.

use humansize::{format_size, BINARY};

use crate::sync::engine::plan::{Decision, SkipReason};
use crate::sync::engine::run::{Item, Outcome, Report};
use crate::sync::engine::scan::Duplicate;

mod live;
mod packs;
mod status;
mod summary;

pub use live::{live_head, Live, LiveLine, LoadLine};
pub use status::{
    compact_delete_preview_lines, compact_preview_lines, compact_status_lines,
    compact_summary_lines,
};
pub use summary::{
    delete_preview_lines, preview_lines, privacy_lines, problem_lines, progress_line,
    rate_limit_lines, summary_lines,
};

const ORDER: [&str; 10] = [
    "in sync",
    "local ahead (push)",
    "local only (push)",
    "remote ahead (pull)",
    "remote only (pull)",
    "conflict",
    "duplicate",
    "fork",
    "deleted on the remote",
    "skipped",
];

/// Status label of one session, from the decision and which sides have it.
pub fn label(item: &Item) -> &'static str {
    match item.decision {
        Decision::InSync { .. } => ORDER[0],
        Decision::Push if item.on_remote => ORDER[1],
        Decision::Push => ORDER[2],
        Decision::Pull if item.on_local => ORDER[3],
        Decision::Pull => ORDER[4],
        Decision::Conflict => ORDER[5],
        Decision::Duplicate => ORDER[6],
        Decision::Fork => ORDER[7],
        Decision::Skip(SkipReason::DeletedOnRemote) => ORDER[8],
        Decision::OtherKey | Decision::UnknownVersion | Decision::Skip(_) => ORDER[9],
    }
}

/// Backend line, one line per session and totals per state.
pub fn status_lines(report: &Report, backend: &str) -> Vec<String> {
    let mut lines = vec![backend_line(report, backend)];
    lines.extend(queued_line(report));
    if report.items.is_empty() {
        lines.push("No sessions.".into());
        return lines;
    }
    lines.extend(report.items.iter().flat_map(status_block));
    lines.push(totals(report));
    lines.extend(packs::prune_hint(report));
    lines
}

/// `N queued for auto push`, only when the queue is not empty.
pub(super) fn queued_line(report: &Report) -> Option<String> {
    (report.queued > 0).then(|| format!("{} queued for auto push", report.queued))
}

pub(super) fn backend_line(report: &Report, backend: &str) -> String {
    let remote = report.items.iter().filter(|i| i.on_remote).count();
    format!("Backend: {backend}, {remote} remote session(s)")
}

pub(super) fn status_block(item: &Item) -> Vec<String> {
    let mut lines = vec![status_line(item)];
    if let (true, Some(id)) = (item.pending, &item.remote_id) {
        lines.push(format!("    pending upload, the next push resumes {id}"));
    }
    lines.extend(item.duplicates.iter().map(duplicate_line));
    lines
}

fn duplicate_line(dup: &Duplicate) -> String {
    let same = if dup.same_hash { "same" } else { "different" };
    format!(
        "    also on the remote: {id} ({same} content), delete with `gh gist delete {id}`",
        id = dup.id
    )
}

fn status_line(item: &Item) -> String {
    let mut line = format!("  {} {} ({})", item.path, label(item), size(item.size));
    if let Outcome::Skipped(why) | Outcome::Failed(why) = &item.outcome {
        line.push_str(&format!(": {why}"));
    }
    line
}

fn totals(report: &Report) -> String {
    let parts: Vec<String> = ORDER
        .iter()
        .filter_map(|name| {
            let n = report.items.iter().filter(|i| label(i) == *name).count();
            (n > 0).then(|| format!("{n} {name}"))
        })
        .collect();
    format!("Totals: {}", parts.join(", "))
}

fn short(hash: Option<&str>) -> String {
    hash.unwrap_or("-").chars().take(12).collect()
}

fn size(bytes: u64) -> String {
    format_size(bytes, BINARY)
}

#[cfg(test)]
mod tests {
    use super::{label, progress_line, status_lines, Decision, Item, Outcome, Report};
    use crate::sync::engine::plan::SkipReason;
    use crate::sync::engine::scan::Duplicate;

    pub(super) fn item(path: &str, decision: Decision, outcome: Outcome) -> Item {
        Item {
            path: path.into(),
            decision,
            outcome,
            size: 2048,
            on_remote: true,
            on_local: true,
            local_hash: Some("a".repeat(64)),
            remote_hash: Some("b".repeat(64)),
            remote_size: 4096,
            remote_id: None,
            pending: false,
            duplicates: Vec::new(),
        }
    }

    #[test]
    fn duplicates_are_listed_with_ids_and_the_delete_command() {
        let mut dup = item("a/d.cast.gz", Decision::Duplicate, Outcome::Nothing);
        dup.remote_id = Some("keep1".into());
        dup.duplicates = vec![
            Duplicate {
                id: "x2".into(),
                same_hash: true,
            },
            Duplicate {
                id: "y3".into(),
                same_hash: false,
            },
        ];
        let lines = status_lines(&report(vec![dup]), "gist");
        assert_eq!(
            lines[1..4],
            [
                "  a/d.cast.gz duplicate (2 KiB)",
                "    also on the remote: x2 (same content), delete with `gh gist delete x2`",
                "    also on the remote: y3 (different content), delete with `gh gist delete y3`",
            ]
        );
    }

    #[test]
    fn pending_entry_names_the_gist_the_next_push_resumes() {
        let mut p = item("a/p.cast.gz", Decision::Push, Outcome::Planned);
        p.remote_id = Some("gist-7".into());
        p.pending = true;
        let lines = status_lines(&report(vec![p]), "gist");
        assert_eq!(lines[2], "    pending upload, the next push resumes gist-7");
    }

    pub(super) fn sync() -> Decision {
        Decision::InSync {
            refresh_index: false,
        }
    }

    pub(super) fn report(items: Vec<Item>) -> Report {
        Report {
            items,
            ..Report::default()
        }
    }

    #[test]
    fn labels_cover_every_state() {
        let mut local_only = item("a/1", Decision::Push, Outcome::Planned);
        local_only.on_remote = false;
        let mut remote_only = item("a/2", Decision::Pull, Outcome::Planned);
        remote_only.on_local = false;
        let cases = [
            (item("a/3", sync(), Outcome::Nothing), "in sync"),
            (
                item("a/4", Decision::Push, Outcome::Planned),
                "local ahead (push)",
            ),
            (local_only, "local only (push)"),
            (
                item("a/5", Decision::Pull, Outcome::Planned),
                "remote ahead (pull)",
            ),
            (remote_only, "remote only (pull)"),
            (
                item("a/6", Decision::Conflict, Outcome::Nothing),
                "conflict",
            ),
            (
                item("a/7", Decision::Skip(SkipReason::Plain), Outcome::Nothing),
                "skipped",
            ),
            (item("a/8", Decision::OtherKey, Outcome::Nothing), "skipped"),
        ];
        for (item, want) in cases {
            assert_eq!(label(&item), want, "{}", item.path);
        }
    }

    #[test]
    fn status_shows_backend_sessions_and_totals() {
        let r = report(vec![
            item("claude/a.cast.gz", sync(), Outcome::Nothing),
            item("claude/b.cast.gz", Decision::Push, Outcome::Planned),
            item(
                "claude/c.cast.gz",
                Decision::Skip(SkipReason::Plain),
                Outcome::Skipped("plain .cast, run `agr compress` first".into()),
            ),
        ]);
        let text = status_lines(&r, "gist").join("\n");
        assert!(
            text.contains("Backend: gist, 3 remote session(s)"),
            "{text}"
        );
        assert!(text.contains("in sync"), "{text}");
        assert!(text.contains("claude/b.cast.gz"), "{text}");
        assert!(text.contains("local ahead (push)"), "{text}");
        assert!(text.contains("2 KiB"), "{text}");
        assert!(
            text.contains("plain .cast, run `agr compress` first"),
            "{text}"
        );
        assert!(
            text.contains("Totals: 1 in sync, 1 local ahead (push), 1 skipped"),
            "{text}"
        );
    }

    #[test]
    fn status_of_nothing_says_so() {
        let text = status_lines(&Report::default(), "gist").join("\n");
        assert!(
            text.contains("Backend: gist, 0 remote session(s)"),
            "{text}"
        );
        assert!(text.contains("No sessions"), "{text}");
    }

    #[test]
    fn progress_line_covers_done_skipped_and_failed() {
        let p = "claude/x.cast.gz";
        let done = item(p, Decision::Push, Outcome::Done(12 * 1024 * 1024));
        let skip = item(
            p,
            Decision::Conflict,
            Outcome::Skipped("conflict, use --overwrite".into()),
        );
        let fail = item(p, Decision::Push, Outcome::Failed("upload broke".into()));
        let gone = item(p, Decision::Push, Outcome::Done(0));
        let got = [
            progress_line("pushed", &done, 3, 40),
            progress_line("pulled", &done, 1, 2),
            progress_line("deleted", &gone, 2, 2),
            progress_line("pushed", &skip, 4, 40),
            progress_line("pushed", &fail, 5, 40),
        ];
        assert_eq!(
            got,
            [
                "[3/40] pushed claude/x.cast.gz (12 MiB)",
                "[1/2] pulled claude/x.cast.gz (12 MiB)",
                "[2/2] deleted claude/x.cast.gz",
                "[4/40] skipped claude/x.cast.gz: conflict, use --overwrite",
                "[5/40] failed claude/x.cast.gz: upload broke",
            ]
        );
    }
}
