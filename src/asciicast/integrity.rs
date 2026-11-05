//! Cast file integrity: diagnose, repair, and interactive check.
//!
//! Provides non-interactive functions for detecting and fixing corrupt lines
//! in asciicast files, plus an interactive wrapper that prompts via stdin.

use std::io::{self, BufRead, Write};
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::files::codec;

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

/// Verdict on one raw line, shared by `diagnose` and `repair` so they agree.
enum Verdict {
    Blank,
    Valid,
    Bad(String),
}

fn classify(raw: &[u8]) -> Verdict {
    let Ok(line) = std::str::from_utf8(raw) else {
        return Verdict::Bad("invalid UTF-8".to_string());
    };
    if line.trim().is_empty() {
        Verdict::Blank
    } else if line.contains('\0') {
        Verdict::Bad("contains null bytes (file corruption)".to_string())
    } else {
        match Event::from_json(line) {
            Ok(_) => Verdict::Valid,
            Err(e) => Verdict::Bad(e.to_string()),
        }
    }
}

/// Line reader over raw bytes. A read error is a decoder error and terminal, a gzip stream
/// cannot resume after one, so callers stop on the first `Err`.
struct RawLines {
    reader: Box<dyn BufRead>,
    buf: Vec<u8>,
}

impl RawLines {
    fn next_line(&mut self) -> io::Result<Option<&[u8]>> {
        self.buf.clear();
        if self.reader.read_until(b'\n', &mut self.buf)? == 0 {
            return Ok(None);
        }
        let mut end = self.buf.len();
        if self.buf[..end].ends_with(b"\n") {
            end -= 1;
            if self.buf[..end].ends_with(b"\r") {
                end -= 1;
            }
        }
        Ok(Some(&self.buf[..end]))
    }
}

/// Open a v3 cast file, returning the line reader positioned after the header and the header.
fn open_v3(path: &Path) -> Result<(RawLines, String)> {
    let mut lines = RawLines {
        reader: codec::open_reader(path)?,
        buf: Vec::new(),
    };
    let raw = lines
        .next_line()
        .context("Failed to read header line")?
        .context("File is empty")?;
    let header_line = String::from_utf8(raw.to_vec()).context("Header line is not valid UTF-8")?;
    let header: Header = serde_json::from_str(&header_line).context("Failed to parse header")?;
    if header.version != 3 {
        bail!(
            "Only asciicast v3 format is supported (got version {})",
            header.version
        );
    }
    Ok((lines, header_line))
}

/// Diagnose an asciicast file for corrupt or unparseable lines.
///
/// Scans the entire file without failing, collecting information about
/// every line that cannot be parsed as a valid event. A decoder error ends
/// the scan and is recorded once.
pub fn diagnose<P: AsRef<Path>>(path: P) -> Result<DiagnoseResult> {
    let (mut lines, _) = open_v3(path.as_ref())?;
    let mut result = DiagnoseResult {
        total_lines: 1, // header
        valid_event_lines: 0,
        bad_lines: Vec::new(),
    };
    loop {
        let line_number = result.total_lines + 1;
        match lines.next_line() {
            Ok(None) => break,
            Ok(Some(raw)) => result.record(line_number, classify(raw), raw.len()),
            Err(e) => {
                let reason = format!("I/O error: {}", e);
                result.record(line_number, Verdict::Bad(reason), 0);
                break;
            }
        }
    }
    Ok(result)
}

impl DiagnoseResult {
    fn record(&mut self, line_number: usize, verdict: Verdict, byte_len: usize) {
        self.total_lines += 1;
        match verdict {
            Verdict::Blank => {}
            Verdict::Valid => self.valid_event_lines += 1,
            Verdict::Bad(reason) => self.bad_lines.push(LineDiagnostic {
                line_number,
                reason,
                byte_len,
            }),
        }
    }
}

/// Repair an asciicast file by removing corrupt/unparseable lines.
///
/// Streams the file through the codec, keeps only the header and valid event
/// lines, and writes the result back atomically in the same format. Returns
/// the number of lines removed. A decoder error aborts and leaves the file as is.
pub fn repair<P: AsRef<Path>>(path: P) -> Result<usize> {
    let path = path.as_ref();
    let (mut lines, header_line) = open_v3(path)?;
    let mut removed = 0;
    codec::atomic_write(path, |w| {
        writeln!(w, "{}", header_line)?;
        while let Some(raw) = lines.next_line().context("Failed to read line")? {
            match classify(raw) {
                Verdict::Blank => {}
                Verdict::Bad(_) => removed += 1,
                Verdict::Valid => {
                    w.write_all(raw)?;
                    writeln!(w)?;
                }
            }
        }
        Ok(())
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

    print_diagnosis(&diagnosis);
    if confirm_removal(diagnosis.bad_lines.len())? {
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

#[cfg(not(tarpaulin_include))]
fn print_diagnosis(diagnosis: &DiagnoseResult) {
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
}

#[cfg(not(tarpaulin_include))]
fn confirm_removal(count: usize) -> Result<bool> {
    print!("Remove {} corrupt line(s) and continue? [y/N]: ", count);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    let answer = input.trim();
    Ok(answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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
