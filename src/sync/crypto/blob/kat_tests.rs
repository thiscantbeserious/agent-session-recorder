//! Known-answer vectors, computed outside this code (Python `cryptography` plus a
//! hand-written HChaCha20 checked against draft-irtf-cfrg-xchacha-03 2.2.1).

use super::{seal_with_prefix, Key, FRAME, PREFIX_LEN};
use crate::sync::crypto::sha256_hex;

fn plain(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 31 + 7) as u8).collect()
}

fn hex(b: &[u8]) -> String {
    data_encoding::HEXLOWER.encode(b)
}

const KAT_SMALL: &str = "4147523102404142434445464748494a4b4c4d4e4f5051520fcdef0aba5292f8959407780543edc8c71e885971e5e03bbe10372e";
const KAT_TWO_FRAMES_SHA256: &str =
    "28cb7cd33a57e94b9fc001c82f323a10cbbfb2722b9e276f55e1b9b937bed85a";

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

/// sha256 of a three-frame blob (2*FRAME+3 bytes): counters 0, 1 and 2, last flag on frame 2.
const KAT_THREE_FRAMES_SHA256: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

#[test]
fn known_answer_for_three_frames_pins_counter_two_and_the_last_flag() {
    let k = Key::from_bytes(core::array::from_fn(|i| i as u8));
    let prefix: [u8; PREFIX_LEN] = core::array::from_fn(|i| 0x40 + i as u8);
    let mut out = Vec::new();
    let src = plain(2 * FRAME + 3);
    seal_with_prefix(&k, "a/b.cast.gz", 7, &prefix, src.as_slice(), &mut out).unwrap();
    assert_eq!(sha256_hex(&out), KAT_THREE_FRAMES_SHA256);
}
