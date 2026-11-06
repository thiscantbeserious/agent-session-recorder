//! Post-process a finished recording: silence-optimize, then compress per `storage.compression`.

use std::path::{Path, PathBuf};

use crate::asciicast::remove_silence;
use crate::config::{Compression, Config};
use crate::storage::compress::{compress_file, Outcome};

/// Returns the path of the finished recording, `.cast.gz` when it was compressed.
/// On any failure the plain file stays and its path is returned with a warning.
pub fn finalize_recording(path: &Path, config: &Config) -> PathBuf {
    if config.storage.compression != Compression::Gzip {
        return path.to_path_buf();
    }
    match compress_file(path) {
        Ok(Outcome::Compressed { warning, .. }) => {
            if let Some(w) = warning {
                eprintln!("  \u{26a0} {w}");
            }
            crate::files::codec::gzip_sibling(path)
        }
        Ok(Outcome::Skipped(why)) => warn_keep(path, &why),
        Err(e) => warn_keep(path, &format!("{e:#}")),
    }
}

fn warn_keep(path: &Path, why: &str) -> PathBuf {
    eprintln!("  \u{26a0} Compression skipped, kept plain file: {why}");
    path.to_path_buf()
}

/// Clamps idle time in the plain recording without a backup, prints nothing.
/// Returns None for a missing or empty file, else the warning line when optimizing failed, the file then stays as recorded.
pub fn optimize_recording(path: &Path, config: &Config) -> Option<String> {
    if !config.recording.auto_optimize || !path.metadata().is_ok_and(|m| m.len() > 0) {
        return None;
    }
    remove_silence(path)
        .err()
        .map(|e| format!("Auto-optimize failed, kept original: {e:#}"))
}
