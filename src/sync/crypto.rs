//! Sealing, metadata tokens, recovery phrase and hashing for session sync.
//!
//! Blob format: `b"AGR1" || version u8 || nonce [24] || ciphertext+tag`, XChaCha20-Poly1305.

use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::XChaCha20Poly1305;
use data_encoding::{BASE32_NOPAD, BASE64URL_NOPAD, HEXLOWER};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zeroize::Zeroize;

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
const CHECKSUM_LEN: usize = 2;
const MAGIC: &[u8] = b"AGR1";
const VERSION: u8 = 1;
const HEADER_LEN: usize = MAGIC.len() + 1;
const MIN_BLOB_LEN: usize = HEADER_LEN + NONCE_LEN + TAG_LEN;
const BLOB_AAD_PREFIX: &[u8] = b"agr:blob:";
const META_AAD: &[u8] = b"agr:meta";
const PHRASE_PREFIX: &str = "agr1";
const PHRASE_BODY_LEN: usize = 55;

/// Errors from opening a sealed blob or token.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CryptoError {
    #[error("sealed data is too short")]
    Truncated,
    #[error("not an agr blob")]
    BadMagic,
    #[error("unsupported blob version {0}")]
    UnsupportedVersion(u8),
    #[error("authentication failed (tampered data, wrong key or wrong context)")]
    Authentication,
    #[error("sealed with another key")]
    OtherKey,
    #[error("metadata token is not valid base64url")]
    BadToken,
    #[error("metadata is not valid")]
    BadMeta,
}

/// Errors from importing a recovery phrase.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PhraseError {
    #[error("recovery phrase must start with agr1-")]
    Prefix,
    #[error("recovery phrase has the wrong length")]
    Length,
    #[error("recovery phrase has invalid characters")]
    Encoding,
    #[error("recovery phrase checksum does not match, check for typos")]
    Checksum,
}

/// Sealed session metadata, carried in the gist description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    pub v: u8,
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub gen: u64,
}

/// 256-bit sync key. Never printed, zeroized on drop.
pub struct Key([u8; KEY_LEN]);

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Key(..)")
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl Key {
    pub fn generate() -> Self {
        Key(XChaCha20Poly1305::generate_key(&mut OsRng).into())
    }

    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Key(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    pub(super) fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(&self.0.into())
    }
}

pub fn sha256_hex(data: &[u8]) -> String {
    HEXLOWER.encode(&Sha256::digest(data))
}

/// SHA-256 (hex) and byte count of a reader, streamed.
pub fn sha256_reader(mut src: impl std::io::Read) -> std::io::Result<(String, u64)> {
    let mut hasher = Sha256::new();
    let size = std::io::copy(&mut src, &mut hasher)?;
    Ok((HEXLOWER.encode(&hasher.finalize()), size))
}

/// AAD binding a blob to the header, its session path and its generation.
pub fn blob_aad(path: &str, gen: u64) -> Vec<u8> {
    let tail = format!(":{gen}");
    [
        MAGIC,
        &[VERSION],
        BLOB_AAD_PREFIX,
        path.as_bytes(),
        tail.as_bytes(),
    ]
    .concat()
}

pub fn seal(key: &Key, aad: &[u8], plain: &[u8]) -> Vec<u8> {
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    seal_with_nonce(key, &nonce.into(), aad, plain)
}

fn seal_with_nonce(key: &Key, nonce: &[u8; NONCE_LEN], aad: &[u8], plain: &[u8]) -> Vec<u8> {
    let payload = Payload { msg: plain, aad };
    let ct = key
        .cipher()
        .encrypt(nonce.into(), payload)
        .expect("XChaCha20-Poly1305 encryption cannot fail for in-memory input");
    [MAGIC, &[VERSION], nonce, &ct].concat()
}

pub fn open(key: &Key, aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if blob.len() < MIN_BLOB_LEN {
        return Err(CryptoError::Truncated);
    }
    if &blob[..4] != MAGIC {
        return Err(CryptoError::BadMagic);
    }
    if blob[4] != VERSION {
        return Err(CryptoError::UnsupportedVersion(blob[4]));
    }
    let (nonce, ct) = blob[HEADER_LEN..].split_at(NONCE_LEN);
    let payload = Payload { msg: ct, aad };
    key.cipher()
        .decrypt(nonce.into(), payload)
        .map_err(|_| CryptoError::Authentication)
}

pub fn seal_meta(key: &Key, meta: &Meta) -> String {
    let json = serde_json::to_vec(meta).expect("Meta serializes to JSON");
    BASE64URL_NOPAD.encode(&seal(key, META_AAD, &json))
}

pub fn open_meta(key: &Key, token: &str) -> Result<Meta, CryptoError> {
    let raw = BASE64URL_NOPAD
        .decode(token.as_bytes())
        .map_err(|_| CryptoError::BadToken)?;
    let json = open(key, META_AAD, &raw).map_err(|e| match e {
        CryptoError::Authentication => CryptoError::OtherKey,
        other => other,
    })?;
    serde_json::from_slice(&json).map_err(|_| CryptoError::BadMeta)
}

fn checksum(key: &[u8]) -> [u8; CHECKSUM_LEN] {
    let digest = Sha256::digest(key);
    [digest[0], digest[1]]
}

/// `agr1-` plus the base32 of `key || sha256(key)[..2]` in groups of five.
pub fn export_phrase(key: &Key) -> String {
    let mut raw = key.0.to_vec();
    raw.extend_from_slice(&checksum(&key.0));
    let encoded = BASE32_NOPAD.encode(&raw);
    let groups: Vec<&str> = encoded
        .as_bytes()
        .chunks(5)
        .map(|c| std::str::from_utf8(c).expect("base32 is ascii"))
        .collect();
    format!("{PHRASE_PREFIX}-{}", groups.join("-"))
}

pub fn import_phrase(phrase: &str) -> Result<Key, PhraseError> {
    let clean: String = phrase
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .to_uppercase();
    let body = clean
        .strip_prefix(&PHRASE_PREFIX.to_uppercase())
        .ok_or(PhraseError::Prefix)?;
    if body.len() != PHRASE_BODY_LEN {
        return Err(PhraseError::Length);
    }
    let raw = BASE32_NOPAD
        .decode(body.as_bytes())
        .map_err(|_| PhraseError::Encoding)?;
    let (key, sum) = raw.split_at(KEY_LEN);
    if checksum(key) != sum {
        return Err(PhraseError::Checksum);
    }
    Ok(Key(key.try_into().expect("split at KEY_LEN")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KAT_KEY: &str = "808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f";
    const KAT_NONCE: &str = "404142434445464748494a4b4c4d4e4f5051525354555657";
    const KAT_AAD: &str = "50515253c0c1c2c3c4c5c6c7";
    const KAT_PLAIN: &str = "Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
    const KAT_CT: &str = "bd6d179d3e83d43b9576579493c0e939572a1700252bfaccbed2902c21396cbb731c7f1b0b4aa6440bf3a82f4eda7e39ae64c6708c54c216cb96b72e1213b4522f8c9ba40db5d945b11b69b982c1bb9e3f3fac2bc369488f76b2383565d3fff921f9664c97637da9768812f615c68b13b52ec0875924c1c7987947deafd8780acf49";

    fn hex(s: &str) -> Vec<u8> {
        data_encoding::HEXLOWER.decode(s.as_bytes()).unwrap()
    }

    fn key(b: u8) -> Key {
        Key::from_bytes([b; 32])
    }

    fn sealed() -> Vec<u8> {
        seal(&key(1), b"aad", b"hello session")
    }

    #[test]
    fn sha256_matches_nist_abc() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_reader_hashes_and_counts() {
        let got = sha256_reader(&b"abc"[..]).unwrap();
        assert_eq!(got, (sha256_hex(b"abc"), 3));
        assert_eq!(sha256_reader(&b""[..]).unwrap(), (sha256_hex(b""), 0));
    }

    #[test]
    fn seal_matches_xchacha_known_answer() {
        let key = Key::from_bytes(hex(KAT_KEY).try_into().unwrap());
        let nonce: [u8; 24] = hex(KAT_NONCE).try_into().unwrap();
        let blob = seal_with_nonce(&key, &nonce, &hex(KAT_AAD), KAT_PLAIN.as_bytes());
        assert_eq!(&blob[..5], b"AGR1\x01");
        assert_eq!(&blob[5..29], &nonce);
        assert_eq!(&blob[29..], hex(KAT_CT).as_slice());
    }

    #[test]
    fn seal_open_round_trip_with_header() {
        let blob = sealed();
        assert_eq!(&blob[..5], b"AGR1\x01");
        assert_eq!(open(&key(1), b"aad", &blob).unwrap(), b"hello session");
    }

    #[test]
    fn two_seals_differ() {
        assert_ne!(sealed(), sealed());
    }

    #[test]
    fn open_fails_on_flipped_ciphertext_byte() {
        let mut blob = sealed();
        blob[30] ^= 1;
        assert_eq!(
            open(&key(1), b"aad", &blob),
            Err(CryptoError::Authentication)
        );
    }

    #[test]
    fn open_fails_on_flipped_tag_byte() {
        let mut blob = sealed();
        let last = blob.len() - 1;
        blob[last] ^= 1;
        assert_eq!(
            open(&key(1), b"aad", &blob),
            Err(CryptoError::Authentication)
        );
    }

    #[test]
    fn open_fails_on_wrong_key_and_wrong_aad() {
        let blob = sealed();
        assert_eq!(
            open(&key(2), b"aad", &blob),
            Err(CryptoError::Authentication)
        );
        assert_eq!(
            open(&key(1), b"other", &blob),
            Err(CryptoError::Authentication)
        );
    }

    #[test]
    fn open_fails_on_truncated_blob() {
        let blob = sealed();
        assert_eq!(
            open(&key(1), b"aad", &blob[..20]),
            Err(CryptoError::Truncated)
        );
        assert_eq!(open(&key(1), b"aad", &[]), Err(CryptoError::Truncated));
    }

    #[test]
    fn open_fails_on_unknown_version_and_magic() {
        let mut blob = sealed();
        blob[4] = 2;
        assert_eq!(
            open(&key(1), b"aad", &blob),
            Err(CryptoError::UnsupportedVersion(2))
        );
        blob[0] = b'X';
        assert_eq!(open(&key(1), b"aad", &blob), Err(CryptoError::BadMagic));
    }

    #[test]
    fn blob_aad_binds_header_path_and_gen() {
        assert_eq!(
            blob_aad("claude/x.cast.gz", 3),
            b"AGR1\x01agr:blob:claude/x.cast.gz:3"
        );
    }

    #[test]
    fn blob_sealed_for_gen_3_opens_only_with_gen_3_and_its_path() {
        let blob = seal(&key(1), &blob_aad("a/x.cast.gz", 3), b"p");
        assert_eq!(
            open(&key(1), &blob_aad("a/x.cast.gz", 3), &blob).unwrap(),
            b"p"
        );
        assert_eq!(
            open(&key(1), &blob_aad("a/x.cast.gz", 2), &blob),
            Err(CryptoError::Authentication)
        );
        assert_eq!(
            open(&key(1), &blob_aad("a/y.cast.gz", 3), &blob),
            Err(CryptoError::Authentication)
        );
    }

    #[test]
    fn sealed_gzip_session_shows_no_magic_or_text() {
        let text = "secret customer ticket ABC-123 output line";
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut enc, text.as_bytes()).unwrap();
        let gz = enc.finish().unwrap();
        assert_eq!(&gz[..2], &[0x1f, 0x8b]);
        let blob = seal(&key(1), b"aad", &gz);
        assert!(blob.len() >= gz.len() + 45);
        assert_ne!(&blob[..2], &[0x1f, 0x8b]);
        assert!(!blob
            .windows(6)
            .any(|w| text.as_bytes().windows(6).any(|t| t == w)));
    }

    fn meta() -> Meta {
        Meta {
            v: 2,
            path: "claude/x.cast.gz".into(),
            sha256: "ab".repeat(32),
            size: 123,
            gen: 4,
        }
    }

    #[test]
    fn meta_round_trips_as_base64url() {
        let token = seal_meta(&key(1), &meta());
        assert!(!token.is_empty());
        assert!(token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
        assert_eq!(open_meta(&key(1), &token).unwrap(), meta());
    }

    #[test]
    fn meta_token_for_another_key_is_other_key() {
        let token = seal_meta(&key(1), &meta());
        assert_eq!(open_meta(&key(2), &token), Err(CryptoError::OtherKey));
    }

    #[test]
    fn meta_token_garbage_is_bad_token() {
        assert_eq!(
            open_meta(&key(1), "!!not base64!!"),
            Err(CryptoError::BadToken)
        );
    }

    #[test]
    fn meta_token_is_not_a_blob_for_a_path() {
        let token = seal_meta(&key(1), &meta());
        let raw = data_encoding::BASE64URL_NOPAD
            .decode(token.as_bytes())
            .unwrap();
        assert_eq!(
            open(&key(1), &blob_aad("claude/x.cast.gz", 4), &raw),
            Err(CryptoError::Authentication)
        );
    }

    #[test]
    fn generated_keys_differ_and_debug_is_redacted() {
        let (a, b) = (Key::generate(), Key::generate());
        assert_ne!(a.as_bytes(), b.as_bytes());
        assert_eq!(format!("{a:?}"), "Key(..)");
    }

    #[test]
    fn phrase_round_trips_with_prefix_and_groups() {
        let k = key(7);
        let phrase = export_phrase(&k);
        assert!(phrase.starts_with("agr1-"));
        assert_eq!(phrase.len(), 5 + 55 + 10);
        assert_eq!(import_phrase(&phrase).unwrap().as_bytes(), k.as_bytes());
    }

    #[test]
    fn phrase_import_is_lenient_about_format() {
        let k = key(9);
        let phrase = export_phrase(&k);
        let messy = format!("  {} ", phrase.to_lowercase().replace('-', " "));
        assert_eq!(import_phrase(&messy).unwrap().as_bytes(), k.as_bytes());
        let nodash = phrase.replace('-', "");
        assert_eq!(import_phrase(&nodash).unwrap().as_bytes(), k.as_bytes());
    }

    #[test]
    fn phrase_rejects_wrong_prefix_and_length() {
        let phrase = export_phrase(&key(3));
        let bad_prefix = phrase.replacen("agr1", "agr2", 1);
        assert_eq!(import_phrase(&bad_prefix).err(), Some(PhraseError::Prefix));
        assert_eq!(
            import_phrase(&phrase[..phrase.len() - 6]).err(),
            Some(PhraseError::Length)
        );
        assert_eq!(import_phrase("agr1-").err(), Some(PhraseError::Length));
    }

    #[test]
    fn phrase_rejects_any_single_changed_character() {
        let phrase = export_phrase(&key(5));
        let body_start = 5;
        assert_eq!(phrase.len(), 70);
        for i in (body_start..phrase.len()).filter(|&i| phrase.as_bytes()[i] != b'-') {
            let mut bytes = phrase.clone().into_bytes();
            bytes[i] = if bytes[i] == b'A' { b'B' } else { b'A' };
            let changed = String::from_utf8(bytes).unwrap();
            assert!(import_phrase(&changed).is_err(), "change at {i} accepted");
        }
    }

    #[test]
    fn phrase_rejects_checksum_mismatch() {
        let phrase = export_phrase(&key(5));
        let mut bytes = phrase.into_bytes();
        let i = 11;
        bytes[i] = if bytes[i] == b'A' { b'B' } else { b'A' };
        let changed = String::from_utf8(bytes).unwrap();
        assert_eq!(import_phrase(&changed).err(), Some(PhraseError::Checksum));
    }
}
