//! Sealing, metadata tokens, recovery phrase and hashing for session sync.
//!
//! One-shot seal v1 (`seal`, metadata tokens): `b"AGR1" || 0x01 || nonce [24] || ciphertext+tag`.
//! Session blobs v2 are streamed in 64 KiB frames, see `blob` and `blob/stream.rs`.

pub(in crate::sync) mod blob;
mod phrase;
mod seal;

pub use phrase::PhraseError;
pub(in crate::sync) use phrase::{export_phrase, import_phrase};
pub(in crate::sync) use seal::open;
pub use seal::{open_meta, seal, seal_meta, Meta};

use chacha20poly1305::aead::{Generate, KeyInit};
use chacha20poly1305::XChaCha20Poly1305;
use data_encoding::{BASE32_NOPAD, HEXLOWER};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zeroize::Zeroize;

const KEY_LEN: usize = 32;

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
    #[error("blob is too long for one stream")]
    TooLong,
    #[error("sealed with another key")]
    OtherKey,
    #[error("metadata token is not valid base64url")]
    BadToken,
    #[error("metadata is not valid")]
    BadMeta,
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
        Key(chacha20poly1305::Key::generate().into())
    }

    pub(in crate::sync) fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Key(bytes)
    }

    pub(in crate::sync) fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    pub(super) fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(&self.0.into())
    }
}

/// Random id of `n` bytes (at most 24) as base32 without padding.
pub(in crate::sync) fn random_id_base32(n: usize) -> String {
    let nonce = chacha20poly1305::XNonce::generate();
    BASE32_NOPAD.encode(&nonce[..n])
}

pub fn sha256_hex(data: &[u8]) -> String {
    HEXLOWER.encode(&Sha256::digest(data))
}

/// SHA-256 (hex) and byte count of a reader, streamed.
pub(in crate::sync) fn sha256_reader(
    mut src: impl std::io::Read,
) -> std::io::Result<(String, u64)> {
    let mut hasher = Sha256::new();
    let size = std::io::copy(&mut src, &mut hasher)?;
    Ok((HEXLOWER.encode(&hasher.finalize()), size))
}

#[cfg(test)]
mod tests {
    use super::{sha256_hex, sha256_reader, Key};

    #[test]
    fn sha256_matches_nist_abc() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn random_id_base32_is_26_chars_for_16_bytes() {
        assert_eq!(super::random_id_base32(16).len(), 26);
    }

    #[test]
    fn sha256_reader_hashes_and_counts() {
        let got = sha256_reader(&b"abc"[..]).unwrap();
        assert_eq!(got, (sha256_hex(b"abc"), 3));
        assert_eq!(sha256_reader(&b""[..]).unwrap(), (sha256_hex(b""), 0));
    }

    /// 0.11 wipes the key copy in the cipher only with the `zeroize` feature, this stops
    /// compiling when the feature is dropped from Cargo.toml.
    #[test]
    fn cipher_wipes_its_key_on_drop() {
        fn assert_zod<T: zeroize::ZeroizeOnDrop>() {}
        assert_zod::<chacha20poly1305::XChaCha20Poly1305>();
    }

    #[test]
    fn generated_keys_differ_and_debug_is_redacted() {
        let (a, b) = (Key::generate(), Key::generate());
        assert_ne!(a.as_bytes(), b.as_bytes());
        assert_eq!(format!("{a:?}"), "Key(..)");
    }
}
