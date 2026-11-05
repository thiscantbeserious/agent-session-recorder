//! Compress plain `.cast` sessions to `.cast.gz`, shared by `agr compress`, the upgrade offer
//! and post-recording compression. Returns data only, callers do the printing.

mod file;
mod plan;

pub use file::{compress_file, compress_file_with_verify, Outcome};
pub use plan::{estimate, plan, CompressPlan, PlannedFile};

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// A session that was not touched, with the reason (live recording, differing leftover).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub path: PathBuf,
    pub reason: String,
}

/// A session whose compression failed, the plain file is still in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failed {
    pub path: PathBuf,
    pub error: String,
}

/// Result of a compress run.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CompressReport {
    pub compressed: usize,
    pub skipped: Vec<Skipped>,
    pub failed: Vec<Failed>,
    /// Non-fatal notes, for example a backup that stayed plain.
    pub warnings: Vec<String>,
    pub bytes_saved: u64,
    /// Planned files not reached because the run was stopped.
    pub not_processed: usize,
}

/// Progress after one file finished, `current` is the file just handled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current: PathBuf,
}

/// Compress every planned file, one at a time. A failure never aborts the run.
/// `stop` is checked between files, `on_progress` is called after each file.
pub fn run(
    plan: &CompressPlan,
    stop: &AtomicBool,
    on_progress: &mut dyn FnMut(&Progress),
) -> CompressReport {
    let mut report = CompressReport::default();
    for path in &plan.live {
        let reason = "live recording".to_string();
        report.skipped.push(Skipped {
            path: path.clone(),
            reason,
        });
    }
    let total = plan.files.len();
    let bytes_total = plan.total_size();
    let mut bytes_done = 0;
    for (i, file) in plan.files.iter().enumerate() {
        if stop.load(Ordering::SeqCst) {
            report.not_processed = total - i;
            break;
        }
        record(&mut report, &file.path, compress_file(&file.path));
        bytes_done += file.size;
        on_progress(&Progress {
            done: i + 1,
            total,
            bytes_done,
            bytes_total,
            current: file.path.clone(),
        });
    }
    report
}

fn record(report: &mut CompressReport, path: &Path, result: anyhow::Result<Outcome>) {
    match result {
        Ok(Outcome::Compressed { saved, warning }) => {
            report.compressed += 1;
            report.bytes_saved += saved;
            report.warnings.extend(warning);
        }
        Ok(Outcome::Skipped(reason)) => report.skipped.push(Skipped {
            path: path.to_path_buf(),
            reason,
        }),
        Err(e) => report.failed.push(Failed {
            path: path.to_path_buf(),
            error: format!("{e:#}"),
        }),
    }
}
