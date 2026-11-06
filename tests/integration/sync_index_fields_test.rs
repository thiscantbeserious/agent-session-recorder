//! Index fields, remote id fallback and duplicate pick for sync rows

use agr::files::codec;
use agr::sync::backend::RemoteEntry;
use agr::sync::crypto::{seal_meta, Key, Meta};
use agr::sync::index::{self, Index, IndexEntry};
use agr::sync::plan::{decide, resolve, Command, Decision, Remote};
use agr::sync::scan::{scan, Row};
use agr::{Config, StorageManager};
use tempfile::TempDir;

const PATH: &str = "claude/a.cast.gz";

fn storage(temp: &TempDir) -> StorageManager {
    let mut config = Config::default();
    config.storage.directory = temp.path().to_string_lossy().to_string();
    StorageManager::new(config)
}

fn local_file(temp: &TempDir, text: &str) {
    let dir = temp.path().join("claude");
    std::fs::create_dir_all(&dir).unwrap();
    codec::atomic_write(
        &dir.join("a.cast.gz"),
        |w| Ok(w.write_all(text.as_bytes())?),
    )
    .unwrap();
}

fn token(key: &Key, sha: &str, gen: u64) -> String {
    let meta = Meta {
        v: 2,
        path: PATH.into(),
        sha256: sha.into(),
        size: 1,
        gen,
    };
    seal_meta(key, &meta)
}

fn remote(id: &str, token: &str, created_at: &str) -> RemoteEntry {
    RemoteEntry {
        id: id.into(),
        meta_token: token.into(),
        created_at: created_at.into(),
    }
}

fn index_with(entry: IndexEntry) -> Index {
    let mut index = Index::default();
    index.entries.insert(PATH.into(), entry);
    index
}

fn rows(temp: &TempDir, key: &Key, remote: &[RemoteEntry], index: &Index) -> Vec<Row> {
    scan(&storage(temp), key, remote, index).unwrap()
}

fn row_of(rows: Vec<Row>) -> Row {
    rows.into_iter().find(|r| r.path == PATH).unwrap()
}

#[test]
fn old_index_without_new_fields_still_loads() {
    let temp = TempDir::new().unwrap();
    let old = r#"{"version":1,"entries":{"claude/a.cast.gz":
        {"sha256":"h","remote_id":"id1","synced_at":"2026-11-06T10:00:00+00:00"}}}"#;
    std::fs::write(index::path_for(temp.path()), old).unwrap();
    let (index, warning) = index::load(temp.path());
    assert_eq!(warning, None);
    let entry = &index.entries[PATH];
    assert_eq!(entry.remote_id, "id1");
    assert_eq!(
        (entry.gen, entry.token.clone(), entry.pending),
        (0, None, false)
    );
}

#[test]
fn missing_listing_falls_back_to_the_index_id_and_token() {
    let (temp, key) = (TempDir::new().unwrap(), Key::generate());
    local_file(&temp, "x");
    let entry = IndexEntry {
        remote_id: "gist-9".into(),
        token: Some("tok".into()),
        gen: 3,
        ..IndexEntry::default()
    };
    let row = row_of(rows(&temp, &key, &[], &index_with(entry)));
    assert_eq!(row.remote_id.as_deref(), Some("gist-9"));
    assert_eq!(row.token.as_deref(), Some("tok"));
    assert_eq!(row.gen, 3);
    assert!(!row.pending);
}

#[test]
fn pending_entry_resumes_its_id_without_a_token() {
    let (temp, key) = (TempDir::new().unwrap(), Key::generate());
    local_file(&temp, "x");
    let entry = IndexEntry {
        remote_id: "gist-9".into(),
        token: Some("stale".into()),
        pending: true,
        ..IndexEntry::default()
    };
    let row = row_of(rows(&temp, &key, &[], &index_with(entry)));
    assert_eq!(row.remote_id.as_deref(), Some("gist-9"));
    assert_eq!(row.token, None);
    assert!(row.pending);
    assert_eq!(decide(&row.input), Decision::Push);
}

#[test]
fn listing_wins_over_the_index_id() {
    let (temp, key) = (TempDir::new().unwrap(), Key::generate());
    let listed = remote("listed", &token(&key, "h", 2), "0000000001");
    let entry = IndexEntry {
        remote_id: "old".into(),
        ..IndexEntry::default()
    };
    let row = row_of(rows(&temp, &key, &[listed], &index_with(entry)));
    assert_eq!(row.remote_id.as_deref(), Some("listed"));
    assert_eq!(row.gen, 2);
}

#[test]
fn oldest_created_at_wins_with_id_as_tie_break_on_any_order() {
    let (temp, key) = (TempDir::new().unwrap(), Key::generate());
    let t = token(&key, "same", 1);
    let all = [
        remote("b", &t, "0000000005"),
        remote("z", &t, "0000000002"),
        remote("a", &t, "0000000002"),
    ];
    let mut reversed = all.clone();
    reversed.reverse();
    for list in [&all, &reversed] {
        let row = row_of(rows(&temp, &key, list, &Index::default()));
        assert_eq!(row.remote_id.as_deref(), Some("a"));
        let mut others: Vec<_> = row.duplicates.iter().map(|d| d.id.clone()).collect();
        others.sort();
        assert_eq!(others, ["b", "z"]);
    }
}

#[test]
fn same_hash_duplicates_stay_a_note() {
    let (temp, key) = (TempDir::new().unwrap(), Key::generate());
    let t = token(&key, "same", 1);
    let list = [remote("a", &t, "1"), remote("b", &t, "2")];
    let row = row_of(rows(&temp, &key, &list, &Index::default()));
    assert_eq!(row.input.remote, Remote::Hash("same".into()));
    assert_eq!(row.duplicates.len(), 1);
    assert!(row.duplicates[0].same_hash);
}

#[test]
fn different_hash_duplicates_are_never_acted_on_not_even_with_overwrite() {
    let (temp, key) = (TempDir::new().unwrap(), Key::generate());
    local_file(&temp, "x");
    let list = [
        remote("a", &token(&key, "one", 1), "1"),
        remote("b", &token(&key, "two", 1), "2"),
    ];
    let row = row_of(rows(&temp, &key, &list, &Index::default()));
    let decision = decide(&row.input);
    assert_eq!(decision, Decision::Duplicate);
    for command in [Command::Push, Command::Pull] {
        assert_eq!(resolve(decision, command, true), Decision::Duplicate);
    }
    assert_eq!(row.remote_id.as_deref(), Some("a"));
}

#[test]
fn older_description_gen_marks_the_row_for_repair() {
    let (temp, key) = (TempDir::new().unwrap(), Key::generate());
    let listed = remote("a", &token(&key, "h", 2), "1");
    let entry = |gen| IndexEntry {
        remote_id: "a".into(),
        gen,
        ..IndexEntry::default()
    };
    let behind = row_of(rows(
        &temp,
        &key,
        std::slice::from_ref(&listed),
        &index_with(entry(3)),
    ));
    assert!(behind.repair_description);
    let level = row_of(rows(&temp, &key, &[listed], &index_with(entry(2))));
    assert!(!level.repair_description);
}
