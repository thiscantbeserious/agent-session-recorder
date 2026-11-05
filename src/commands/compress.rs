//! Compress command handler

use anyhow::Result;
use humansize::{format_size, BINARY};

use agr::storage::compress::{self, CompressPlan, CompressReport};
use agr::theme::current_theme;
use agr::{Config, StorageManager};

use super::prompt::should_proceed;

/// Compress plain sessions after a preview and a confirmation (`--yes` skips the prompt).
#[cfg(not(tarpaulin_include))]
pub fn handle(agent: Option<&str>, yes: bool) -> Result<()> {
    let storage = StorageManager::new(Config::load()?);
    let plan = compress::plan(&storage, agent)?;
    let theme = current_theme();
    print_lines(&preview_lines(&plan));
    if plan.files.is_empty() {
        return Ok(());
    }
    let question = format!("Compress {} session(s)?", plan.files.len());
    if !should_proceed(&question, yes)? {
        println!("{}", theme.primary_text("No changes made."));
        return Ok(());
    }
    print_lines(&report_lines(&compress::run(&plan)));
    Ok(())
}

fn print_lines(lines: &[String]) {
    let theme = current_theme();
    for line in lines {
        println!("{}", theme.primary_text(line));
    }
}

fn size(bytes: u64) -> String {
    format_size(bytes, BINARY)
}

/// Preview of what a run would do: count, current and projected size, skipped live sessions.
fn preview_lines(plan: &CompressPlan) -> Vec<String> {
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
        };
        let text = report_lines(&report).join("\n");
        assert!(text.contains("Compressed: 2"), "{text}");
        assert!(text.contains("Saved: 1 KiB"), "{text}");
        assert!(text.contains("s.cast (live recording)"), "{text}");
        assert!(text.contains("f.cast: disk full"), "{text}");
        assert!(text.contains("Warning: backup stayed plain"), "{text}");
    }
}
