//! Session playback functionality
//!
//! Handles playing back recorded sessions using asciinema.

use std::path::Path;
use std::process::Command;

use anyhow::Result;

/// Result of a playback operation
#[derive(Debug, Clone)]
pub enum PlaybackResult {
    /// Playback completed successfully
    Success(String),
    /// Playback was interrupted (e.g., user pressed q)
    Interrupted,
    /// Playback failed with an error
    Error(String),
}

impl PlaybackResult {
    /// Get a human-readable message for this result
    pub fn message(&self) -> String {
        match self {
            PlaybackResult::Success(name) => format!("Played: {}", name),
            PlaybackResult::Interrupted => "Playback interrupted".to_string(),
            PlaybackResult::Error(e) => format!("Failed to play: {}", e),
        }
    }
}

/// Play a session recording using asciinema.
///
/// This function assumes the terminal is in normal mode (not TUI mode).
/// The caller is responsible for suspending/resuming any TUI before/after calling this.
///
/// # Arguments
/// * `path` - Path to the .cast file to play
///
/// # Returns
/// A `PlaybackResult` indicating success, interruption, or error.
pub fn play_session(path: &Path) -> Result<PlaybackResult> {
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

/// Play a session recording with speed multiplier.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_result_success_message() {
        let result = PlaybackResult::Success("test.cast".to_string());
        assert_eq!(result.message(), "Played: test.cast");
    }

    #[test]
    fn playback_result_interrupted_message() {
        let result = PlaybackResult::Interrupted;
        assert_eq!(result.message(), "Playback interrupted");
    }

    #[test]
    fn playback_result_error_message() {
        let result = PlaybackResult::Error("not found".to_string());
        assert_eq!(result.message(), "Failed to play: not found");
    }

    #[test]
    fn playback_result_clone() {
        let result = PlaybackResult::Success("test.cast".to_string());
        let cloned = result.clone();
        assert_eq!(result.message(), cloned.message());
    }

    #[test]
    fn playback_result_debug() {
        let result = PlaybackResult::Interrupted;
        let debug = format!("{:?}", result);
        assert!(debug.contains("Interrupted"));
    }
}
