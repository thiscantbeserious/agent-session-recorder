//! STREAM BE32 (Hoang, Reyhanitabar, Rogaway, Vizar 2015) over XChaCha20-Poly1305.
//! Nonce = prefix[19] || counter u32 BE || last flag u8, as aead 0.5 `StreamBE32`.

use crate::sync::crypto::{CryptoError, Key};
use chacha20poly1305::aead::AeadInOut;
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

pub(super) const PREFIX_LEN: usize = 19;
pub(super) const TAG_LEN: usize = 16;

/// Shared state. Deliberately not Clone or Copy: two copies would hand out the same nonces.
struct Stream {
    aead: XChaCha20Poly1305,
    prefix: [u8; PREFIX_LEN],
    counter: u32,
}

impl Stream {
    fn new(key: &Key, prefix: [u8; PREFIX_LEN]) -> Self {
        Stream {
            aead: key.cipher(),
            prefix,
            counter: 0,
        }
    }

    /// prefix || counter BE || flag, the only place a nonce is built.
    fn nonce(&self, last: bool) -> XNonce {
        let mut n = [0u8; 24];
        n[..PREFIX_LEN].copy_from_slice(&self.prefix);
        n[PREFIX_LEN..PREFIX_LEN + 4].copy_from_slice(&self.counter.to_be_bytes());
        n[PREFIX_LEN + 4] = u8::from(last);
        XNonce::from(n)
    }

    /// Nonce for a non-last frame. The counter moves on BEFORE the AEAD runs, and
    /// `checked_add` refuses at u32::MAX, nothing is encrypted then.
    fn take_next(&mut self) -> Result<XNonce, CryptoError> {
        let next = self.counter.checked_add(1).ok_or(CryptoError::TooLong)?;
        let nonce = self.nonce(false);
        self.counter = next;
        Ok(nonce)
    }

    fn seal(&self, nonce: &XNonce, aad: &[u8], buf: &mut Vec<u8>) -> Result<(), CryptoError> {
        let res = self.aead.encrypt_in_place(nonce, aad, buf);
        res.map_err(|_| CryptoError::TooLong)
    }

    /// Any failure clears `buf`, so no plaintext leaves a frame that did not verify.
    fn open(&self, nonce: &XNonce, aad: &[u8], buf: &mut Vec<u8>) -> Result<(), CryptoError> {
        self.aead.decrypt_in_place(nonce, aad, buf).map_err(|_| {
            buf.clear();
            CryptoError::Authentication
        })
    }

    #[cfg(test)]
    fn at(key: &Key, prefix: [u8; PREFIX_LEN], counter: u32) -> Self {
        Stream {
            counter,
            ..Stream::new(key, prefix)
        }
    }
}

pub(super) struct Sealer(Stream);

impl Sealer {
    pub(super) fn new(key: &Key, prefix: [u8; PREFIX_LEN]) -> Self {
        Sealer(Stream::new(key, prefix))
    }

    pub(super) fn next(&mut self, aad: &[u8], buf: &mut Vec<u8>) -> Result<(), CryptoError> {
        let nonce = self.0.take_next()?;
        self.0.seal(&nonce, aad, buf)
    }

    pub(super) fn last(self, aad: &[u8], buf: &mut Vec<u8>) -> Result<(), CryptoError> {
        self.0.seal(&self.0.nonce(true), aad, buf)
    }
}

pub(super) struct Opener(Stream);

impl Opener {
    pub(super) fn new(key: &Key, prefix: [u8; PREFIX_LEN]) -> Self {
        Opener(Stream::new(key, prefix))
    }

    pub(super) fn next(&mut self, aad: &[u8], buf: &mut Vec<u8>) -> Result<(), CryptoError> {
        let nonce = self.0.take_next()?;
        self.0.open(&nonce, aad, buf)
    }

    pub(super) fn last(self, aad: &[u8], buf: &mut Vec<u8>) -> Result<(), CryptoError> {
        self.0.open(&self.0.nonce(true), aad, buf)
    }
}

#[cfg(test)]
mod tests;
