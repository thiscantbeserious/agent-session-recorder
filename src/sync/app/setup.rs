//! Preconditions shared by the sync commands, key resolution and backend error hints.

use std::path::{Path, PathBuf};

use thiserror::Error;

use super::key::{self, Imported, KeyError};
use crate::config::SyncBackendKind;
use crate::sync::backend::file::FileBackend;
use crate::sync::backend::gist::GistBackend;
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
            "gh not found".into(),
            Some("Install it from https://cli.github.com".into()),
        ),
        BackendError::NotInstalled(tool) => (format!("{tool} not found"), None),
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
pub fn open_backend(
    kind: SyncBackendKind,
    test_dir: Option<PathBuf>,
) -> Option<Box<dyn SyncBackend>> {
    if kind == SyncBackendKind::Off {
        return None;
    }
    Some(match test_dir {
        Some(dir) => Box::new(FileBackend::new(dir)),
        None => Box::new(GistBackend::new()),
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

#[cfg(test)]
mod tests {
    use super::key;
    use super::{
        explain, export_key, import_key, needs_privacy_ack, open_backend, resolve_key,
        BackendError, ImportError, Imported, Key, KeyError, KeyNeed, KeyState, PathBuf, SetupError,
        SyncBackendKind,
    };
    use crate::sync::backend::memory::MemoryBackend;

    #[test]
    fn privacy_ack_is_due_once_for_creating_pushes() {
        assert!(needs_privacy_ack(false, 1));
        assert!(!needs_privacy_ack(true, 1));
        assert!(!needs_privacy_ack(false, 0));
    }

    #[test]
    fn backend_factory_follows_config_and_test_switch() {
        assert!(open_backend(SyncBackendKind::Off, None).is_none());
        assert!(open_backend(SyncBackendKind::Off, Some("/x".into())).is_none());
        let gist = open_backend(SyncBackendKind::Gist, None).unwrap();
        assert_eq!(gist.name(), "gist");
        let fake = open_backend(SyncBackendKind::Gist, Some("/x".into())).unwrap();
        assert_eq!(fake.name(), "file");
    }

    #[test]
    fn export_then_import_round_trips_and_guards() {
        let (_dir, path) = key_path();
        assert_eq!(export_key(&path), Ok(None));
        let key = Key::generate();
        key::save_new(&path, &key).unwrap();
        let phrase = export_key(&path).unwrap().unwrap();
        assert!(phrase.starts_with("agr1-"));
        assert_eq!(import_key(&path, &phrase, false), Ok(Imported::Same));
        let (_d2, other) = key_path();
        assert_eq!(import_key(&other, &phrase, false), Ok(Imported::Created));
        let different = crate::sync::crypto::export_phrase(&Key::generate());
        assert_eq!(
            import_key(&path, &different, false),
            Err(ImportError::Key(KeyError::Differs))
        );
        assert_eq!(import_key(&path, &different, true), Ok(Imported::Replaced));
        assert!(matches!(
            import_key(&path, "agr1-AAAA", true),
            Err(ImportError::Phrase(_))
        ));
    }

    fn key_path() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.key");
        (dir, path)
    }

    #[test]
    fn existing_key_is_used() {
        let (_dir, path) = key_path();
        let key = Key::generate();
        key::save_new(&path, &key).unwrap();
        let state = resolve_key(&path, &MemoryBackend::new(), KeyNeed::Existing).unwrap();
        assert!(matches!(&state, KeyState::Have(k) if k.as_bytes() == key.as_bytes()));
    }

    #[test]
    fn missing_key_aborts_when_it_must_exist() {
        let (_dir, path) = key_path();
        let got = resolve_key(&path, &MemoryBackend::new(), KeyNeed::Existing);
        assert!(matches!(got, Err(SetupError::NoKey)));
    }

    #[test]
    fn missing_key_on_an_empty_remote_is_fresh_and_not_stored() {
        let (_dir, path) = key_path();
        let got = resolve_key(&path, &MemoryBackend::new(), KeyNeed::MayCreate).unwrap();
        assert!(matches!(got, KeyState::Fresh(_)));
        assert!(!path.exists());
    }

    #[test]
    fn missing_key_with_remote_sessions_never_creates_one() {
        let (_dir, path) = key_path();
        let backend = MemoryBackend::new();
        backend.put(None, "token", b"blob");
        let got = resolve_key(&path, &backend, KeyNeed::MayCreate);
        assert!(matches!(got, Err(SetupError::KeyOnRemote)));
    }

    #[test]
    fn loose_key_permissions_are_an_error() {
        let (_dir, path) = key_path();
        std::fs::write(&path, "agr-sync-key v1 AAAA\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            let got = resolve_key(&path, &MemoryBackend::new(), KeyNeed::Existing);
            assert!(matches!(got, Err(SetupError::Key(KeyError::Insecure(_)))));
        }
    }

    #[test]
    fn explain_gives_message_and_hint() {
        let (msg, hint) = explain(&BackendError::NotInstalled("gh"));
        assert_eq!(msg, "gh not found");
        assert!(hint.unwrap().contains("https://cli.github.com"));
        let (_, hint) = explain(&BackendError::NotLoggedIn);
        assert!(hint.unwrap().contains("gh auth login"));
        let (_, hint) = explain(&BackendError::MissingScope);
        assert!(hint.unwrap().contains("gh auth refresh -s gist"));
        let (msg, hint) = explain(&BackendError::RateLimited);
        assert!(!msg.contains('\n') && msg.contains("rate limit"), "{msg}");
        assert_eq!(hint.as_deref(), Some("Please run again later"));
        assert_eq!(
            explain(&BackendError::Other("x".into())),
            ("x".into(), None)
        );
    }
}
