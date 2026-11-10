//! Machine-local state (~/.config/agr/state.toml), never synced

use crate::files::codec;
use anyhow::{Context, Result};

use serde::{Deserialize, Serialize};
use std::path::Path;

use super::Config;

const FILE: &str = "state.toml";

/// What agr remembers between runs, separate from user-edited config.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    /// Version of agr that last showed the compression offer
    #[serde(default)]
    pub compress_offer_version: Option<String>,
    /// Set once the user confirmed the sync privacy notice
    #[serde(default)]
    pub sync_privacy_ack: bool,
    /// Last failed automatic push, shown by `agr sync status`
    #[serde(default)]
    pub last_auto_push_error: Option<String>,
    /// Random id of this machine for sync packs, created once
    #[serde(default)]
    pub writer_id: Option<String>,
}

impl State {
    /// Load from `~/.config/agr/state.toml`, default when missing or broken
    pub fn load() -> Self {
        Config::config_dir().map_or_else(|_| Self::default(), |d| Self::load_from(&d.join(FILE)))
    }

    /// Save to `~/.config/agr/state.toml`
    pub fn save(&self) -> Result<()> {
        self.save_to(&Config::config_dir()?.join(FILE))
    }

    /// Store the last auto push error in the file at `path`.
    pub fn store_auto_error(path: &Path, line: String) -> Result<()> {
        let mut state = Self::load_from(path);
        state.last_auto_push_error = Some(line);
        state.save_to(path)
    }

    /// Like `store_auto_error` for `~/.config/agr/state.toml`, re-reads the file so a writer
    /// that ran meanwhile is not overwritten.
    pub fn store_auto_error_default(line: String) -> Result<()> {
        Self::store_auto_error(&Config::config_dir()?.join(FILE), line)
    }

    /// The writer id stored in the file at `path`, created and saved on first use.
    pub fn writer_id(path: &Path) -> Result<String> {
        let mut state = Self::load_from(path);
        if let Some(id) = &state.writer_id {
            return Ok(id.clone());
        }
        let id = crate::sync::new_writer_id();
        state.writer_id = Some(id.clone());
        state.save_to(path)?;
        Ok(id)
    }

    /// Load from a path, default when the file is missing or malformed (warns on stderr).
    pub fn load_from(path: &Path) -> Self {
        let (state, warning) = Self::load_checked(path);
        if let Some(w) = warning {
            eprintln!("Warning: {w}");
        }
        state
    }

    /// Like `load_from`, but returns the warning for an unparseable file instead of printing.
    pub fn load_checked(path: &Path) -> (Self, Option<String>) {
        let Ok(text) = std::fs::read_to_string(path) else {
            return (Self::default(), None);
        };
        match toml::from_str(&text) {
            Ok(state) => (state, None),
            Err(e) => (
                Self::default(),
                Some(format!("ignoring unreadable {}: {e}", path.display())),
            ),
        }
    }

    /// Save to a path atomically (temp file plus rename), creating parent dirs
    pub fn save_to(&self, path: &Path) -> Result<()> {
        let contents = toml::to_string_pretty(self).context("Failed to serialize state")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create state directory: {:?}", parent))?;
        }
        codec::atomic_write(path, |w| Ok(w.write_all(contents.as_bytes())?))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn store_auto_error_keeps_fields_written_by_others() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(super::FILE);
        let other = State {
            sync_privacy_ack: true,
            compress_offer_version: Some("9".into()),
            ..State::default()
        };
        other.save_to(&path).unwrap();
        State::store_auto_error(&path, "offline".into()).unwrap();
        let got = State::load_from(&path);
        assert_eq!(got.compress_offer_version.as_deref(), Some("9"));
        assert!(got.sync_privacy_ack);
        assert_eq!(got.last_auto_push_error.as_deref(), Some("offline"));
    }

    use super::*;

    #[test]
    fn sync_fields_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        let state = State {
            compress_offer_version: None,
            sync_privacy_ack: true,
            last_auto_push_error: Some("offline".into()),
            writer_id: None,
        };
        state.save_to(&path).unwrap();
        assert_eq!(State::load_checked(&path), (state, None));
    }

    #[test]
    fn old_state_without_sync_fields_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        std::fs::write(&path, "compress_offer_version = \"0.9.0\"\n").unwrap();
        let (state, warning) = State::load_checked(&path);
        assert_eq!(warning, None);
        assert_eq!(state.compress_offer_version.as_deref(), Some("0.9.0"));
        assert!(!state.sync_privacy_ack);
        assert_eq!(state.last_auto_push_error, None);
    }
}
