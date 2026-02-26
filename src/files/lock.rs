//! Lock file utilities for cast files.
//!
//! Provides lock creation, checking, removal, stale detection, and
//! file finder helpers to handle recording rename race conditions.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Metadata stored in a lock file to identify the owning process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockInfo {
    pub pid: u32,
    pub started: String,
}

/// Get the hidden lock file path for a given cast file.
///
/// Prepends a dot to the filename component so the lock file is hidden in directory listings.
/// Given `dir/session.cast`, produces `dir/.session.cast.lock`.
/// Edge case: if the filename already starts with a dot (e.g. `.hidden.cast`),
/// the result will have two dots (e.g. `..hidden.cast.lock`), which is valid on Unix.
pub fn lock_path_for(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let filename = path.file_name().unwrap_or_default();
    let hidden_name = format!(".{}.lock", filename.to_string_lossy());
    if parent.as_os_str().is_empty() {
        PathBuf::from(hidden_name)
    } else {
        parent.join(hidden_name)
    }
}

/// Get the legacy (visible) lock file path for a given cast file.
///
/// Appends `.lock` without a dot prefix, producing the old format `session.cast.lock`.
/// Used for dual-format compatibility checks and migration. Intended to be removed
/// in a future cleanup pass once old-format files are no longer in use.
pub fn old_lock_path_for(path: &Path) -> PathBuf {
    let mut lock = path.as_os_str().to_owned();
    lock.push(".lock");
    PathBuf::from(lock)
}

/// Create a lock file for the given cast file path.
///
/// Writes JSON with the current PID and an ISO8601 timestamp.
pub fn create_lock(path: &Path) -> Result<()> {
    let lock_path = lock_path_for(path);
    let info = LockInfo {
        pid: std::process::id(),
        started: chrono::Utc::now().to_rfc3339(),
    };
    let json = serde_json::to_string(&info).context("Failed to serialize lock info")?;
    fs::write(&lock_path, json)
        .with_context(|| format!("Failed to write lock file: {}", lock_path.display()))
}

/// Read lock info from a specific lock file path if the owning PID is still alive.
///
/// Returns `None` if the lock file is missing, malformed, or the PID is dead.
fn read_lock_at_path(lock_path: &Path) -> Option<LockInfo> {
    let contents = fs::read_to_string(lock_path).ok()?;
    let info: LockInfo = serde_json::from_str(&contents).ok()?;
    if !is_pid_alive(info.pid) {
        return None;
    }
    Some(info)
}

/// Read lock info if the lock file exists and the owning PID is still alive.
///
/// Checks the new hidden format first, then falls back to the old visible format.
/// Returns `None` if no live lock is found in either format.
pub fn read_lock(path: &Path) -> Option<LockInfo> {
    read_lock_at_path(&lock_path_for(path)).or_else(|| read_lock_at_path(&old_lock_path_for(path)))
}

/// Remove the lock file for the given cast file path (best-effort).
///
/// Removes both the new hidden-format and old visible-format lock files.
/// Silently ignores errors if files do not exist or cannot be removed.
pub fn remove_lock(path: &Path) {
    let _ = fs::remove_file(lock_path_for(path));
    let _ = fs::remove_file(old_lock_path_for(path));
}

/// Check a single lock file path and handle stale or active locks.
///
/// If the lock is stale (dead PID or malformed), removes it and returns `Ok(())`.
/// If the lock is active (live PID), returns an error.
/// If no lock file exists, returns `Ok(())`.
fn check_lock_at_path(lock_path: &Path, cast_path: &Path) -> Result<()> {
    let contents = match fs::read_to_string(lock_path) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };
    let info: LockInfo = match serde_json::from_str(&contents) {
        Ok(i) => i,
        Err(_) => {
            let _ = fs::remove_file(lock_path);
            return Ok(());
        }
    };
    if is_pid_alive(info.pid) {
        anyhow::bail!(
            "File is locked by an active recording: {}",
            cast_path.display()
        );
    }
    let _ = fs::remove_file(lock_path);
    Ok(())
}

/// Verify that the given cast file is not actively locked by a live process.
///
/// Checks both the new hidden format and the old visible format. Auto-cleans stale
/// lock files in either format. Bails if an active lock exists in either format.
pub fn check_not_locked(path: &Path) -> Result<()> {
    check_lock_at_path(&lock_path_for(path), path)?;
    check_lock_at_path(&old_lock_path_for(path), path)?;
    Ok(())
}

/// Scan a directory for a `.cast` file matching the given inode number.
///
/// Useful for locating a file that was renamed while being recorded.
#[cfg(unix)]
pub fn find_by_inode(dir: &Path, target_inode: u64) -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("cast") {
            continue;
        }
        if let Ok(meta) = fs::metadata(&path) {
            if meta.ino() == target_inode {
                return Some(path);
            }
        }
    }
    None
}

/// Scan a directory for a `.cast` file whose first line matches the given header.
///
/// Useful for locating a file by its asciicast header content after a rename.
pub fn find_by_header(dir: &Path, target_header: &str) -> Option<PathBuf> {
    use std::io::{BufRead, BufReader};
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("cast") {
            continue;
        }
        if let Ok(file) = fs::File::open(&path) {
            let mut reader = BufReader::new(file);
            let mut first_line = String::new();
            if reader.read_line(&mut first_line).is_ok() && first_line.trim_end() == target_header {
                return Some(path);
            }
        }
    }
    None
}

/// Check whether a process with the given PID is still running.
///
/// Uses `kill(pid, 0)` which checks for process existence without sending a signal.
/// Returns `true` if the process exists (even if owned by another user — EPERM).
#[cfg(unix)]
pub(crate) fn is_pid_alive(pid: u32) -> bool {
    // SAFETY: kill with signal 0 only checks process existence, no signal is sent.
    let ret = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if ret == 0 {
        return true;
    }
    // EPERM means the process exists but belongs to another user
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
pub(crate) fn is_pid_alive(_pid: u32) -> bool {
    false
}
