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
