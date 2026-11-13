//! One-shot v1 seal and open (`AGR1 || 0x01`) and the sealed metadata token.

use super::{CryptoError, Key};
use chacha20poly1305::aead::{Aead, Generate, Payload};
use chacha20poly1305::XNonce;
use data_encoding::BASE64URL_NOPAD;
use serde::{Deserialize, Serialize};

const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
const MAGIC: &[u8] = b"AGR1";
const VERSION: u8 = 1;
const HEADER_LEN: usize = MAGIC.len() + 1;
const MIN_BLOB_LEN: usize = HEADER_LEN + NONCE_LEN + TAG_LEN;
const META_AAD: &[u8] = b"agr:meta";

/// Sealed session metadata, carried in the gist description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    pub v: u8,
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub gen: u64,
}

pub fn seal(key: &Key, aad: &[u8], plain: &[u8]) -> Vec<u8> {
    let nonce = XNonce::generate();
    seal_with_nonce(key, &nonce.into(), aad, plain)
}

fn seal_with_nonce(key: &Key, nonce: &[u8; NONCE_LEN], aad: &[u8], plain: &[u8]) -> Vec<u8> {
    let payload = Payload { msg: plain, aad };
    let ct = key
        .cipher()
        .encrypt(&XNonce::from(*nonce), payload)
        .expect("XChaCha20-Poly1305 encryption cannot fail for in-memory input");
    [MAGIC, &[VERSION], nonce, &ct].concat()
}

pub(in crate::sync) fn open(key: &Key, aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, CryptoError> {
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
        .decrypt(
            &XNonce::try_from(nonce).expect("split at NONCE_LEN"),
            payload,
        )
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

// inline: seal_with_nonce and open are private to crypto, the known-answer test needs a fixed nonce
#[cfg(test)]
mod tests {
    use super::{open, open_meta, seal, seal_meta, seal_with_nonce, CryptoError, Key, Meta};

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
            open(&key(1), b"agr:blob:claude/x.cast.gz:4", &raw),
            Err(CryptoError::Authentication)
        );
    }
}
