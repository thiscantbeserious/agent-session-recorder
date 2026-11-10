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
    use agr::sync::testing::{
        migrate_preview, migrate_run, sha256_hex, PackStore, Repo, RepoError,
    };
    use agr::sync::{Engine, Options};
    use agr::{Config, StorageManager};

    const TARGET: u64 = 64 * 1024;
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

    fn counts(rig: &Rig) -> (usize, usize) {
        (rig.mem.appends(), rig.legacy.deletes().len())
    }

    #[test]
    fn moved_session_keeps_gen_and_sha() {
        let rig = Rig::new();
        let plain = b"session body".to_vec();
        let id = rig.legacy.v1(&rig.key, (A, 3), &plain, None);
        let store = Listed(&rig.mem, &rig.legacy);
        let report = migrate_run(&rig.env(&store)).unwrap();
        assert_eq!(rig.merged(A), Some((3, sha256_hex(&plain))));
        assert_eq!(rig.legacy.deletes(), vec![id]);
        assert_eq!((report.moved, report.deleted), (1, 1));
    }

    #[test]
    fn already_moved_is_skipped() {
        let rig = Rig::new();
        let plain = b"session body".to_vec();
        rig.put_moved(A, 3, &plain);
        let id = rig.legacy.v1(&rig.key, (A, 3), &plain, None);
        let before = rig.mem.appends();
        let store = Listed(&rig.mem, &rig.legacy);
        migrate_run(&rig.env(&store)).unwrap();
        assert_eq!(rig.mem.appends() - before, 0);
        assert_eq!(rig.legacy.deletes(), vec![id]);
    }

    #[test]
    fn old_gist_deleted_only_after_the_index_shows_it() {
        let rig = Rig::new();
        rig.legacy.v1(&rig.key, (A, 3), b"body", None);
        rig.mem.before_append(|s, _| s.fail_list());
        let store = Listed(&rig.mem, &rig.legacy);
        let result = migrate_run(&rig.env(&store));
        assert!(result.is_err(), "{result:?}");
        assert_eq!(rig.legacy.deletes().len(), 0);
    }

    #[test]
    fn sha_mismatch_is_kept_and_reported() {
        let rig = Rig::new();
        let wrong = "ab".repeat(32);
        let id = rig.legacy.v1(&rig.key, (A, 3), b"body", Some(&wrong));
        let store = Listed(&rig.mem, &rig.legacy);
        let report = migrate_run(&rig.env(&store)).unwrap();
        assert!(report.kept.contains(&id), "{report:?}");
        assert_eq!(counts(&rig), (0, 0));
        assert!(report
            .lines
            .iter()
            .any(|l| l.contains(&format!("gh gist delete {id}"))));
    }

    #[test]
    fn rate_limit_stops_with_n_left_and_rerun_finishes() {
        let rig = Rig::new();
        for n in 0..5 {
            rig.legacy.v1(
                &rig.key,
                (&format!("claude/s{n}.cast.gz"), 1),
                b"body",
                None,
            );
        }
        let store = Listed(&rig.mem, &rig.legacy);
        rig.legacy.limit_deletes(Some(2));
        let first = migrate_run(&rig.env(&store)).unwrap();
        let out = first.lines.join("\n");
        assert!(out.contains("3 left, run again later"), "{out}");
        assert_eq!(rig.legacy.deletes().len(), 2);
        let appended = rig.mem.appends();
        rig.legacy.limit_deletes(None);
        let second = migrate_run(&rig.env(&store)).unwrap();
        assert_eq!((second.left, rig.legacy.deletes().len()), (0, 5));
        assert_eq!(
            rig.mem.appends(),
            appended,
            "the re-run moves nothing again"
        );
    }

    #[test]
    fn listing_past_3000_does_not_abort() {
        let rig = Rig::new();
        rig.legacy.plain(3100);
        rig.legacy.v1(&rig.key, (A, 1), b"body", None);
        let store = Listed(&rig.mem, &rig.legacy);
        let report = migrate_run(&rig.env(&store));
        assert!(report.is_ok(), "{report:?}");
        assert_eq!(rig.legacy.deletes().len(), 1);
    }

    #[test]
    fn migrate_aborts_on_partial_index_view() {
        let rig = Rig::new();
        rig.put_moved("claude/other.cast.gz", 1, b"x");
        rig.legacy.v1(&rig.key, (A, 3), b"body", None);
        rig.legacy.stub(OLD);
        let id = rig.mem.list().unwrap().packs[0].id.clone();
        rig.mem.fail_read_index(&id);
        let before = counts(&rig);
        let store = Listed(&rig.mem, &rig.legacy);
        assert!(migrate_run(&rig.env(&store)).is_err());
        assert!(migrate_preview(&rig.env(&store)).is_err());
        assert_eq!(counts(&rig), before);
        assert_eq!(rig.legacy.deletes().len(), 0);
    }

    #[test]
    fn migrate_opens_without_the_v1_guard_and_unblocks() {
        let rig = Rig::new();
        rig.legacy.v1(&rig.key, (A, 3), b"body", None);
        let bad = rig.legacy.v1(
            &rig.key,
            ("claude/b.cast.gz", 2),
            b"b",
            Some(&"cd".repeat(32)),
        );
        let store = Listed(&rig.mem, &rig.legacy);
        let open = || Repo::open(&store, &rig.key, "w1", TARGET, None).err();
        assert!(matches!(open(), Some(RepoError::NeedsMigrate)));
        let report = migrate_run(&rig.env(&store)).unwrap();
        assert!(report
            .lines
            .iter()
            .any(|l| l.contains(&format!("gh gist delete {bad}"))));
        assert!(
            matches!(open(), Some(RepoError::NeedsMigrate)),
            "the kept gist blocks"
        );
        rig.legacy.delete_by_hand(&bad);
        assert!(open().is_none());
    }

    #[test]
    fn a_session_the_view_holds_at_a_higher_gen_with_other_content_is_not_deleted() {
        let rig = Rig::new();
        rig.put_moved(A, 5, b"newer content from another machine");
        let id = rig.legacy.v1(&rig.key, (A, 3), b"old content", None);
        let store = Listed(&rig.mem, &rig.legacy);
        let report = migrate_run(&rig.env(&store)).unwrap();
        assert_eq!(rig.legacy.deletes().len(), 0, "{report:?}");
        assert!(report
            .lines
            .iter()
            .any(|l| l.contains(&format!("gh gist delete {id}"))));
    }
}
