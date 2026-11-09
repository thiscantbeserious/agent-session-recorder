//! Streamed blob v2: `AGR1 || 0x02 || nonce prefix [19] || frames`, STREAM (BE32) over
//! XChaCha20-Poly1305 in 64 KiB frames, see ADR D9.

use super::{CryptoError, Key};
use chacha20poly1305::aead::Generate;
use data_encoding::HEXLOWER;
use sha2::{Digest as _, Sha256};
use std::io::{self, BufRead, BufReader, Read, Write};
use stream::{Opener, Sealer};
use thiserror::Error;

mod stream;
use stream::{PREFIX_LEN, TAG_LEN};

const MAGIC: &[u8] = b"AGR1";
const VERSION: u8 = 2;
const HEADER_LEN: usize = MAGIC.len() + 1 + PREFIX_LEN;
const FRAME: usize = 64 * 1024;
const AAD_PREFIX: &[u8] = b"agr:blob:";

/// Errors from sealing or opening a streamed blob.
#[derive(Debug, Error)]
pub enum BlobError {
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error("blob io failed: {0}")]
    Io(#[from] io::Error),
}

/// Plaintext SHA-256 (hex) and size seen by the sealer or opener.
pub type Digest = (String, u64);

/// Exact byte count of a sealed blob for `size` plain bytes: header, one tag per frame, an
/// empty input still has one (last) frame.
pub fn sealed_len(size: u64) -> u64 {
    let frames = size.div_ceil(FRAME as u64).max(1);
    HEADER_LEN as u64 + size + frames * TAG_LEN as u64
}

fn aad(path: &str, gen: u64) -> Vec<u8> {
    let tail = format!(":{gen}");
    [
        MAGIC,
        &[VERSION],
        AAD_PREFIX,
        path.as_bytes(),
        tail.as_bytes(),
    ]
    .concat()
}

/// Reads up to one frame into `buf`, true when no more input follows (look-ahead).
fn read_frame<R: Read>(src: &mut BufReader<R>, buf: &mut Vec<u8>, max: usize) -> io::Result<bool> {
    buf.clear();
    src.by_ref().take(max as u64).read_to_end(buf)?;
    loop {
        match src.fill_buf() {
            Ok(rest) => return Ok(rest.is_empty()),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

/// Seals `src` into `dst`, one frame in memory at a time, returns plaintext hash and size.
pub fn seal_stream(
    key: &Key,
    path: &str,
    gen: u64,
    src: impl Read,
    dst: impl Write,
) -> Result<Digest, BlobError> {
    let prefix = <[u8; PREFIX_LEN]>::generate();
    seal_with_prefix(key, path, gen, &prefix, src, dst)
}

/// `last` consumes the sealer, so the Option makes a second last frame impossible.
fn seal_frame(
    enc: &mut Option<Sealer>,
    last: bool,
    aad: &[u8],
    buf: &mut Vec<u8>,
) -> Result<(), CryptoError> {
    if last {
        enc.take().expect("one last frame").last(aad, buf)
    } else {
        enc.as_mut().expect("not yet last").next(aad, buf)
    }
}

/// Mirror of `seal_frame`.
fn open_frame(
    dec: &mut Option<Opener>,
    last: bool,
    aad: &[u8],
    buf: &mut Vec<u8>,
) -> Result<(), CryptoError> {
    if last {
        dec.take().expect("one last frame").last(aad, buf)
    } else {
        dec.as_mut().expect("not yet last").next(aad, buf)
    }
}

fn seal_with_prefix(
    key: &Key,
    path: &str,
    gen: u64,
    prefix: &[u8; PREFIX_LEN],
    src: impl Read,
    mut dst: impl Write,
) -> Result<Digest, BlobError> {
    dst.write_all(&[MAGIC, &[VERSION], prefix].concat())?;
    let mut enc = Some(Sealer::new(key, *prefix));
    let (aad, mut src) = (aad(path, gen), BufReader::new(src));
    let (mut hasher, mut size, mut buf) =
        (Sha256::new(), 0u64, Vec::with_capacity(FRAME + TAG_LEN));
    loop {
        let last = read_frame(&mut src, &mut buf, FRAME)?;
        hasher.update(&buf);
        size += buf.len() as u64;
        seal_frame(&mut enc, last, &aad, &mut buf)?;
        dst.write_all(&buf)?;
        if last {
            return Ok((HEXLOWER.encode(&hasher.finalize()), size));
        }
    }
}

/// Checks length, magic and version, returns the nonce prefix.
fn read_header(src: &mut impl Read) -> Result<[u8; PREFIX_LEN], BlobError> {
    let mut head = Vec::with_capacity(HEADER_LEN);
    src.take(HEADER_LEN as u64).read_to_end(&mut head)?;
    if head.len() < HEADER_LEN {
        return Err(CryptoError::Truncated.into());
    }
    if &head[..4] != MAGIC {
        return Err(CryptoError::BadMagic.into());
    }
    if head[4] != VERSION {
        return Err(CryptoError::UnsupportedVersion(head[4]).into());
    }
    Ok(head[5..].try_into().expect("header is HEADER_LEN"))
}

/// Opens a sealed blob from `src` into `dst`, a frame is written only after it verified.
pub fn open_stream(
    key: &Key,
    path: &str,
    gen: u64,
    src: impl Read,
    mut dst: impl Write,
) -> Result<Digest, BlobError> {
    let mut src = BufReader::new(src);
    let prefix = read_header(&mut src)?;
    let mut dec = Some(Opener::new(key, prefix));
    let aad = aad(path, gen);
    let (mut hasher, mut size, mut buf) =
        (Sha256::new(), 0u64, Vec::with_capacity(FRAME + TAG_LEN));
    loop {
        let last = read_frame(&mut src, &mut buf, FRAME + TAG_LEN)?;
        if buf.len() < TAG_LEN {
            return Err(CryptoError::Truncated.into());
        }
        open_frame(&mut dec, last, &aad, &mut buf)?;
        hasher.update(&buf);
        size += buf.len() as u64;
        dst.write_all(&buf)?;
        if last {
            return Ok((HEXLOWER.encode(&hasher.finalize()), size));
        }
    }
}

/// Whole-blob shorthand for tests, never on the production path.
#[cfg(test)]
pub fn seal_blob(key: &Key, path: &str, gen: u64, plain: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    seal_stream(key, path, gen, plain, &mut out).expect("sealing into a Vec cannot fail");
    out
}

#[cfg(test)]
pub fn open_blob(key: &Key, path: &str, gen: u64, blob: &[u8]) -> Result<Vec<u8>, BlobError> {
    let mut out = Vec::new();
    open_stream(key, path, gen, blob, &mut out)?;
    Ok(out)
}

#[cfg(test)]
mod kat_tests;
#[cfg(test)]
mod tests;
