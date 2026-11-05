//! Rename suggestion after analysis, keeps the `.cast` or `.cast.gz` suffix.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;

use agr::analyzer::{AnalyzerService, ValidatedMarker};
use agr::files::codec;
use agr::files::filename::rename_file;

/// Current file name without the session suffix, the input for the LLM suggestion.
pub(super) fn current_stem(filepath: &Path) -> String {
    let name = filepath
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("");
    codec::session_stem(name).to_string()
}

/// Path the suggested name would give, same directory and suffix.
pub(super) fn target_path(filepath: &Path, suggested: &str) -> PathBuf {
    filepath.with_file_name(format!("{suggested}{}", codec::session_suffix(filepath)))
}

/// Rename to the suggested stem, returns the new path.
pub(super) fn apply_rename(filepath: &Path, suggested: &str) -> Result<PathBuf> {
    Ok(rename_file(filepath, suggested)?)
}

fn confirm(question: &str) -> Result<bool> {
    print!("\n{question} [y/N]: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    let answer = input.trim();
    Ok(answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes"))
}

/// Suggest a descriptive rename via LLM and prompt user to accept.
#[cfg(not(tarpaulin_include))]
pub(super) fn handle_rename_suggestion(
    service: &AnalyzerService,
    markers: &[ValidatedMarker],
    total_duration: f64,
    filepath: &Path,
    effective_timeout_secs: u64,
) -> Result<()> {
    if markers.is_empty() {
        return Ok(());
    }
    let timeout = Duration::from_secs(effective_timeout_secs);
    let current = current_stem(filepath);
    let Some(suggested) = service.suggest_rename(markers, total_duration, timeout, &current) else {
        return Ok(());
    };
    let target = target_path(filepath, &suggested);
    if target == *filepath || target.exists() {
        return Ok(());
    }
    let name = target
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("");
    if confirm(&format!("Rename to \"{name}\"?"))? {
        println!(
            "Renamed to: {}",
            apply_rename(filepath, &suggested)?.display()
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "rename_tests.rs"]
mod tests;
