//! Auto push hook, the local checks run here, the upload runs in a detached child.

use std::path::Path;

use crate::config::Config;
use crate::sync::{auto, runner};

/// Print at most one line and start the push child. Never fails the recording.
pub fn after_finalize(config: &Config, path: &Path) {
    let (lines, spawn) = auto::after_recording(config, path);
    if let Some(line) = lines.first() {
        eprintln!("  \u{26a0} {line}");
    }
    let (Some(args), Ok(exe)) = (spawn, std::env::current_exe()) else {
        return;
    };
    if let Err(e) = runner::spawn_detached(&exe, &args) {
        eprintln!("  \u{26a0} Auto push not started: {e}");
    }
}
