//! Queues codec warnings for the status line while a TUI owns the terminal.

use std::sync::Mutex;

use crate::files::codec;

/// Serialises tests that share the process wide queue.
#[cfg(test)]
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

static QUEUE: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub(crate) fn push(message: &str) {
    QUEUE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(message.to_string());
}

/// Take the newest queued warning, dropping older ones.
pub fn take_latest() -> Option<String> {
    let mut queue = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let latest = queue.pop();
    queue.clear();
    latest
}

/// Installs the queueing sink for its lifetime, restores stderr on drop (also on panic).
pub struct WarningGuard;

impl WarningGuard {
    pub fn install() -> Self {
        codec::set_warning_sink(Some(push));
        Self
    }
}

impl Drop for WarningGuard {
    fn drop(&mut self) {
        codec::set_warning_sink(None);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Write into a dir whose fsync fails, returns the dir path for message matching.
    fn failing_fsync_write() -> String {
        let dir = tempfile::TempDir::new().unwrap();
        let mode = std::fs::Permissions::from_mode;
        std::fs::set_permissions(dir.path(), mode(0o300)).unwrap();
        let result = codec::atomic_write(&dir.path().join("x.cast"), |w| Ok(w.write_all(b"a")?));
        std::fs::set_permissions(dir.path(), mode(0o700)).unwrap();
        result.unwrap();
        dir.path().display().to_string()
    }

    #[test]
    fn guard_queues_warnings_and_restores_on_drop() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let guard = WarningGuard::install();
        let queued = failing_fsync_write();
        let seen = take_latest().unwrap_or_default();
        assert!(seen.contains(&queued), "{seen:?}");

        drop(guard);
        failing_fsync_write();
        assert_eq!(take_latest(), None);
    }
}
