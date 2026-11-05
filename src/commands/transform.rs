//! Transform command handler for asciicast file transformations.
//!
//! Provides CLI support for applying transforms to asciicast recordings,
//! such as silence removal.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use agr::asciicast::{AsciicastFile, SilenceRemoval, Transform, DEFAULT_SILENCE_THRESHOLD};
use agr::theme::current_theme;
use agr::Config;

use agr::asciicast::integrity::check_file_integrity;

/// Resolve the threshold to use for silence removal.
///
/// Priority order:
/// 1. CLI argument (explicit user intent)
/// 2. Header's `idle_time_limit` (recording author's intent)
/// 3. Default constant (2.0 seconds)
pub fn resolve_threshold(cli_threshold: Option<f64>, header_idle_time_limit: Option<f64>) -> f64 {
    cli_threshold
        .or(header_idle_time_limit)
        .unwrap_or(DEFAULT_SILENCE_THRESHOLD)
}

/// Validate that a threshold is valid for silence removal.
///
/// Returns an error if the threshold is:
/// - Zero or negative
/// - NaN
/// - Infinity
pub fn validate_threshold(threshold: f64) -> Result<()> {
    if threshold <= 0.0 {
        bail!("Threshold must be positive (got: {})", threshold);
    }
    if !threshold.is_finite() {
        bail!(
            "Threshold must be a finite number (got: {})",
            if threshold.is_nan() {
                "NaN"
            } else {
                "Infinity"
            }
        );
    }
    Ok(())
}

/// Where the threshold came from, for the report line.
fn threshold_source(cli: Option<f64>, header_idle: Option<f64>) -> &'static str {
    if cli.is_some() {
        "CLI argument"
    } else if header_idle.is_some() {
        "header idle_time_limit"
    } else {
        "default"
    }
}

/// Print the duration summary after the transform.
fn print_saved(original: f64, new: f64) {
    let theme = current_theme();
    let saved = original - new;
    let line = if saved > 0.0 {
        format!(
            "Duration reduced from {:.1}s to {:.1}s (saved {:.1}s)",
            original, new, saved
        )
    } else {
        "No changes needed (all intervals below threshold)".to_string()
    };
    println!("{}", theme.primary_text(&line));
}

/// Print where the result went.
fn print_destination(output: Option<&str>, output_path: &Path) {
    let theme = current_theme();
    let line = match output {
        Some(_) => format!("Output written to: {}", output_path.display()),
        None => "File modified in-place".to_string(),
    };
    println!("{}", theme.primary_text(&line));
}

/// Load the file, refusing corrupt or locked ones.
fn load_for_transform(filepath: &Path) -> Result<AsciicastFile> {
    check_file_integrity(filepath)?;
    agr::files::lock::check_not_locked(filepath)?;
    AsciicastFile::parse(filepath)
        .with_context(|| format!("Failed to parse asciicast file: {}", filepath.display()))
}

/// Handle the transform command with silence removal.
///
/// Applies silence removal transform to the specified file, either modifying
/// it in-place or writing to a separate output file.
#[cfg(not(tarpaulin_include))]
pub fn handle_remove_silence(
    file: &str,
    threshold: Option<f64>,
    output: Option<&str>,
) -> Result<()> {
    let config = Config::load()?;
    let filepath = super::resolve_existing_session(file, &config)?;
    let mut cast = load_for_transform(&filepath)?;
    let idle = cast.header.idle_time_limit;
    let effective = resolve_threshold(threshold, idle);
    validate_threshold(effective)?;
    let message = format!(
        "Applying silence removal with {:.2}s threshold (from {})",
        effective,
        threshold_source(threshold, idle)
    );
    println!("{}", current_theme().primary_text(&message));
    let original = cast.duration();
    SilenceRemoval::new(effective).transform(&mut cast.events);
    let output_path = output.map_or_else(|| filepath.clone(), PathBuf::from);
    cast.write(&output_path)
        .with_context(|| format!("Failed to write output file: {}", output_path.display()))?;
    print_saved(original, cast.duration());
    print_destination(output, &output_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Threshold Resolution Tests
    // ========================================================================

    #[test]
    fn resolve_threshold_cli_takes_priority() {
        // CLI argument overrides everything
        let result = resolve_threshold(Some(3.0), Some(1.5));
        assert!((result - 3.0).abs() < 0.001);
    }

    #[test]
    fn resolve_threshold_header_used_when_no_cli() {
        // Header's idle_time_limit used when CLI not specified
        let result = resolve_threshold(None, Some(1.5));
        assert!((result - 1.5).abs() < 0.001);
    }

    #[test]
    fn resolve_threshold_default_when_nothing_specified() {
        // Default used when neither CLI nor header specified
        let result = resolve_threshold(None, None);
        assert!((result - DEFAULT_SILENCE_THRESHOLD).abs() < 0.001);
    }

    #[test]
    fn resolve_threshold_cli_overrides_header() {
        // Explicit CLI value overrides header even if header exists
        let result = resolve_threshold(Some(5.0), Some(1.0));
        assert!((result - 5.0).abs() < 0.001);
    }

    // ========================================================================
    // Threshold Validation Tests
    // ========================================================================

    #[test]
    fn validate_threshold_accepts_positive_values() {
        assert!(validate_threshold(0.1).is_ok());
        assert!(validate_threshold(1.0).is_ok());
        assert!(validate_threshold(100.0).is_ok());
    }

    #[test]
    fn validate_threshold_rejects_zero() {
        let result = validate_threshold(0.0);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("positive"));
    }

    #[test]
    fn validate_threshold_rejects_negative() {
        let result = validate_threshold(-1.0);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("positive"));
    }

    #[test]
    fn validate_threshold_rejects_nan() {
        let result = validate_threshold(f64::NAN);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("finite") || err_msg.contains("NaN"));
    }

    #[test]
    fn validate_threshold_rejects_positive_infinity() {
        let result = validate_threshold(f64::INFINITY);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("finite") || err_msg.contains("Infinity"));
    }

    #[test]
    fn validate_threshold_rejects_negative_infinity() {
        let result = validate_threshold(f64::NEG_INFINITY);
        assert!(result.is_err());
        // Could be rejected as negative or non-finite, both are valid
    }
}
