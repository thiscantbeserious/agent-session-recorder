//! Process lifecycle guard for long-running child processes.
//!
//! Detects termination conditions and ensures clean shutdown:
//! - SIGINT (Ctrl+C) via ctrlc handler
//! - SIGHUP (terminal hangup) via signal_hook
//! - Parent process death (terminal force-closed, reparented to init/subreaper)
//!
//! The orphan detection uses parent PID comparison rather than checking for PID 1,
//! which works correctly on Linux with systemd subreapers and on macOS with launchd.

use std::process::{Child, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

/// Guards a child process against becoming an orphan.
///
/// Create before spawning, register signals, then use `wait_or_kill` instead of `.status()`.
pub struct ProcessGuard {
    interrupted: Arc<AtomicBool>,
    #[cfg(unix)]
    initial_ppid: u32,
}

impl Default for ProcessGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessGuard {
    /// Snapshot the current parent PID for later orphan detection.
    pub fn new() -> Self {
        Self {
            interrupted: Arc::new(AtomicBool::new(false)),
            #[cfg(unix)]
            initial_ppid: unsafe { libc::getppid() as u32 },
        }
    }

    /// Register SIGINT (Ctrl+C) and SIGHUP (terminal hangup) handlers.
    ///
    /// Both set the same `interrupted` flag checked by `wait_or_kill`.
    /// Safe to call multiple times — duplicate registrations are ignored.
    pub fn register_signal_handlers(&self) {
        let flag = self.interrupted.clone();
        ctrlc::set_handler(move || {
            flag.store(true, Ordering::SeqCst);
        })
        .ok(); // Ignore if handler already set

        #[cfg(unix)]
        {
            use signal_hook::flag::register;
            let _ = register(libc::SIGHUP, self.interrupted.clone());
        }
    }

    /// Whether the interrupted flag was set (by SIGINT or SIGHUP).
    pub fn is_interrupted(&self) -> bool {
        self.interrupted.load(Ordering::SeqCst)
    }

    /// Wait for a child process, killing it on signal or orphan detection.
    ///
    /// Polls every 100ms. Terminates the child when any of:
    /// - Signal handler set the interrupted flag (SIGINT/SIGHUP)
    /// - Parent process died (detected via ppid change)
    pub fn wait_or_kill(&self, child: &mut Child) -> Result<ExitStatus> {
        loop {
            match child.try_wait()? {
                Some(status) => return Ok(status),
                None => {
                    if self.should_terminate() {
                        let _ = child.kill();
                        return child.wait().map_err(Into::into);
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }

    /// Check all termination conditions: signal flag or parent death.
    fn should_terminate(&self) -> bool {
        self.interrupted.load(Ordering::SeqCst) || Self::is_orphaned(self)
    }

    /// Detect parent death by comparing current ppid against the initial snapshot.
    ///
    /// Works on both macOS (reparented to launchd/PID 1) and Linux (reparented to
    /// a systemd subreaper or PID 1). Any ppid change means the parent died.
    #[cfg(unix)]
    fn is_orphaned(&self) -> bool {
        let current_ppid = unsafe { libc::getppid() as u32 };
        current_ppid != self.initial_ppid
    }

    #[cfg(not(unix))]
    fn is_orphaned(&self) -> bool {
        false
    }
}
