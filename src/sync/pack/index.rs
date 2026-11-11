//! Sealed pack index: JSON v3 under AAD `agr:pack-index:` plus the pack uuid.

use crate::sync::crypto::{open, random_id_base32, seal, CryptoError, Key};
use data_encoding::BASE64URL_NOPAD;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One session range in the pack stream, a tombstone has `deleted` set.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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

/// `<uuid>.<base64url(sealed json)>`, the uuid is base32 and the body base64url, so neither has a dot. The uuid is also the public pack tag, the plain prefix only
/// tells "other pack" from "other key", the AAD still binds it.
pub fn seal_index(key: &Key, idx: &Index) -> Result<String, IndexError> {
    let ok = |c: char| c.is_ascii_uppercase() || ('2'..='7').contains(&c);
    if idx.pack.is_empty() || !idx.pack.chars().all(ok) {
        return Err(IndexError::BadUuid);
    }
    let json = serde_json::to_vec(idx).expect("Index serializes to JSON");
    let sealed = seal(key, &aad(&idx.pack), &json);
    Ok(format!("{}.{}", idx.pack, BASE64URL_NOPAD.encode(&sealed)))
}

/// Measures the sealed text of one index so a plan can ask how long it gets with more entries.
pub struct TextLen {
    text: usize,
    raw: usize,
}

impl TextLen {
    /// The sealed text length when the JSON grows by `extra` bytes.
    pub fn with_extra(&self, extra: usize) -> usize {
        let enc = |n| BASE64URL_NOPAD.encode_len(n);
        self.text - enc(self.raw) + enc(self.raw + extra)
    }
}

/// Seals `idx` once and keeps what is needed to size its growth, the seal overhead is not named here.
pub fn text_len(key: &Key, idx: &Index) -> Result<TextLen, IndexError> {
    let text = seal_index(key, idx)?;
    let body = text.rsplit_once('.').map_or(0, |(_, b)| b.len());
    let raw = BASE64URL_NOPAD
        .decode_len(body)
        .map_err(|_| IndexError::Bad)?;
    Ok(TextLen {
        text: text.len(),
        raw,
    })
}

pub fn open_index(key: &Key, uuid: &str, text: &str) -> Result<Index, IndexError> {
    let (pack, body) = text.rsplit_once('.').ok_or(IndexError::Bad)?;
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
    random_id_base32(16)
}
