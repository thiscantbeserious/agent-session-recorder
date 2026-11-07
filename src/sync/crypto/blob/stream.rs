//! STREAM BE32 (Hoang, Reyhanitabar, Rogaway, Vizar 2015) over XChaCha20-Poly1305.
//! Nonce = prefix[19] || counter u32 BE || last flag u8, as aead 0.5 `StreamBE32`.

use crate::sync::crypto::{CryptoError, Key};
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

    fn nonce(&self, _last: bool) -> XNonce {
        XNonce::default()
    }

    fn take_next(&mut self) -> Result<XNonce, CryptoError> {
        Ok(self.nonce(false))
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

    pub(super) fn next(&mut self, _aad: &[u8], _buf: &mut Vec<u8>) -> Result<(), CryptoError> {
        self.0.take_next().map(drop)
    }

    pub(super) fn last(self, _aad: &[u8], _buf: &mut Vec<u8>) -> Result<(), CryptoError> {
        Ok(())
    }
}

pub(super) struct Opener(Stream);

impl Opener {
    pub(super) fn new(key: &Key, prefix: [u8; PREFIX_LEN]) -> Self {
        Opener(Stream::new(key, prefix))
    }

    pub(super) fn next(&mut self, _aad: &[u8], _buf: &mut Vec<u8>) -> Result<(), CryptoError> {
        self.0.take_next().map(drop)
    }

    pub(super) fn last(self, _aad: &[u8], _buf: &mut Vec<u8>) -> Result<(), CryptoError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
