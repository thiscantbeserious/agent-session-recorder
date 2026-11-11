//! Sync lock `<storage>/.agr-sync.lock`, pid inside, stale pids are reclaimed.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::index::LOCK_FILE;
use super::run::SyncError;
use crate::files::lock::{is_pid_alive, LockInfo};

/// Held lock, removed on drop.
#[derive(Debug)]
pub struct SyncLock(PathBuf);

impl SyncLock {
    /// Take the lock, `Busy` when a live process holds it.
    pub fn acquire(storage_dir: &Path) -> Result<Self, SyncError> {
        let path = storage_dir.join(LOCK_FILE);
        if !wait_for_holder(&path)? {
            return Ok(Self(path));
        }
        // ponytail: two runs can both see a stale holder, one removes the other's fresh lock
        // between our check and our remove. Window is microseconds and needs a dead pid
        // first, upgrade to an flock on the lock file if it ever shows up.
        let _ = std::fs::remove_file(&path);
        if create(&path)? {
            return Ok(Self(path));
        }
        Err(SyncError::Busy(path))
    }
}

/// Tries to create the lock, a lock that is released between our failed create and our read is
/// tried again `RETRIES` times. `Ok(false)` when the lock is ours, `Ok(true)` when the holder
/// is dead (stale), `Busy` when it is alive or keeps appearing and vanishing.
fn wait_for_holder(path: &Path) -> Result<bool, SyncError> {
    for _ in 0..RETRIES {
        if create(path)? {
            return Ok(false);
        }
        match holder(path) {
            Holder::Gone => {}
            Holder::Alive => break,
            Holder::Dead => return Ok(true),
        }
    }
    Err(SyncError::Busy(path.to_path_buf()))
}

/// Create attempts while the holder keeps vanishing.
const RETRIES: usize = 5;

impl Drop for SyncLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// `Ok(false)` when the lock exists. The JSON goes to a temp file first and is linked into
/// place, so the lock never appears empty to a second process.
fn create(path: &Path) -> Result<bool, SyncError> {
    create_with(path, |from, to| std::fs::hard_link(from, to))
}

fn create_with(
    path: &Path,
    link: impl Fn(&Path, &Path) -> std::io::Result<()>,
) -> Result<bool, SyncError> {
    let fail = |e: std::io::Error| SyncError::Failed(format!("sync lock: {e}"));
    let info = LockInfo {
        pid: std::process::id(),
        started: chrono::Utc::now().to_rfc3339(),
    };
    let json = serde_json::to_string(&info).unwrap_or_default();
    // Unique per call, threads share the pid and a shared temp would share the lock's inode.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("tmp{}-{n}", info.pid));
    std::fs::write(&tmp, &json).map_err(fail)?;
    let linked = link(&tmp, path);
    let _ = std::fs::remove_file(&tmp);
    match linked {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => Ok(false),
        Err(_) => create_new(path, &json).map_err(fail),
    }
}

/// No hard links on this filesystem, create and write in place. A second process may see the
/// file empty for a moment, `holder_alive` counts that as held.
fn create_new(path: &Path, json: &str) -> std::io::Result<bool> {
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => std::io::Write::write_all(&mut file, json.as_bytes()).map(|()| true),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e),
    }
}

enum Holder {
    Alive,
    Dead,
    /// Released between our failed create and our read, just try again.
    Gone,
}

/// Only a readable lock of a dead pid is stale. Empty (still being written) or unreadable
/// counts as held, so a lock is never removed under a live holder.
fn holder(path: &Path) -> Holder {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == ErrorKind::NotFound => Holder::Gone,
        Err(_) => Holder::Alive,
        Ok(text) if text.is_empty() => Holder::Alive,
        Ok(text) => match serde_json::from_str::<LockInfo>(&text) {
            Ok(info) if !is_pid_alive(info.pid) => Holder::Dead,
            _ => Holder::Alive,
        },
    }
}

// inline: SyncLock and its hard-link fallback are private to the engine
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filesystem_without_hard_links_falls_back_to_create_new() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOCK_FILE);
        let no_links = |_: &Path, _: &Path| Err(std::io::Error::from(ErrorKind::Unsupported));
        assert!(matches!(create_with(&path, no_links), Ok(true)));
        assert!(matches!(holder(&path), Holder::Alive));
        assert!(matches!(create_with(&path, no_links), Ok(false)));
    }

    #[test]
    fn empty_lock_file_is_busy_not_reclaimed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOCK_FILE);
        std::fs::write(&path, "").unwrap();
        assert!(matches!(
            SyncLock::acquire(dir.path()),
            Err(SyncError::Busy(_))
        ));
        assert!(path.exists());
    }

    #[test]
    fn stale_lock_is_reclaimed_and_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOCK_FILE);
        std::fs::write(&path, r#"{"pid":4294967,"started":"x"}"#).unwrap();
        let lock = SyncLock::acquire(dir.path()).unwrap();
        assert!(matches!(holder(&path), Holder::Alive));
        drop(lock);
        assert!(!path.exists());
    }
}
