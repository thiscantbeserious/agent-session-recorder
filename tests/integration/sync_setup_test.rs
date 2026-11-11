//! Key resolution, backend factory and error hints of `sync::setup`.

use agr::config::SyncBackendKind;
use agr::sync::key::{self, Imported, KeyError};
use agr::sync::setup::{
    explain, export_key, import_key, needs_privacy_ack, open_store, resolve_key, ImportError,
    KeyNeed, KeyState, SetupError,
};
use agr::sync::testing::{key_bytes, MemoryStore, PackStore};
use agr::sync::{Key, StoreError};
use std::path::PathBuf;

#[test]
fn privacy_ack_is_due_once_for_creating_pushes() {
    assert!(needs_privacy_ack(false, 1));
    assert!(!needs_privacy_ack(true, 1));
    assert!(!needs_privacy_ack(false, 0));
}

#[test]
fn backend_factory_follows_config_and_test_switch() {
    assert!(open_store(SyncBackendKind::Off, None, None).is_none());
    assert!(open_store(SyncBackendKind::Off, Some("/x".into()), None).is_none());
    let gist = open_store(
        SyncBackendKind::Gist,
        None,
        Some(std::sync::Arc::new(|_| {})),
    )
    .unwrap();
    assert_eq!(gist.name(), "gist");
    let fake = open_store(SyncBackendKind::Gist, Some("/x".into()), None).unwrap();
    assert_eq!(fake.name(), "directory");
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
    let (_d3, third) = key_path();
    key::save_new(&third, &Key::generate()).unwrap();
    let different = export_key(&third).unwrap().unwrap();
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
    let state = resolve_key(&path, &MemoryStore::new(), KeyNeed::Existing).unwrap();
    assert!(matches!(&state, KeyState::Have(k) if key_bytes(k) == key_bytes(&key)));
}

#[test]
fn missing_key_aborts_when_it_must_exist() {
    let (_dir, path) = key_path();
    let got = resolve_key(&path, &MemoryStore::new(), KeyNeed::Existing);
    assert!(matches!(got, Err(SetupError::NoKey)));
}

#[test]
fn missing_key_on_an_empty_remote_is_fresh_and_not_stored() {
    let (_dir, path) = key_path();
    let got = resolve_key(&path, &MemoryStore::new(), KeyNeed::MayCreate).unwrap();
    assert!(matches!(got, KeyState::Fresh(_)));
    assert!(!path.exists());
}

#[test]
fn missing_key_with_remote_sessions_never_creates_one() {
    let (_dir, path) = key_path();
    let store = MemoryStore::new();
    store.create("tag", "index").unwrap();
    let got = resolve_key(&path, &store, KeyNeed::MayCreate);
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
        let got = resolve_key(&path, &MemoryStore::new(), KeyNeed::Existing);
        assert!(matches!(got, Err(SetupError::Key(KeyError::Insecure(_)))));
    }
}

#[test]
fn explain_gives_message_and_hint() {
    let (msg, hint) = explain(&StoreError::NotInstalled("gh"));
    assert!(msg.starts_with("gh not found"), "{msg}");
    assert!(hint.unwrap().contains("https://cli.github.com"));
    let (_, hint) = explain(&StoreError::NotLoggedIn);
    assert!(hint.unwrap().contains("gh auth login"));
    let (_, hint) = explain(&StoreError::MissingScope);
    assert!(hint.unwrap().contains("gh auth refresh -s gist"));
    let (msg, hint) = explain(&StoreError::RateLimited);
    assert!(!msg.contains('\n') && msg.contains("rate limit"), "{msg}");
    assert_eq!(hint.as_deref(), Some("Please run again later"));
    assert_eq!(explain(&StoreError::Other("x".into())), ("x".into(), None));
}

#[test]
fn save_new_refuses_existing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync.key");
    key::save_new(&path, &Key::generate()).unwrap();
    assert_eq!(
        key::save_new(&path, &Key::generate()),
        Err(KeyError::Exists)
    );
}

#[cfg(unix)]
#[test]
fn save_new_uses_mode_0600() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync.key");
    key::save_new(&path, &Key::generate()).unwrap();
    assert!(path.exists());
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}
