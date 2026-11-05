//! Compress plain `.cast` sessions to `.cast.gz`, shared by `agr compress`, the upgrade offer
//! and post-recording compression. Returns data only, callers do the printing.

mod file;
mod plan;

pub use file::{compress_file, compress_file_with_verify, Outcome};
pub use plan::{estimate, plan, CompressPlan, PlannedFile};

use std::path::{Path, PathBuf};

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
}

/// Compress every planned file, one at a time. A failure never aborts the run.
pub fn run(plan: &CompressPlan) -> CompressReport {
    let mut report = CompressReport::default();
    for path in &plan.live {
        let reason = "live recording".to_string();
        report.skipped.push(Skipped {
            path: path.clone(),
            reason,
        });
    }
    for file in &plan.files {
        record(&mut report, &file.path, compress_file(&file.path));
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
