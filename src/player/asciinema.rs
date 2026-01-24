//! Legacy asciinema CLI wrapper
//!
//! Provides playback by shelling out to the asciinema CLI.
//! Note: This may have display issues if the recording was made at a different
//! terminal size. Use the native player for better size handling.

use std::path::Path;
use std::process::Command;

use anyhow::Result;

use super::PlaybackResult;

/// Play a session recording using asciinema directly.
///
/// Use this if you want the original asciinema experience with potential
/// size mismatch issues.
pub fn play_session_asciinema(path: &Path) -> Result<PlaybackResult> {
    let status = Command::new("asciinema").arg("play").arg(path).status();

    let result = match status {
        Ok(exit) if exit.success() => {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            PlaybackResult::Success(name)
        }
        Ok(_) => PlaybackResult::Interrupted,
        Err(e) => PlaybackResult::Error(e.to_string()),
    };

    Ok(result)
}

/// Play a session recording with speed multiplier using asciinema.
///
/// # Arguments
/// * `path` - Path to the .cast file to play
/// * `speed` - Speed multiplier (e.g., 2.0 for 2x speed)
pub fn play_session_with_speed(path: &Path, speed: f64) -> Result<PlaybackResult> {
    let status = Command::new("asciinema")
        .arg("play")
        .arg("--speed")
        .arg(speed.to_string())
        .arg(path)
        .status();

    let result = match status {
        Ok(exit) if exit.success() => {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            PlaybackResult::Success(name)
        }
        Ok(_) => PlaybackResult::Interrupted,
        Err(e) => PlaybackResult::Error(e.to_string()),
    };

    Ok(result)
}
