//! `agr sync migrate`: recognise table, local index, move loop on `MemoryBackend` and `MemoryStore`.

use agr::sync::testing::{recognise, seal_meta, Kind, LegacyGist, Meta};
use agr::sync::Key;
use chrono::{DateTime, SecondsFormat};

const NOW: i64 = 1_800_000_000;
const HOUR: i64 = 3600;

fn at(secs_ago: i64) -> String {
    DateTime::from_timestamp(NOW - secs_ago, 0)
        .unwrap()
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn stub(age: i64) -> LegacyGist {
    LegacyGist {
        id: "s".into(),
        description: "agr:pending".into(),
        files: vec![(".keep".into(), 4)],
        created_at: at(age),
    }
}

fn meta() -> Meta {
    Meta {
        v: 1,
        path: "claude/a.cast.gz".into(),
        sha256: "00".repeat(32),
        size: 10,
        gen: 3,
    }
}

fn v1(desc: String, extra: &[&str]) -> LegacyGist {
    let mut files = vec![
        (".keep".into(), 4),
        ("manifest".into(), 9),
        ("data.000".into(), 9),
    ];
    files.extend(extra.iter().map(|n| (n.to_string(), 1)));
    LegacyGist {
        id: "g".into(),
        description: desc,
        files,
        created_at: at(5 * HOUR),
    }
}

mod table {
    use super::*;

    #[test]
    fn stub_younger_than_one_hour_is_kept() {
        let key = Key::generate();
        let kind = recognise(&stub(HOUR - 1), &key, NOW);
        assert_eq!(kind, Kind::Kept);
    }

    #[test]
    fn stub_over_one_hour_is_a_stub() {
        let key = Key::generate();
        assert_eq!(recognise(&stub(HOUR + 1), &key, NOW), Kind::Stub);
    }

    #[test]
    fn ours_v1_is_expected() {
        let key = Key::generate();
        let g = v1(format!("agr:v1 {}", seal_meta(&key, &meta())), &[]);
        assert_eq!(recognise(&g, &key, NOW), Kind::Ours(meta()));
    }

    #[test]
    fn other_key_unknown_version_and_extra_files_are_kept() {
        let key = Key::generate();
        let token = seal_meta(&key, &meta());
        let other = v1(
            format!("agr:v1 {}", seal_meta(&Key::generate(), &meta())),
            &[],
        );
        let newer = v1(format!("agr:v2 {token}"), &[]);
        let extra = v1(format!("agr:v1 {token}"), &["notes.txt"]);
        for g in [other, newer, extra] {
            assert_eq!(recognise(&g, &key, NOW), Kind::Kept, "{g:?}");
        }
    }

    #[test]
    fn a_stub_with_a_foreign_file_and_a_foreign_gist() {
        let key = Key::generate();
        let mut s = stub(2 * HOUR);
        s.files.push(("x.txt".into(), 1));
        assert_eq!(recognise(&s, &key, NOW), Kind::Kept);
        let mut plain = stub(2 * HOUR);
        plain.description = "my notes".into();
        assert_eq!(recognise(&plain, &key, NOW), Kind::Ignored);
    }
}

mod local_index {
    use agr::sync::testing::{convert_index, index};

    fn v1_text() -> String {
        let e = |sha: &str, gen: u32| {
            format!(
                r#"{{"sha256":"{sha}","remote_id":"gid","synced_at":"t","gen":{gen},"token":"tok","pending":false,"size":7,"mtime_ns":9}}"#
            )
        };
        let (a, b) = (e("h", 3), e("k", 5));
        format!(r#"{{"version":1,"entries":{{"claude/a.cast.gz":{a},"codex/b.cast.gz":{b}}}}}"#)
    }

    #[test]
    fn index_v1_gens_carry_into_v2() {
        let dir = tempfile::tempdir().unwrap();
        let v2 = convert_index(&v1_text()).unwrap().unwrap();
        std::fs::write(index::path_for(dir.path()), v2).unwrap();
        let (idx, warning) = index::load(dir.path());
        assert_eq!((idx.version, warning), (2, None));
        let got: Vec<_> = idx
            .entries
            .values()
            .map(|e| (e.sha256.as_str(), e.gen))
            .collect();
        assert_eq!(got, vec![("h", 3), ("k", 5)]);
        assert!(idx
            .entries
            .values()
            .all(|e| !e.pending && e.token.is_none()));
    }
}

mod moving {
    use crate::helpers::legacy_fake::{Listed, Rig, OLD};
    use agr::files::codec;
    use agr::sync::testing::index;
    use agr::sync::testing::{migrate_preview, migrate_run, sha256_hex};
    use agr::sync::{Engine, Options};
    use agr::{Config, StorageManager};

    const A: &str = "claude/a.cast.gz";

    fn engine_dir(dir: &std::path::Path) -> StorageManager {
        let mut config = Config::default();
        config.storage.directory = dir.to_string_lossy().to_string();
        StorageManager::new(config)
    }

    #[test]
    fn unsynced_local_edit_survives_migrate() {
        let rig = Rig::new();
        let old = b"old content".to_vec();
        rig.legacy.v1(&rig.key, (A, 3), &old, None);
        rig.write_v1_index(&[(A, &sha256_hex(&old), 3)]);
        let local = rig.dir.path().join(A);
        std::fs::create_dir_all(local.parent().unwrap()).unwrap();
        codec::atomic_write(&local, |w| Ok(w.write_all(b"edited after the last sync")?)).unwrap();
        let before = std::fs::read(&local).unwrap();
        let store = Listed(&rig.mem, &rig.legacy);
        migrate_run(&rig.env(&store)).unwrap();
        assert_eq!(std::fs::read(&local).unwrap(), before);
        let storage = engine_dir(rig.dir.path());
        let engine = Engine::new(&store, &rig.key, &storage, "w1", 64 * 1024 * 1024);
        let report = engine.push(&Options::default()).unwrap();
        assert_eq!(report.items.len(), 1);
        assert_eq!(rig.merged(A).unwrap().0, 4);
        assert!(index::load(rig.dir.path()).0.entries[A].gen >= 4);
    }

    #[test]
    fn unreadable_v1_is_kept_and_named() {
        let rig = Rig::new();
        let id = rig.legacy.v1_without_part(&rig.key, A);
        let store = Listed(&rig.mem, &rig.legacy);
        let report = migrate_run(&rig.env(&store)).unwrap();
        let named = |l: &String| l.contains("gh gist delete") && l.contains("cannot be read");
        assert!(report.lines.iter().any(named), "{:?}", report.lines);
        assert!(report.lines.iter().any(|l| l.contains(&id)));
        assert_eq!(rig.legacy.deletes().len(), 0);
    }

    #[test]
    fn dry_run_counts_and_writes_nothing() {
        let rig = Rig::new();
        let plain = vec![7u8; 1536];
        rig.legacy
            .v1(&rig.key, ("claude/x.cast.gz", 2), &plain, None);
        rig.legacy
            .v1(&rig.key, ("claude/y.cast.gz", 1), &plain, None);
        rig.legacy
            .v1(&rig.key, ("claude/z.cast.gz", 3), &plain, None);
        rig.put_moved("claude/z.cast.gz", 3, &plain);
        (0..4).for_each(|_| drop(rig.legacy.stub(OLD)));
        rig.legacy.stub(10);
        rig.legacy.foreign("claude/q.cast.gz");
        rig.legacy.plain(2);
        rig.write_v1_index(&[("claude/x.cast.gz", "h", 2)]);
        let before = (rig.mem.appends(), rig.index_text());
        let store = Listed(&rig.mem, &rig.legacy);
        let report = migrate_preview(&rig.env(&store)).unwrap();
        let line = "2 sessions to move (3 KiB), 1 already moved, 4 empty stubs to delete, 2 not ours (kept)";
        assert_eq!(report.lines, vec![line.to_string()]);
        assert_eq!(
            (rig.mem.appends() - before.0, rig.legacy.deletes().len()),
            (0, 0)
        );
        assert_eq!(rig.index_text(), before.1);
    }
}
