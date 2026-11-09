//! Sealed pack index: JSON v3 under AAD `agr:pack-index:` plus the pack uuid.

use crate::sync::crypto::{open, seal, CryptoError, Key};
use chacha20poly1305::aead::Generate;
use chacha20poly1305::XNonce;
use data_encoding::{BASE32_NOPAD, BASE64URL_NOPAD};
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
    #[error("pack uuid must be non-empty base32 without a dot")]
    BadUuid,
    #[error("pack index is not valid")]
    Bad,
}

const AAD_PREFIX: &str = "agr:pack-index:";
const VERSION: u8 = 3;

fn aad(uuid: &str) -> Vec<u8> {
    format!("{AAD_PREFIX}{uuid}").into_bytes()
}

/// `<uuid>.<base64url(sealed json)>`. The uuid is also the public pack tag, the plain prefix only
/// tells "other pack" from "other key", the AAD still binds it.
pub fn seal_index(key: &Key, idx: &Index) -> Result<String, IndexError> {
    let json = serde_json::to_vec(idx).expect("Index serializes to JSON");
    let sealed = seal(key, &aad(&idx.pack), &json);
    Ok(format!("{}.{}", idx.pack, BASE64URL_NOPAD.encode(&sealed)))
}

pub fn open_index(key: &Key, uuid: &str, text: &str) -> Result<Index, IndexError> {
    let (pack, body) = text.split_once('.').ok_or(IndexError::Bad)?;
    if pack != uuid {
        return Err(IndexError::OtherPack);
    }
    let raw = BASE64URL_NOPAD
        .decode(body.as_bytes())
        .map_err(|_| IndexError::Bad)?;
    let json = open(key, &aad(uuid), &raw).map_err(|e| match e {
        CryptoError::Authentication => IndexError::OtherKey,
        _ => IndexError::Bad,
    })?;
    #[derive(Deserialize)]
    struct Head {
        v: u8,
    }
    let head: Head = serde_json::from_slice(&json).map_err(|_| IndexError::Bad)?;
    match head.v {
        VERSION => serde_json::from_slice(&json).map_err(|_| IndexError::Bad),
        v => Err(IndexError::UnknownVersion(v)),
    }
}

/// Random 128 bit pack id as 26 base32 characters.
pub fn new_pack_uuid() -> String {
    let nonce = XNonce::generate();
    BASE32_NOPAD.encode(&nonce[..16])
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
        let sealed =
            crate::sync::crypto::seal(&key, &super::aad("PACK"), br#"{"v":4,"something":"else"}"#);
        let text = format!("PACK.{}", super::BASE64URL_NOPAD.encode(&sealed));
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
}
