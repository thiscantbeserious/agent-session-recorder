//! Cast file integrity: diagnose, repair, and interactive check.
//!
//! Provides non-interactive functions for detecting and fixing corrupt lines
//! in asciicast files, plus an interactive wrapper that prompts via stdin.

use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

use anyhow::{bail, Context, Result};

use super::types::{Event, Header};
use super::AsciicastFile;

/// A single issue found during file diagnosis.
#[derive(Debug, Clone)]
pub struct LineDiagnostic {
    /// 1-based line number in the file.
    pub line_number: usize,
    /// What's wrong with this line.
    pub reason: String,
    /// Byte length of the corrupt line.
    pub byte_len: usize,
}

/// Result of diagnosing an asciicast file for issues.
#[derive(Debug, Clone)]
pub struct DiagnoseResult {
    /// Total lines in the file (including header).
    pub total_lines: usize,
    /// Lines that are valid events.
    pub valid_event_lines: usize,
    /// Lines with issues that would be removed by repair.
    pub bad_lines: Vec<LineDiagnostic>,
}

/// Diagnose an asciicast file for corrupt or unparseable lines.
///
/// Scans the entire file without failing, collecting information about
/// every line that cannot be parsed as a valid event.
pub fn diagnose<P: AsRef<Path>>(path: P) -> Result<DiagnoseResult> {
    let path = path.as_ref();
    let file =
        fs::File::open(path).with_context(|| format!("Failed to open file: {:?}", path))?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    // Validate header
    let header_line = lines
        .next()
        .context("File is empty")?
        .context("Failed to read header line")?;

    let header: Header =
        serde_json::from_str(&header_line).context("Failed to parse header")?;
    if header.version != 3 {
        bail!(
            "Only asciicast v3 format is supported (got version {})",
            header.version
        );
    }

    let mut total_lines = 1; // header
    let mut valid_event_lines = 0;
    let mut bad_lines = Vec::new();

    for (line_num, line_result) in lines.enumerate() {
        total_lines += 1;
        let file_line = line_num + 2; // 1-based, header is line 1

        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                bad_lines.push(LineDiagnostic {
                    line_number: file_line,
                    reason: format!("I/O error: {}", e),
                    byte_len: 0,
                });
                continue;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        if line.contains('\0') {
            bad_lines.push(LineDiagnostic {
                line_number: file_line,
                reason: "contains null bytes (file corruption)".to_string(),
                byte_len: line.len(),
            });
            continue;
        }

        if let Err(e) = Event::from_json(&line) {
            bad_lines.push(LineDiagnostic {
                line_number: file_line,
                reason: format!("{}", e),
                byte_len: line.len(),
            });
        } else {
            valid_event_lines += 1;
        }
    }

    Ok(DiagnoseResult {
        total_lines,
        valid_event_lines,
        bad_lines,
    })
}

/// Repair an asciicast file by removing corrupt/unparseable lines.
///
/// Reads the file, keeps only the header and valid event lines, and
/// writes the result back atomically. Returns the number of lines removed.
pub fn repair<P: AsRef<Path>>(path: P) -> Result<usize> {
    let path = path.as_ref();
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read file: {:?}", path))?;

    let mut lines_iter = content.lines();

    let header_line = lines_iter.next().context("File is empty")?;
    let header: Header =
        serde_json::from_str(header_line).context("Failed to parse header")?;
    if header.version != 3 {
        bail!(
            "Only asciicast v3 format is supported (got version {})",
            header.version
        );
    }

    let mut removed = 0;
    let temp_path = path.with_extension("cast.tmp");
    let mut file = fs::File::create(&temp_path)
        .with_context(|| format!("Failed to create temp file: {:?}", temp_path))?;

    writeln!(file, "{}", header_line)?;

    for line in lines_iter {
        if line.trim().is_empty() {
            continue;
        }

        if line.contains('\0') {
            removed += 1;
            continue;
        }

        if Event::from_json(line).is_err() {
            removed += 1;
            continue;
        }

        writeln!(file, "{}", line)?;
    }

    file.sync_all()?;
    drop(file);

    fs::rename(&temp_path, path).with_context(|| {
        let _ = fs::remove_file(&temp_path);
        format!("Failed to replace file: {:?}", path)
    })?;

    Ok(removed)
}

/// Check a cast file for corruption and offer to repair interactively.
///
/// Returns Ok(()) if the file is clean or was repaired. Returns Err if the
/// user declines repair or if repair fails.
#[cfg(not(tarpaulin_include))]
pub fn check_file_integrity(path: &Path) -> Result<()> {
    if AsciicastFile::parse(path).is_ok() {
        return Ok(());
    }

    let diagnosis = diagnose(path)?;
    if diagnosis.bad_lines.is_empty() {
        anyhow::bail!("Failed to parse file: {:?}", path);
    }

    eprintln!(
        "File has {} corrupt line(s) ({} valid events):",
        diagnosis.bad_lines.len(),
        diagnosis.valid_event_lines
    );
    for diag in &diagnosis.bad_lines {
        eprintln!(
            "  line {}: {} ({} bytes)",
            diag.line_number, diag.reason, diag.byte_len
        );
    }
    print!(
        "Remove {} corrupt line(s) and continue? [y/N]: ",
        diagnosis.bad_lines.len()
    );
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes") {
        let removed = repair(path)?;
        println!("Removed {} corrupt line(s).", removed);
        Ok(())
    } else {
        anyhow::bail!(
            "Aborting: file has corrupt lines. Run 'agr repair {:?}' to fix.",
            path
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn diagnose_detects_null_byte_lines() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.cast");

        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, "{{\"version\":3}}").unwrap();
        writeln!(f, "[0.1, \"o\", \"hello\"]").unwrap();
        f.write_all(&[0u8; 100]).unwrap();
        writeln!(f).unwrap();
        writeln!(f, "[0.2, \"o\", \" world\"]").unwrap();
        drop(f);

        let result = diagnose(&path).unwrap();
        assert_eq!(result.bad_lines.len(), 1);
        assert_eq!(result.bad_lines[0].line_number, 3);
        assert!(result.bad_lines[0].reason.contains("null bytes"));
        assert_eq!(result.valid_event_lines, 2);
    }

    #[test]
    fn diagnose_clean_file_has_no_bad_lines() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.cast");

        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, "{{\"version\":3}}").unwrap();
        writeln!(f, "[0.1, \"o\", \"hello\"]").unwrap();
        writeln!(f, "[0.2, \"o\", \" world\"]").unwrap();
        drop(f);

        let result = diagnose(&path).unwrap();
        assert!(result.bad_lines.is_empty());
        assert_eq!(result.valid_event_lines, 2);
    }

    #[test]
    fn repair_removes_corrupt_lines() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.cast");

        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, "{{\"version\":3}}").unwrap();
        writeln!(f, "[0.1, \"o\", \"hello\"]").unwrap();
        f.write_all(&[0u8; 100]).unwrap();
        writeln!(f).unwrap();
        writeln!(f, "[0.2, \"o\", \" world\"]").unwrap();
        drop(f);

        assert!(AsciicastFile::parse(&path).is_err());

        let removed = repair(&path).unwrap();
        assert_eq!(removed, 1);

        let file = AsciicastFile::parse(&path).unwrap();
        assert_eq!(file.events.len(), 2);
    }

    #[test]
    fn repair_clean_file_removes_nothing() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.cast");

        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, "{{\"version\":3}}").unwrap();
        writeln!(f, "[0.1, \"o\", \"hello\"]").unwrap();
        drop(f);

        let removed = repair(&path).unwrap();
        assert_eq!(removed, 0);
    }
}
