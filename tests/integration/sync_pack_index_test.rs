//! Sealed pack index text.

use agr::sync::testing::{new_pack_uuid, open_index, seal, seal_index, Entry, Index, IndexError};
use agr::sync::Key;

fn sample(pack: &str) -> Index {
    Index {
        v: 3,
        pack: pack.to_string(),
        writer: "w1".to_string(),
        sealed: false,
        len: 42,
        entries: vec![Entry {
            path: "a/b.cast".to_string(),
            gen: 2,
            sha256: "ab".repeat(32),
            size: 10,
            offset: 0,
            length: 42,
            deleted: false,
            at: "2026-11-09T10:00:00Z".to_string(),
        }],
    }
}

#[test]
fn index_round_trips_through_seal() {
    let key = Key::generate();
    let idx = sample("PACK");
    let opened = open_index(&key, "PACK", &seal_index(&key, &idx).unwrap()).unwrap();
    assert_eq!(opened, idx);
}

#[test]
fn index_with_other_key_is_other_key() {
    let text = seal_index(&Key::generate(), &sample("PACK")).unwrap();
    let err = open_index(&Key::generate(), "PACK", &text).err();
    assert!(matches!(err, Some(IndexError::OtherKey)));
}

#[test]
fn index_with_v_4_is_unknown_version() {
    let key = Key::generate();
    let idx = Index {
        v: 4,
        ..sample("PACK")
    };
    let err = open_index(&key, "PACK", &seal_index(&key, &idx).unwrap()).err();
    assert!(matches!(err, Some(IndexError::UnknownVersion(4))));
}

#[test]
fn index_v4_with_other_shape_is_unknown_version() {
    let key = Key::generate();
    let sealed = seal(
        &key,
        b"agr:pack-index:PACK",
        br#"{"v":4,"something":"else"}"#,
    );
    let text = format!("PACK.{}", data_encoding::BASE64URL_NOPAD.encode(&sealed));
    let err = open_index(&key, "PACK", &text).unwrap_err();
    assert!(matches!(err, IndexError::UnknownVersion(4)), "{err:?}");
}

#[test]
fn seal_index_rejects_a_uuid_with_a_dot() {
    let key = Key::generate();
    assert!(seal_index(&key, &sample("AB.CD")).is_err());
}

#[test]
fn index_text_is_base64url() {
    let text = seal_index(&Key::generate(), &sample("PACK")).unwrap();
    assert!(!text.contains(['=', '+', '/']));
}

#[test]
fn index_from_another_pack_is_rejected() {
    let key = Key::generate();
    let text = seal_index(&key, &sample("PACK")).unwrap();
    assert!(matches!(
        open_index(&key, "OTHER", &text),
        Err(IndexError::OtherPack)
    ));
}

#[test]
fn two_seals_of_one_index_differ() {
    let (k, i) = (Key::generate(), sample("PACK"));
    assert_ne!(seal_index(&k, &i).unwrap(), seal_index(&k, &i).unwrap());
}

#[test]
fn pack_uuid_is_26_base32_chars() {
    assert_eq!(new_pack_uuid().len(), 26);
}
