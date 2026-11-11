//! `State::update_at` re-reads under the lock, so a stale loaded copy never wipes a field
//! that another writer stored meanwhile (the writer id).

use agr::config::state::State;

#[test]
fn clearing_the_auto_error_keeps_a_writer_id_stored_meanwhile() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.toml");
    State::store_auto_error(&path, "offline".into()).unwrap();
    let stale = State::load_from(&path);
    assert!(stale.writer_id.is_none());
    State::keep_writer_id_at(&path, "writer-1").unwrap();
    let taken = State::update_at(&path, |s| s.last_auto_push_error.take()).unwrap();
    assert_eq!(taken.as_deref(), Some("offline"));
    let got = State::load_from(&path);
    assert_eq!(got.writer_id.as_deref(), Some("writer-1"));
    assert_eq!(got.last_auto_push_error, None);
}

#[test]
fn the_privacy_ack_keeps_a_writer_id_stored_meanwhile() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.toml");
    State::keep_writer_id_at(&path, "writer-2").unwrap();
    State::update_at(&path, |s| s.sync_privacy_ack = true).unwrap();
    let got = State::load_from(&path);
    assert_eq!(got.writer_id.as_deref(), Some("writer-2"));
    assert!(got.sync_privacy_ack);
}
