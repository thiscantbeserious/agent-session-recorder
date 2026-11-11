//! Lines for previews, notices, problems and run summaries.

use super::{label, short, size};
use crate::sync::engine::plan::Decision;
use crate::sync::engine::run::{Item, Outcome, Report};

/// What a push or pull would do (`verb` is `push` or `pull`), with sizes.
pub fn preview_lines(report: &Report, verb: &str) -> Vec<String> {
    let Some(head) = planned_head(report, verb) else {
        return vec![format!("Nothing to {verb}.")];
    };
    let rows = planned(report).map(|i| format!("  {} ({})", i.path, size(planned_bytes(i, verb))));
    std::iter::once(head).chain(rows).collect()
}

fn planned(report: &Report) -> impl Iterator<Item = &Item> {
    report
        .items
        .iter()
        .filter(|i| i.outcome == Outcome::Planned)
}

fn planned_bytes(item: &Item, verb: &str) -> u64 {
    if verb == "pull" {
        item.remote_size
    } else {
        item.size
    }
}

/// `Would push 721 session(s), 440.77 MiB`, `None` when nothing is planned.
pub(super) fn planned_head(report: &Report, verb: &str) -> Option<String> {
    let count = planned(report).count();
    let total: u64 = planned(report).map(|i| planned_bytes(i, verb)).sum();
    (count > 0).then(|| format!("Would {verb} {count} session(s), {}", size(total)))
}

/// First push notice, privacy and key backup.
pub fn privacy_lines() -> Vec<String> {
    [
        "Syncing uploads your sessions to secret GitHub gists.",
        "A secret gist is not listed, but anyone with the link can read it. Sessions can contain secrets typed in the terminal.",
        "Every session is encrypted on this machine first, gists only hold ciphertext and an opaque token.",
        "Back up your key with agr sync key export. Losing every copy of the key leaves synced sessions unrecoverable.",
    ]
    .map(String::from)
    .to_vec()
}

/// Skipped, conflicts with both hashes and sizes, failures and the index warning.
pub fn problem_lines(report: &Report) -> Vec<String> {
    let mut lines = detail("Skipped", report.skipped, report, skipped_line);
    lines.extend(detail("Conflicts", report.conflicts, report, conflict_line));
    lines.extend(detail("Failed", report.failed, report, failed_line));
    lines.extend(report.warning.iter().map(|w| format!("Warning: {w}")));
    lines
}

fn sessions(n: usize) -> String {
    format!("{n} session{}", if n == 1 { "" } else { "s" })
}

/// Remote copies `agr sync delete` would remove.
pub fn delete_preview_lines(report: &Report) -> Vec<String> {
    let remote: Vec<&Item> = report.items.iter().filter(|i| i.on_remote).collect();
    if remote.is_empty() {
        return vec!["Nothing to delete.".into()];
    }
    let mut lines = vec![format!(
        "Would delete {} from the remote",
        sessions(remote.len())
    )];
    let line = |i: &&Item| match i.decision {
        Decision::Fork => format!("  {} (fork, every version is deleted)", i.path),
        _ => format!("  {}", i.path),
    };
    lines.extend(remote.iter().map(line));
    lines.push(restore_line(&remote));
    lines
}

/// Only a session with a local copy can be uploaded again.
fn restore_line(remote: &[&Item]) -> String {
    if remote.iter().any(|i| i.on_local) {
        "A later `agr sync push --overwrite` uploads the session again.".into()
    } else {
        "Remote-only sessions cannot be restored by `agr sync push`.".into()
    }
}

/// One line per acted-on session, `[i/n] pushed path (size)`, `done` is the past tense verb.
pub fn progress_line(done: &str, item: &Item, i: usize, n: usize) -> String {
    let head = format!("[{i}/{n}]");
    match &item.outcome {
        Outcome::Done(0) => format!("{head} {done} {}", item.path),
        Outcome::Done(bytes) => format!("{head} {done} {} ({})", item.path, size(*bytes)),
        Outcome::Skipped(why) => format!("{head} skipped {}: {why}", item.path),
        Outcome::Failed(why) => format!("{head} failed {}: {why}", item.path),
        _ => format!("{head} {} {}", item.path, label(item)),
    }
}

/// Two lines when GitHub stopped the run, `left` sessions were not done.
pub fn rate_limit_lines(left: usize) -> [String; 2] {
    [
        "GitHub rate limit reached, sync stopped".into(),
        format!("{left} left, please run again later"),
    ]
}

/// Result of a real run, counts, skipped, conflicts with both hashes and sizes, failures.
pub fn summary_lines(report: &Report) -> Vec<String> {
    let mut lines = count_lines(report);
    lines.extend(
        problem_lines(report)
            .into_iter()
            .filter(|l| !l.starts_with("Warning: ")),
    );
    if lines.is_empty() {
        lines.push("Nothing to do.".into());
    }
    lines.extend(fork_lines(report));
    lines.extend(report.warning.iter().map(|w| format!("Warning: {w}")));
    lines
}

/// Forks this run's push left, not already shown as a fork item, with the way out.
pub(super) fn fork_lines(report: &Report) -> Vec<String> {
    let shown = |p: &&String| {
        let fork = |i: &Item| i.path == **p && i.decision == Decision::Fork;
        !report.items.iter().any(fork)
    };
    let mut lines: Vec<String> = report
        .forks
        .iter()
        .filter(shown)
        .map(|p| format!("Fork after push: {p}"))
        .collect();
    if !lines.is_empty() {
        lines.push("run `agr sync push --overwrite` to resolve".into());
    }
    lines
}

pub(super) fn count_lines(report: &Report) -> Vec<String> {
    let counts = [
        ("Pushed", report.pushed),
        ("Pulled", report.pulled),
        ("Deleted", report.deleted),
    ];
    let mut lines: Vec<String> = counts
        .iter()
        .filter(|(_, n)| *n > 0)
        .map(|(name, n)| format!("{name}: {n}"))
        .collect();
    if report.bytes > 0 {
        lines.push(format!("Transferred: {}", size(report.bytes)));
    }
    lines
}

pub(super) fn detail(
    name: &str,
    count: usize,
    report: &Report,
    line: fn(&Item) -> Option<String>,
) -> Vec<String> {
    if count == 0 {
        return Vec::new();
    }
    let mut lines = vec![format!("{name}: {count}")];
    lines.extend(report.items.iter().filter_map(line));
    lines
}

fn skipped_line(item: &Item) -> Option<String> {
    match (&item.outcome, item.decision) {
        (Outcome::Skipped(_), Decision::Conflict) => None,
        (Outcome::Skipped(why), _) => Some(format!("  {} ({why})", item.path)),
        _ => None,
    }
}

pub(super) fn conflict_line(item: &Item) -> Option<String> {
    let (Outcome::Skipped(_), Decision::Conflict) = (&item.outcome, item.decision) else {
        return None;
    };
    Some(format!(
        "  {} local {} {}, remote {} {}",
        item.path,
        short(item.local_hash.as_deref()),
        size(item.size),
        short(item.remote_hash.as_deref()),
        size(item.remote_size)
    ))
}

pub(super) fn failed_line(item: &Item) -> Option<String> {
    match &item.outcome {
        Outcome::Failed(why) => Some(format!("  {}: {why}", item.path)),
        _ => None,
    }
}
