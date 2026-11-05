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

    /// Load from a path, default when the file is missing or malformed
    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
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
