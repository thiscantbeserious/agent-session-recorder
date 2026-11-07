//! Sync lock `<storage>/.agr-sync.lock`, pid inside, stale pids are reclaimed.

use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use super::index::LOCK_FILE;
use super::run::SyncError;
use crate::files::lock::{is_pid_alive, LockInfo};

/// Held lock, removed on drop.
#[derive(Debug)]
pub(super) struct SyncLock(PathBuf);

impl SyncLock {
    /// Take the lock, `Busy` when a live process holds it.
    pub(super) fn acquire(storage_dir: &Path) -> Result<Self, SyncError> {
        let path = storage_dir.join(LOCK_FILE);
        if create(&path)? {
            return Ok(Self(path));
        }
        if holder_alive(&path) {
            return Err(SyncError::Busy);
        }
        // ponytail: two runs can both see a stale holder, one removes the other's fresh lock
        // between our check and our remove. Window is microseconds and needs a dead pid
        // first, upgrade to an flock on the lock file if it ever shows up.
        let _ = std::fs::remove_file(&path);
        if create(&path)? {
            return Ok(Self(path));
        }
        Err(SyncError::Busy)
    }
}

impl Drop for SyncLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// `Ok(false)` when the file already exists.
fn create(path: &Path) -> Result<bool, SyncError> {
    let file = OpenOptions::new().write(true).create_new(true).open(path);
    let mut file = match file {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::AlreadyExists => return Ok(false),
        Err(e) => return Err(SyncError::Failed(format!("sync lock: {e}"))),
    };
    let info = LockInfo {
        pid: std::process::id(),
        started: chrono::Utc::now().to_rfc3339(),
    };
    let json = serde_json::to_string(&info).unwrap_or_default();
    file.write_all(json.as_bytes())
        .map_err(|e| SyncError::Failed(format!("sync lock: {e}")))?;
    Ok(true)
}

fn holder_alive(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    serde_json::from_str::<LockInfo>(&text).is_ok_and(|info| is_pid_alive(info.pid))
}
