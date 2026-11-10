//! The cached clone of the own open pack (branch C).

use super::append_tests::{append, Rig};
use super::cache::pack_dir;
use super::{PackStore, StoreError};

#[test]
fn cached_clone_is_reused_across_appends() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &[]);
    let store = rig.store();
    append(&store, &id, (0, "i0"), (b"abc", "i1")).unwrap();
    append(&store, &id, (3, "i1"), (b"def", "i2")).unwrap();
    assert_eq!(rig.count("clone"), 1);
    assert_eq!(rig.count("fetch"), 1);
    assert_eq!(rig.count("merge --ff-only FETCH_HEAD"), 1);
}

#[test]
fn cached_clone_removed_on_seal() {
    let rig = Rig::new();
    let (old, next) = (rig.remote.seed("i0", &[]), rig.remote.seed("j0", &[]));
    let store = rig.store();
    append(&store, &old, (0, "i0"), (b"abc", "i1")).unwrap();
    assert!(rig.clone_dir(&old).exists());
    // The pack is sealed, the pack layer says so and the old clone goes, the next one stays.
    store.sealed(&old);
    append(&store, &next, (0, "j0"), (b"abc", "j1")).unwrap();
    assert!(!rig.clone_dir(&old).exists());
    assert!(rig.clone_dir(&next).exists());
}

#[test]
fn missing_cached_clone_is_cloned_once() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &[]);
    let store = rig.store();
    append(&store, &id, (0, "i0"), (b"abc", "i1")).unwrap();
    std::fs::remove_dir_all(rig.clone_dir(&id)).unwrap();
    rig.remote.clear_calls();
    append(&store, &id, (3, "i1"), (b"def", "i2")).unwrap();
    assert_eq!(rig.count("clone"), 1);
    std::fs::remove_dir_all(rig.clone_dir(&id)).unwrap();
    rig.remote.fail("clone");
    rig.remote.clear_calls();
    let res = append(&store, &id, (6, "i2"), (b"ghi", "i3"));
    assert_eq!(rig.count("clone"), 1, "no loop");
    assert!(matches!(res, Err(StoreError::Other(_))), "{res:?}");
}

#[test]
fn cached_clone_with_other_index_is_rejected_and_recloned() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &[]);
    let store = rig.store();
    append(&store, &id, (0, "i0"), (b"abc", "i1")).unwrap();
    rig.remote.commit_elsewhere(&id, |f| {
        f.insert("index".into(), b"x9".to_vec());
    });
    let res = append(&store, &id, (3, "i1"), (b"def", "i2"));
    assert!(matches!(res, Err(StoreError::Moved(_))), "{res:?}");
    assert!(!rig.clone_dir(&id).exists());
    rig.remote.clear_calls();
    append(&store, &id, (3, "x9"), (b"def", "i2")).unwrap();
    assert_eq!(rig.count("clone"), 1);
}

#[test]
fn cached_clone_with_stale_len_is_rejected_and_recloned() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &[]);
    let store = rig.store();
    append(&store, &id, (0, "i0"), (b"abc", "i1")).unwrap();
    rig.remote.commit_elsewhere(&id, |f| {
        f.insert("data.001".into(), b"zz".to_vec());
    });
    let res = append(&store, &id, (3, "i1"), (b"def", "i2"));
    assert!(matches!(res, Err(StoreError::Moved(_))), "{res:?}");
    assert!(!rig.clone_dir(&id).exists());
}

#[test]
fn cached_clone_dropped_on_moved() {
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &[]);
    rig.remote.race_on_push(|f| {
        f.insert("index".into(), b"r1".to_vec());
    });
    let res = append(&rig.store(), &id, (0, "i0"), (b"abc", "i1"));
    assert!(matches!(res, Err(StoreError::Moved(_))), "{res:?}");
    assert!(!rig.clone_dir(&id).exists());
}

#[test]
fn cache_path_is_inside_user_cache_dir() {
    let root = std::path::Path::new("cache");
    let path = pack_dir(root, "abc123").unwrap();
    assert_eq!(path, root.join("agr/packs").join("abc123"));
}

#[test]
fn a_gist_id_that_is_not_hex_never_becomes_a_path() {
    let rig = Rig::new();
    let res = append(&rig.store(), "../evil", (0, "i0"), (b"abc", "i1"));
    assert!(matches!(res, Err(StoreError::Other(_))), "{res:?}");
    assert!(rig.remote.calls().is_empty());
    assert!(pack_dir(rig.root.path(), "").is_err());
}

#[cfg(unix)]
#[test]
fn cache_dir_is_0700_on_unix() {
    use std::os::unix::fs::PermissionsExt;
    let rig = Rig::new();
    let id = rig.remote.seed("i0", &[]);
    append(&rig.store(), &id, (0, "i0"), (b"abc", "i1")).unwrap();
    let packs = rig.clone_dir(&id).parent().unwrap().to_path_buf();
    for dir in [packs.parent().unwrap(), &packs] {
        let mode = std::fs::metadata(dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700, "{dir:?}");
    }
}

#[test]
fn with_cache_dir_moves_every_cache_file() {
    // A run with a temp cache dir must not read or write the user's real cache.
    let root = std::path::PathBuf::from("tmp-root");
    let store = super::GistStore::new().with_cache_dir(root.clone());
    assert_eq!(
        store.cache_file.as_deref(),
        Some(root.join("agr").join("index-cache.json").as_path())
    );
}
