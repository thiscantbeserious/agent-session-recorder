//! Compress command handler

use anyhow::Result;
use humansize::{format_size, BINARY};
use std::ffi::OsStr;
use std::io::IsTerminal;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use agr::storage::compress::{self, CompressPlan, CompressReport, Progress};
use agr::theme::current_theme;
use agr::{Config, StorageManager};

use super::prompt::should_proceed;

/// Compress plain sessions after a preview and a confirmation (`--yes` skips the prompt).
/// `--dry-run` only prints the preview, it never asks and never changes anything.
#[cfg(not(tarpaulin_include))]
pub fn handle(agent: Option<&str>, yes: bool, dry_run: bool) -> Result<()> {
    let storage = StorageManager::new(Config::load()?);
    let plan = compress::plan(&storage, agent)?;
    print_lines(&preview_lines(&plan));
    if plan.files.is_empty() || dry_run {
        return Ok(());
    }
    confirm_and_run(&plan, |q| should_proceed(q, yes))
}

/// Ask once, then compress and print the report, or say nothing changed.
pub(super) fn confirm_and_run(
    plan: &CompressPlan,
    ask: impl FnOnce(&str) -> Result<bool>,
) -> Result<()> {
    let question = format!("Compress {} session(s)?", plan.files.len());
    if !ask(&question)? {
        println!("{}", current_theme().primary_text("No changes made."));
        return Ok(());
    }
    print_lines(&report_lines(&run_interruptible(plan)));
    Ok(())
}

/// Run with Ctrl+C stopping after the current file, progress on stderr only on a terminal.
#[cfg(not(tarpaulin_include))]
fn run_interruptible(plan: &CompressPlan) -> CompressReport {
    let stop = Arc::new(AtomicBool::new(false));
    #[cfg(unix)]
    let _ = signal_hook::flag::register(libc::SIGINT, stop.clone());
    let show = std::io::stderr().is_terminal();
    let width = terminal_size::terminal_size().map_or(80, |(w, _)| w.0 as usize);
    let report = compress::run(plan, &stop, &mut |p| {
        if show {
            eprint!("{CLEAR_LINE}{}", progress_line(p, width));
        }
    });
    if show {
        eprint!("{CLEAR_LINE}");
    }
    report
}

/// Carriage return plus erase-line, so one status line updates in place.
const CLEAR_LINE: &str = "\r\x1b[2K";

/// `Compressing 312/721 · 2.1 GiB of 5.5 GiB · claude/foo.cast`, cut to `width - 1` chars.
pub(super) fn progress_line(p: &Progress, width: usize) -> String {
    let name = p.current.file_name().and_then(OsStr::to_str).unwrap_or("");
    let agent = p
        .current
        .parent()
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .unwrap_or("");
    let line = format!(
        "Compressing {}/{} \u{b7} {} of {} \u{b7} {agent}/{name}",
        p.done,
        p.total,
        short_size(p.bytes_done),
        short_size(p.bytes_total)
    );
    line.chars().take(width.saturating_sub(1)).collect()
}

pub(super) fn print_lines(lines: &[String]) {
    let theme = current_theme();
    for line in lines {
        println!("{}", theme.primary_text(line));
    }
}

fn size(bytes: u64) -> String {
    format_size(bytes, BINARY)
}

fn short_size(bytes: u64) -> String {
    format_size(bytes, BINARY.decimal_places(1))
}

/// Preview of what a run would do: count, current and projected size, skipped live sessions.
pub(super) fn preview_lines(plan: &CompressPlan) -> Vec<String> {
    let mut lines = Vec::new();
    if plan.files.is_empty() {
        lines.push("No plain sessions to compress.".to_string());
    } else {
        let (now, then) = (plan.total_size(), plan.total_projected());
        lines.push(format!("Sessions to compress: {}", plan.files.len()));
        lines.push(format!("Current size: {}", size(now)));
        lines.push(format!("Projected size: {}", size(then)));
        lines.push(format!(
            "Projected savings: {}",
            size(now.saturating_sub(then))
        ));
    }
    if !plan.live.is_empty() {
        lines.push(format!("Skipping {} live session(s):", plan.live.len()));
        lines.extend(plan.live.iter().map(|p| format!("  {}", p.display())));
    }
    lines
}

/// Report of a finished run: compressed, skipped, failed, bytes saved, warnings.
fn report_lines(report: &CompressReport) -> Vec<String> {
    let mut lines = vec![format!("Compressed: {}", report.compressed)];
    lines.push(format!("Saved: {}", size(report.bytes_saved)));
    if report.not_processed > 0 {
        lines.push(format!(
            "Stopped: {} session(s) left untouched",
            report.not_processed
        ));
    }
    if !report.skipped.is_empty() {
        lines.push(format!("Skipped: {}", report.skipped.len()));
        lines.extend(
            report
                .skipped
                .iter()
                .map(|s| format!("  {} ({})", s.path.display(), s.reason)),
        );
    }
    if !report.failed.is_empty() {
        lines.push(format!("Failed: {}", report.failed.len()));
        lines.extend(
            report
                .failed
                .iter()
                .map(|f| format!("  {}: {}", f.path.display(), f.error)),
        );
    }
    lines.extend(report.warnings.iter().map(|w| format!("Warning: {w}")));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use agr::storage::compress::{Failed, PlannedFile, Skipped};
    use std::path::PathBuf;

    fn planned(size: u64, projected: u64) -> PlannedFile {
        PlannedFile {
            path: PathBuf::from("a/x.cast"),
            agent: "a".into(),
            size,
            projected,
        }
    }

    #[test]
    fn preview_lists_count_sizes_and_live_sessions() {
        let plan = CompressPlan {
            files: vec![planned(2048, 512)],
            live: vec![PathBuf::from("a/live.cast")],
        };
        let text = preview_lines(&plan).join("\n");
        assert!(text.contains("Sessions to compress: 1"), "{text}");
        assert!(text.contains("Current size: 2 KiB"), "{text}");
        assert!(text.contains("Projected size: 512 B"), "{text}");
        assert!(text.contains("Skipping 1 live session(s)"), "{text}");
        assert!(text.contains("live.cast"), "{text}");
    }

    #[test]
    fn report_lists_skipped_failed_saved_and_warnings() {
        let report = CompressReport {
            compressed: 2,
            skipped: vec![Skipped {
                path: "a/s.cast".into(),
                reason: "live recording".into(),
            }],
            failed: vec![Failed {
                path: "a/f.cast".into(),
                error: "disk full".into(),
            }],
            warnings: vec!["backup stayed plain".into()],
            bytes_saved: 1024,
            not_processed: 3,
        };
        let text = report_lines(&report).join("\n");
        assert!(text.contains("Compressed: 2"), "{text}");
        assert!(text.contains("Saved: 1 KiB"), "{text}");
        assert!(
            text.contains("Stopped: 3 session(s) left untouched"),
            "{text}"
        );
        assert!(text.contains("s.cast (live recording)"), "{text}");
        assert!(text.contains("f.cast: disk full"), "{text}");
        assert!(text.contains("Warning: backup stayed plain"), "{text}");
    }

    #[test]
    fn progress_line_shows_counts_sizes_and_agent_file() {
        let p = Progress {
            done: 312,
            total: 721,
            bytes_done: 2254857830,
            bytes_total: 5905580032,
            current: PathBuf::from("/s/claude/foo.cast"),
        };
        assert_eq!(
            progress_line(&p, 200),
            "Compressing 312/721 \u{b7} 2.1 GiB of 5.5 GiB \u{b7} claude/foo.cast"
        );
        assert_eq!(progress_line(&p, 16), "Compressing 312");
    }
}
