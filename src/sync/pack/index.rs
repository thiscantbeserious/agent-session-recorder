//! Sealed pack index: JSON v3 under AAD `agr:pack-index:` plus the pack uuid.

use crate::sync::crypto::Key;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One session range in the pack stream, a tombstone has `deleted` set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub path: String,
    pub gen: u64,
    pub sha256: String,
    pub size: u64,
    pub offset: u64,
    pub length: u64,
    pub deleted: bool,
    pub at: String,
}

/// Index of one pack, stored next to its data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    pub v: u8,
    pub pack: String,
    pub writer: String,
    pub sealed: bool,
    pub len: u64,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IndexError {
    #[error("pack index sealed with another key")]
    OtherKey,
    #[error("pack index belongs to another pack")]
    OtherPack,
    #[error("unknown pack index version {0}")]
    UnknownVersion(u8),
    #[error("pack index is not valid")]
    Bad,
}

pub fn seal_index(_key: &Key, _idx: &Index) -> String {
    "=+/".to_string()
}

pub fn open_index(_key: &Key, uuid: &str, _text: &str) -> Result<Index, IndexError> {
    Ok(Index {
        v: 3,
        pack: uuid.to_string(),
        writer: String::new(),
        sealed: false,
        len: 0,
        entries: Vec::new(),
    })
}

pub fn new_pack_uuid() -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::{new_pack_uuid, open_index, seal_index, Entry, Index, IndexError};
    use crate::sync::crypto::Key;

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
        let opened = open_index(&key, "PACK", &seal_index(&key, &idx)).unwrap();
        assert_eq!(opened, idx);
    }

    #[test]
    fn index_with_other_key_is_other_key() {
        let text = seal_index(&Key::generate(), &sample("PACK"));
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
        let err = open_index(&key, "PACK", &seal_index(&key, &idx)).err();
        assert!(matches!(err, Some(IndexError::UnknownVersion(4))));
    }

    #[test]
    fn index_text_is_base64url() {
        let text = seal_index(&Key::generate(), &sample("PACK"));
        assert!(!text.contains(['=', '+', '/']));
    }

    #[test]
    fn index_from_another_pack_is_rejected() {
        let key = Key::generate();
        let text = seal_index(&key, &sample("PACK"));
        assert!(matches!(
            open_index(&key, "OTHER", &text),
            Err(IndexError::OtherPack)
        ));
    }

    #[test]
    fn two_seals_of_one_index_differ() {
        let (k, i) = (Key::generate(), sample("PACK"));
        assert_ne!(seal_index(&k, &i), seal_index(&k, &i));
    }

    #[test]
    fn pack_uuid_is_26_base32_chars() {
        assert_eq!(new_pack_uuid().len(), 26);
    }
}
