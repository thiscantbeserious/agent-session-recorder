//! Compress one session file, the plain file is removed only after the `.cast.gz` is
//! verified equal and fsynced (`atomic_write` syncs file and directory).

use crate::files::{backup, codec, hidden_auxiliary_path};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

/// What `compress_file` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Plain file replaced by `.cast.gz`. `warning` is set when only the backup step failed.
    Compressed { saved: u64, warning: Option<String> },
    /// Nothing changed, with the reason.
    Skipped(String),
}

/// `x.cast` becomes `x.cast.gz` and its backup (if any) becomes `.x.cast.gz.bak`.
/// A leftover `x.cast` next to an equal `x.cast.gz` (crash after rename) is removed, a
/// differing one is skipped. Any failure before the plain file is removed leaves it untouched.
pub fn compress_file(path: &Path) -> Result<Outcome> {
    compress_file_with_verify(path, codec::same_content)
}

/// `compress_file` with the equality check injectable, for failure tests.
#[doc(hidden)]
pub fn compress_file_with_verify(
    path: &Path,
    verify: impl Fn(&Path, &Path) -> Result<bool>,
) -> Result<Outcome> {
    let gz = codec::gzip_sibling(path);
    if gz.exists() {
        return finish_leftover(path, &gz, &verify);
    }
    codec::transcode(path, &gz)?;
    match verify(path, &gz) {
        Ok(true) => finalize(path, &gz),
        Ok(false) => discard(&gz, "verification failed, content differs"),
        Err(e) => discard(&gz, &format!("verification failed: {e:#}")),
    }
}

/// Remove the unverified gzip and report the failure, the plain file stays.
fn discard(gz: &Path, why: &str) -> Result<Outcome> {
    let _ = fs::remove_file(gz);
    bail!("{why}: {}", gz.display())
}

fn finish_leftover(
    path: &Path,
    gz: &Path,
    verify: &impl Fn(&Path, &Path) -> Result<bool>,
) -> Result<Outcome> {
    if verify(path, gz)? {
        return finalize(path, gz);
    }
    let reason = format!("{} exists and differs, left untouched", gz.display());
    Ok(Outcome::Skipped(reason))
}

/// The gzip is verified: convert the backup first (a failure there leaves the plain file and
/// its backup untouched), keep the age, then remove the plain file.
fn finalize(path: &Path, gz: &Path) -> Result<Outcome> {
    let warning = match compress_backup(path, gz) {
        Ok(warning) => warning,
        Err(e) => return discard(gz, &format!("backup not compressed: {e:#}")),
    };
    let plain_meta = fs::metadata(path)?;
    keep_mtime(gz, &plain_meta);
    fs::remove_file(path).with_context(|| format!("Failed to remove {}", path.display()))?;
    let saved = plain_meta.len().saturating_sub(fs::metadata(gz)?.len());
    Ok(Outcome::Compressed { saved, warning })
}

/// Age based cleanup reads the mtime, so the gzip inherits it (best-effort).
fn keep_mtime(gz: &Path, plain: &fs::Metadata) {
    if let Ok(modified) = plain.modified() {
        let _ = fs::File::options()
            .write(true)
            .open(gz)
            .and_then(|f| f.set_modified(modified));
    }
}

/// `.x.cast.bak` (plain) becomes `.x.cast.gz.bak` (gzip bytes, same as a backup of the gz).
/// An existing `.x.cast.gz.bak` is never overwritten, the plain backup stays and a warning
/// is returned.
fn compress_backup(path: &Path, gz: &Path) -> Result<Option<String>> {
    let old = backup::backup_path_for(path);
    if !old.exists() {
        return Ok(None);
    }
    let target = backup::backup_path_for(gz);
    if target.exists() {
        return Ok(Some(format!(
            "{} already exists, backup of {} left plain",
            target.display(),
            path.display()
        )));
    }
    // Staging name ends in `.cast.gz` so `atomic_write` gzip-encodes it.
    let staging = hidden_auxiliary_path(&target, codec::GZIP_SUFFIX);
    let result = stage_backup(&old, &staging, &target);
    if result.is_err() {
        let _ = fs::remove_file(&staging);
    }
    result.map(|()| None)
}

fn stage_backup(old: &Path, staging: &Path, target: &Path) -> Result<()> {
    codec::transcode(old, staging)?;
    if !codec::same_content(old, staging)? {
        bail!("backup verification failed for {}", old.display());
    }
    fs::rename(staging, target)?;
    fs::remove_file(old)?;
    Ok(())
}
