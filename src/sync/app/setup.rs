//! Preconditions shared by the sync commands, key resolution and backend error hints.

use std::path::{Path, PathBuf};

use thiserror::Error;

use super::key::{self, Imported, KeyError};
use crate::config::SyncBackendKind;
use crate::sync::backend::file::FileBackend;
use crate::sync::backend::gist::{GistBackend, ProgressSink};
use crate::sync::backend::{BackendError, SyncBackend};
use crate::sync::crypto::{export_phrase, import_phrase, Key, PhraseError};
use crate::Config;

/// Whether a missing key may be created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyNeed {
    /// Pull and delete, a missing key aborts.
    Existing,
    /// Push and status, a key is made only when the remote has no agr sessions.
    MayCreate,
}

/// `Fresh` lives in memory only, the caller stores it once the user confirmed.
pub enum KeyState {
    Have(Key),
    Fresh(Key),
}

impl KeyState {
    pub fn key(&self) -> &Key {
        match self {
            KeyState::Have(key) | KeyState::Fresh(key) => key,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SetupError {
    #[error("no sync key on this machine")]
    NoKey,
    #[error("the remote already holds agr sessions but this machine has no sync key")]
    KeyOnRemote,
    #[error(transparent)]
    Key(#[from] KeyError),
    #[error(transparent)]
    Backend(#[from] BackendError),
}

/// Hint line for the import step, shown with `NoKey` and `KeyOnRemote`.
pub const IMPORT_HINT: &str =
    "Run `agr sync key import` with the recovery phrase from `agr sync key export` on your other machine.";

/// `~/.config/agr/sync.key`
pub fn default_key_path() -> anyhow::Result<PathBuf> {
    Ok(Config::config_dir()?.join("sync.key"))
}

/// Load the key, or decide what to do when there is none (ADR "Key file").
pub fn resolve_key(
    path: &Path,
    backend: &dyn SyncBackend,
    need: KeyNeed,
) -> Result<KeyState, SetupError> {
    if let Some(key) = key::load(path)? {
        return Ok(KeyState::Have(key));
    }
    if need == KeyNeed::Existing {
        return Err(SetupError::NoKey);
    }
    if !backend.list()?.is_empty() {
        return Err(SetupError::KeyOnRemote);
    }
    Ok(KeyState::Fresh(Key::generate()))
}

/// One error line and an optional hint line per backend failure.
pub fn explain(error: &BackendError) -> (String, Option<String>) {
    match error {
        BackendError::NotInstalled("gh") => (
            "gh not found, the gist backend needs gh and git".into(),
            Some("Install gh from https://cli.github.com, then run `gh auth login`".into()),
        ),
        BackendError::NotInstalled(tool) => (
            format!("{tool} not found, the gist backend needs gh and git"),
            Some(format!(
                "Install {tool} and gh (https://cli.github.com), then run `gh auth login`"
            )),
        ),
        BackendError::NotLoggedIn => (
            "gh not logged in".into(),
            Some("Run `gh auth login`".into()),
        ),
        BackendError::MissingScope => (
            "the gh token has no gist scope".into(),
            Some("Run `gh auth refresh -s gist`".into()),
        ),
        BackendError::RateLimited => (
            "GitHub rate limit reached, sync stopped".into(),
            Some("Please run again later".into()),
        ),
        other => (other.to_string(), None),
    }
}

/// The configured backend, `None` when `backend = "off"`. `test_dir` (the binary reads it from
/// `AGR_SYNC_TEST_BACKEND_DIR`) swaps the gist backend for a directory fake, tests only.
/// `progress` gets the percent of a running git transfer, the fake never reports one.
pub fn open_backend(
    kind: SyncBackendKind,
    test_dir: Option<PathBuf>,
    progress: Option<ProgressSink>,
) -> Option<Box<dyn SyncBackend>> {
    if kind == SyncBackendKind::Off {
        return None;
    }
    Some(match test_dir {
        Some(dir) => Box::new(FileBackend::new(dir)),
        None => Box::new(match progress {
            Some(sink) => GistBackend::new().with_progress(sink),
            None => GistBackend::new(),
        }),
    })
}

/// The privacy notice is due before a push that would create the first remote entry
/// on this machine, `planned_new` counts planned pushes without a remote copy.
pub fn needs_privacy_ack(acknowledged: bool, planned_new: usize) -> bool {
    !acknowledged && planned_new > 0
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ImportError {
    #[error(transparent)]
    Phrase(#[from] PhraseError),
    #[error(transparent)]
    Key(#[from] KeyError),
}

/// Parse a phrase and store it as the key file.
pub fn import_key(path: &Path, phrase: &str, force: bool) -> Result<Imported, ImportError> {
    let key = import_phrase(phrase)?;
    Ok(key::import(path, &key, force)?)
}

/// The recovery phrase of the stored key, `None` when there is no key.
pub fn export_key(path: &Path) -> Result<Option<String>, KeyError> {
    Ok(key::load(path)?.map(|key| export_phrase(&key)))
}
