//! Streamed blob v2: `AGR1 || 0x02 || nonce prefix [19] || frames`, STREAM (BE32) over
//! XChaCha20-Poly1305 in 64 KiB frames, see ADR D9.

use super::crypto::{CryptoError, Key};
use chacha20poly1305::aead::rand_core::RngCore;
use chacha20poly1305::aead::stream::{DecryptorBE32, EncryptorBE32};
use chacha20poly1305::aead::OsRng;
use data_encoding::HEXLOWER;
use sha2::{Digest as _, Sha256};
use std::io::{self, BufRead, BufReader, Read, Write};
use thiserror::Error;

const MAGIC: &[u8] = b"AGR1";
const VERSION: u8 = 2;
const PREFIX_LEN: usize = 19;
const HEADER_LEN: usize = MAGIC.len() + 1 + PREFIX_LEN;
const TAG_LEN: usize = 16;
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
    Ok(src.fill_buf()?.is_empty())
}

/// Seals `src` into `dst`, one frame in memory at a time, returns plaintext hash and size.
pub fn seal_stream(
    key: &Key,
    path: &str,
    gen: u64,
    src: impl Read,
    dst: impl Write,
) -> Result<Digest, BlobError> {
    let mut prefix = [0u8; PREFIX_LEN];
    OsRng.fill_bytes(&mut prefix);
    seal_with_prefix(key, path, gen, &prefix, src, dst)
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
    let mut enc = EncryptorBE32::from_aead(key.cipher(), prefix.into());
    let (aad, mut src) = (aad(path, gen), BufReader::new(src));
    let (mut hasher, mut size, mut buf) =
        (Sha256::new(), 0u64, Vec::with_capacity(FRAME + TAG_LEN));
    loop {
        let last = read_frame(&mut src, &mut buf, FRAME)?;
        hasher.update(&buf);
        size += buf.len() as u64;
        if last {
            enc.encrypt_last_in_place(&aad, &mut buf)
                .map_err(|_| CryptoError::Authentication)?;
            dst.write_all(&buf)?;
            return Ok((HEXLOWER.encode(&hasher.finalize()), size));
        }
        enc.encrypt_next_in_place(&aad, &mut buf)
            .map_err(|_| CryptoError::Authentication)?;
        dst.write_all(&buf)?;
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
    let mut dec = DecryptorBE32::from_aead(key.cipher(), (&prefix).into());
    let aad = aad(path, gen);
    let (mut hasher, mut size, mut buf) =
        (Sha256::new(), 0u64, Vec::with_capacity(FRAME + TAG_LEN));
    loop {
        let last = read_frame(&mut src, &mut buf, FRAME + TAG_LEN)?;
        if buf.len() < TAG_LEN {
            return Err(CryptoError::Truncated.into());
        }
        if last {
            dec.decrypt_last_in_place(&aad, &mut buf)
                .map_err(|_| CryptoError::Authentication)?;
            hasher.update(&buf);
            size += buf.len() as u64;
            dst.write_all(&buf)?;
            return Ok((HEXLOWER.encode(&hasher.finalize()), size));
        }
        dec.decrypt_next_in_place(&aad, &mut buf)
            .map_err(|_| CryptoError::Authentication)?;
        hasher.update(&buf);
        size += buf.len() as u64;
        dst.write_all(&buf)?;
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
mod tests {
    use super::*;
    use crate::sync::crypto::sha256_hex;

    const PATH: &str = "claude/x.cast.gz";
    const FR: usize = FRAME + TAG_LEN;
    const SIZES: [usize; 8] = [
        0,
        1,
        65535,
        65536,
        65537,
        3 * 65536,
        3 * 65536 + 5,
        4 * 65536,
    ];
    /// 3 full frames plus 5 bytes: four frames, the last one short.
    const FOUR: usize = 3 * 65536 + 5;

    fn key(b: u8) -> Key {
        Key::from_bytes([b; 32])
    }

    fn plain(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i * 31 + 7) as u8).collect()
    }

    fn blob(n: usize) -> Vec<u8> {
        seal_blob(&key(1), PATH, 3, &plain(n))
    }

    fn open_raw(k: &Key, path: &str, gen: u64, b: &[u8]) -> (Result<Digest, BlobError>, Vec<u8>) {
        let mut out = Vec::new();
        let res = open_stream(k, path, gen, b, &mut out);
        (res, out)
    }

    fn crypto_err(r: Result<Digest, BlobError>) -> Option<CryptoError> {
        match r {
            Err(BlobError::Crypto(e)) => Some(e),
            _ => None,
        }
    }

    /// Splits a blob into its header and its frames.
    fn split(b: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
        let frames = b[HEADER_LEN..].chunks(FR).map(<[u8]>::to_vec).collect();
        (b[..HEADER_LEN].to_vec(), frames)
    }

    fn rebuild(head: &[u8], frames: &[Vec<u8>]) -> Vec<u8> {
        [head, &frames.concat()].concat()
    }

    /// Asserts a rejection and that the writer got only whole, correct, earlier frames.
    fn assert_rejected(name: &str, b: &[u8], bad_from: usize) {
        let (res, out) = open_raw(&key(1), PATH, 3, b);
        assert!(res.is_err(), "{name} was accepted");
        assert_eq!(out.len() % FRAME, 0, "{name} wrote a partial frame");
        assert!(
            out.len() <= bad_from * FRAME,
            "{name} wrote past frame {bad_from}"
        );
        assert_eq!(out, plain(FOUR)[..out.len()], "{name} wrote wrong bytes");
    }

    #[test]
    fn round_trips_and_has_exact_length() {
        for n in SIZES {
            let sealed = blob(n);
            let frames = n.div_ceil(FRAME).max(1);
            assert_eq!(sealed.len(), HEADER_LEN + frames * TAG_LEN + n, "len {n}");
            assert_eq!(&sealed[..5], b"AGR1\x02");
            assert_eq!(
                open_blob(&key(1), PATH, 3, &sealed).ok(),
                Some(plain(n)),
                "n {n}"
            );
        }
        assert_eq!(blob(0).len(), 5 + 19 + 16);
    }

    #[test]
    fn seal_and_open_return_hash_and_size() {
        for n in SIZES {
            let want = (sha256_hex(&plain(n)), n as u64);
            let mut sealed = Vec::new();
            let got = seal_stream(&key(1), PATH, 3, plain(n).as_slice(), &mut sealed).ok();
            assert_eq!(got, Some(want.clone()), "seal {n}");
            assert_eq!(
                open_raw(&key(1), PATH, 3, &sealed).0.ok(),
                Some(want),
                "open {n}"
            );
        }
    }

    #[test]
    fn two_seals_differ() {
        assert_ne!(blob(10), blob(10));
    }

    #[test]
    fn truncation_at_every_frame_boundary_and_inside_a_frame_is_rejected() {
        let b = blob(FOUR);
        let (head, frames) = split(&b);
        assert_eq!(frames.len(), 4);
        for k in 0..4 {
            let at_boundary = rebuild(&head, &frames[..k]);
            assert_rejected(&format!("cut at boundary {k}"), &at_boundary, k.max(1));
            for cut in [1, 15, 16, frames[k].len() / 2, frames[k].len() - 1] {
                let inside = [at_boundary.clone(), frames[k][..cut].to_vec()].concat();
                assert_rejected(&format!("cut {cut} into frame {k}"), &inside, k + 1);
            }
        }
    }

    #[test]
    fn dropping_swapping_and_duplicating_frames_is_rejected() {
        let b = blob(FOUR);
        let (head, frames) = split(&b);
        let pick = |idx: &[usize]| {
            rebuild(
                &head,
                &idx.iter().map(|&i| frames[i].clone()).collect::<Vec<_>>(),
            )
        };
        assert_rejected("drop frame 1", &pick(&[0, 2, 3]), 1);
        assert_rejected("drop last", &pick(&[0, 1, 2]), 3);
        assert_rejected("drop first", &pick(&[1, 2, 3]), 0);
        assert_rejected("swap 1 and 2", &pick(&[0, 2, 1, 3]), 1);
        assert_rejected("duplicate 1", &pick(&[0, 1, 1, 2, 3]), 2);
        assert_rejected("append copy of last", &pick(&[0, 1, 2, 3, 3]), 3);
    }

    #[test]
    fn appended_bytes_or_frames_after_the_last_frame_are_rejected() {
        let b = blob(FOUR);
        let (head, frames) = split(&b);
        assert_rejected("append one byte", &[b.clone(), vec![0]].concat(), 3);
        assert_rejected(
            "append 16 zero bytes",
            &[b.clone(), vec![0; 16]].concat(),
            3,
        );
        let extra = blob(FOUR);
        let foreign = split(&extra).1[3].clone();
        assert_rejected(
            "append foreign frame",
            &rebuild(&head, &[frames.clone(), vec![foreign]].concat()),
            3,
        );
    }

    #[test]
    fn a_full_final_frame_without_the_last_flag_plus_appended_data_is_rejected() {
        let b = blob(4 * FRAME);
        let (head, frames) = split(&b);
        assert_eq!(frames.len(), 4);
        assert!(frames.iter().all(|f| f.len() == FR));
        // Frames 0..=2 of a 4 frame blob end in a flag 0 frame, splice a flag 1 tail after it.
        let tail = split(&blob(FRAME + 1)).1;
        assert_eq!(tail[0].len(), FR);
        let forged = rebuild(
            &head,
            &[
                frames[..3].to_vec(),
                vec![frames[3].clone(), tail[1].clone()],
            ]
            .concat(),
        );
        assert_rejected("full last frame plus appended frame", &forged, 3);
        let one_byte = [frames[..3].concat(), frames[3].clone(), vec![9]].concat();
        assert_rejected(
            "full last frame plus one byte",
            &[head.clone(), one_byte].concat(),
            3,
        );
    }

    #[test]
    fn full_frame_sealed_as_next_cannot_pass_as_last_at_a_boundary() {
        let b = blob(FRAME + 1);
        let (head, frames) = split(&b);
        // Frame 0 of this blob is a full flag 0 frame, offered alone it is not the end.
        let (res, out) = open_raw(&key(1), PATH, 3, &rebuild(&head, &frames[..1]));
        assert_eq!(crypto_err(res), Some(CryptoError::Authentication));
        assert!(out.is_empty());
    }

    #[test]
    fn wrong_key_path_and_gen_are_rejected_without_output() {
        let b = blob(FOUR);
        for (k, p, g) in [
            (key(2), PATH, 3),
            (key(1), "claude/y.cast.gz", 3),
            (key(1), PATH, 2),
        ] {
            let (res, out) = open_raw(&k, p, g, &b);
            assert_eq!(crypto_err(res), Some(CryptoError::Authentication));
            assert!(out.is_empty());
        }
    }

    #[test]
    fn v1_bad_magic_and_short_input_are_rejected_by_kind() {
        let mut b = blob(10);
        b[4] = 1;
        let err = open_raw(&key(1), PATH, 3, &b).0.unwrap_err();
        assert_eq!(err.to_string(), "unsupported blob version 1");
        assert_eq!(
            crypto_err(Err(err)),
            Some(CryptoError::UnsupportedVersion(1))
        );
        let mut b = blob(10);
        b[0] = b'X';
        assert_eq!(
            crypto_err(open_raw(&key(1), PATH, 3, &b).0),
            Some(CryptoError::BadMagic)
        );
        let b = blob(10);
        for len in [0, 4, 5, 23] {
            let res = open_raw(&key(1), PATH, 3, &b[..len]).0;
            assert_eq!(crypto_err(res), Some(CryptoError::Truncated), "len {len}");
        }
        let res = open_raw(&key(1), PATH, 3, &b[..HEADER_LEN + 15]).0;
        assert_eq!(crypto_err(res), Some(CryptoError::Truncated));
    }

    #[test]
    fn every_header_and_frame_bit_flip_is_rejected() {
        let b = blob(FOUR);
        let last = b.len() - 1;
        let spots = [
            0,
            3,
            4,
            5,
            12,
            23,
            HEADER_LEN + 1,
            HEADER_LEN + FR + 100,
            3 * FR + HEADER_LEN,
            last - 16,
            last,
        ];
        for i in spots {
            for bit in [0u8, 7] {
                let mut t = b.clone();
                t[i] ^= 1 << bit;
                assert_rejected(&format!("flip byte {i} bit {bit}"), &t, 4);
            }
        }
    }

    #[test]
    fn frame_nonce_is_prefix_counter_and_last_flag() {
        use chacha20poly1305::aead::{Aead, KeyInit, Payload};
        let k = Key::from_bytes([5; 32]);
        let prefix = [9u8; PREFIX_LEN];
        let mut out = Vec::new();
        seal_with_prefix(&k, PATH, 1, &prefix, plain(FRAME + 1).as_slice(), &mut out).unwrap();
        let cipher = chacha20poly1305::XChaCha20Poly1305::new(k.as_bytes().into());
        let nonce = |ctr: u32, flag: u8| [&prefix[..], &ctr.to_be_bytes(), &[flag]].concat();
        let (n0, n1) = (nonce(0, 0), nonce(1, 1));
        let aad = aad(PATH, 1);
        let pay = |msg| Payload { msg, aad: &aad };
        let all = plain(FRAME + 1);
        let f0 = cipher
            .encrypt(n0.as_slice().into(), pay(&all[..FRAME]))
            .unwrap();
        let f1 = cipher
            .encrypt(n1.as_slice().into(), pay(&all[FRAME..]))
            .unwrap();
        assert_eq!(out[HEADER_LEN..], [f0, f1].concat());
    }

    /// Raw frame sealed by hand: `prefix || ctr BE || flag` nonce, same AAD.
    fn raw_frame(k: &Key, prefix: &[u8; PREFIX_LEN], ctr: u32, flag: u8, msg: &[u8]) -> Vec<u8> {
        use chacha20poly1305::aead::{Aead, KeyInit, Payload};
        let nonce = [&prefix[..], &ctr.to_be_bytes(), &[flag]].concat();
        let aad = aad(PATH, 3);
        let cipher = chacha20poly1305::XChaCha20Poly1305::new(k.as_bytes().into());
        cipher
            .encrypt(nonce.as_slice().into(), Payload { msg, aad: &aad })
            .unwrap()
    }

    #[test]
    fn valid_last_frame_followed_by_a_valid_frame_is_rejected_by_lookahead_alone() {
        let prefix = [4u8; PREFIX_LEN];
        let k = key(1);
        let mut one = Vec::new();
        seal_with_prefix(&k, PATH, 3, &prefix, plain(FRAME).as_slice(), &mut one).unwrap();
        assert_eq!(
            open_raw(&k, PATH, 3, &one).0.ok().map(|d| d.1),
            Some(FRAME as u64)
        );
        // Same key, path, gen and prefix, a genuine last-flag frame at the next counter.
        let extra = raw_frame(&k, &prefix, 1, 1, b"tail");
        let (res, out) = open_raw(&k, PATH, 3, &[one, extra].concat());
        assert_eq!(crypto_err(res), Some(CryptoError::Authentication));
        assert!(
            out.is_empty(),
            "frame 0 was written although more data follows"
        );
    }

    #[test]
    fn frames_sealed_without_the_header_in_the_aad_are_rejected() {
        use chacha20poly1305::aead::stream::EncryptorBE32;
        let k = key(1);
        let prefix = [6u8; PREFIX_LEN];
        let bare = format!("agr:blob:{PATH}:3");
        let enc = EncryptorBE32::from_aead(k.cipher(), (&prefix).into());
        let frame = enc.encrypt_last(chacha20poly1305::aead::Payload {
            msg: b"x",
            aad: bare.as_bytes(),
        });
        let blob = [b"AGR1\x02".as_slice(), &prefix, &frame.unwrap()].concat();
        assert_eq!(
            crypto_err(open_raw(&k, PATH, 3, &blob).0),
            Some(CryptoError::Authentication)
        );
    }

    #[test]
    fn known_answer_for_fixed_key_prefix_path_and_gen() {
        let k = Key::from_bytes(core::array::from_fn(|i| i as u8));
        let prefix: [u8; PREFIX_LEN] = core::array::from_fn(|i| 0x40 + i as u8);
        let mut out = Vec::new();
        seal_with_prefix(
            &k,
            "a/b.cast.gz",
            7,
            &prefix,
            &b"known answer"[..],
            &mut out,
        )
        .unwrap();
        assert_eq!(hex(&out), KAT_SMALL);
        let mut two = Vec::new();
        seal_with_prefix(
            &k,
            "a/b.cast.gz",
            7,
            &prefix,
            plain(FRAME + 1).as_slice(),
            &mut two,
        )
        .unwrap();
        assert_eq!(sha256_hex(&two), KAT_TWO_FRAMES_SHA256);
    }

    fn hex(b: &[u8]) -> String {
        data_encoding::HEXLOWER.encode(b)
    }

    const KAT_SMALL: &str = "4147523102404142434445464748494a4b4c4d4e4f5051520fcdef0aba5292f8959407780543edc8c71e885971e5e03bbe10372e";
    const KAT_TWO_FRAMES_SHA256: &str =
        "28cb7cd33a57e94b9fc001c82f323a10cbbfb2722b9e276f55e1b9b937bed85a";
}
