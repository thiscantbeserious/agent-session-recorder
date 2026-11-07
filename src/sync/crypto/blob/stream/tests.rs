use super::{Opener, Sealer, Stream, PREFIX_LEN};
use crate::sync::crypto::{CryptoError, Key};
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::XNonce;

const AAD: &[u8] = b"agr:test";

fn key() -> Key {
    Key::from_bytes([5; 32])
}

fn prefix() -> [u8; PREFIX_LEN] {
    core::array::from_fn(|i| 0x40 + i as u8)
}

fn nonce(ctr: u32, flag: u8) -> [u8; 24] {
    [&prefix()[..], &ctr.to_be_bytes(), &[flag]]
        .concat()
        .try_into()
        .unwrap()
}

/// Frame sealed by hand under a hand-built nonce, no stream layer involved.
fn hand(ctr: u32, flag: u8, msg: &[u8]) -> Vec<u8> {
    let n = XNonce::from(nonce(ctr, flag));
    key()
        .cipher()
        .encrypt(&n, Payload { msg, aad: AAD })
        .unwrap()
}

fn unhand(ctr: u32, flag: u8, frame: &[u8]) -> Option<Vec<u8>> {
    let n = XNonce::from(nonce(ctr, flag));
    let pay = Payload {
        msg: frame,
        aad: AAD,
    };
    key().cipher().decrypt(&n, pay).ok()
}

#[test]
fn nonce_layout_is_prefix_counter_be_and_flag() {
    for ctr in [0, 1, 0x0102_0304, u32::MAX] {
        for flag in [0u8, 1] {
            let s = Stream::at(&key(), prefix(), ctr);
            assert_eq!(
                &s.nonce(flag == 1)[..],
                &nonce(ctr, flag)[..],
                "{ctr} {flag}"
            );
        }
    }
}

#[test]
fn next_is_refused_at_u32_max_and_last_still_works() {
    let mut s = Sealer(Stream::at(&key(), prefix(), u32::MAX - 1));
    let mut ok = b"a".to_vec();
    s.next(AAD, &mut ok).unwrap();
    assert_eq!(ok, hand(u32::MAX - 1, 0, b"a"));
    let mut refused = b"b".to_vec();
    assert_eq!(s.next(AAD, &mut refused), Err(CryptoError::TooLong));
    assert_eq!(refused, b"b", "refused frame was touched");
    let mut last = b"c".to_vec();
    s.last(AAD, &mut last).unwrap();
    assert_eq!(last, hand(u32::MAX, 1, b"c"));

    let mut o = Opener(Stream::at(&key(), prefix(), u32::MAX - 1));
    let mut ok = hand(u32::MAX - 1, 0, b"a");
    o.next(AAD, &mut ok).unwrap();
    assert_eq!(ok, b"a");
    let mut refused = hand(u32::MAX, 0, b"b");
    let before = refused.clone();
    assert_eq!(o.next(AAD, &mut refused), Err(CryptoError::TooLong));
    assert_eq!(refused, before, "refused frame was touched");
    let mut last = hand(u32::MAX, 1, b"c");
    o.last(AAD, &mut last).unwrap();
    assert_eq!(last, b"c");
}

#[test]
fn counter_moves_before_the_aead_and_never_repeats() {
    let mut s = Sealer::new(&key(), prefix());
    let mut frames = Vec::new();
    for i in 0..4u32 {
        let mut buf = vec![i as u8; 3];
        s.next(AAD, &mut buf).unwrap();
        assert_eq!(s.0.counter, i + 1, "counter after frame {i}");
        frames.push(buf);
    }
    let mut buf = vec![4u8; 3];
    s.last(AAD, &mut buf).unwrap();
    frames.push(buf);
    for (i, f) in frames.iter().enumerate() {
        let flag = u8::from(i == 4);
        assert_eq!(
            unhand(i as u32, flag, f),
            Some(vec![i as u8; 3]),
            "frame {i}"
        );
    }
}

#[test]
fn failed_open_clears_the_buffer() {
    let mut bad = hand(0, 0, b"secret");
    bad[0] ^= 1;
    let mut buf = bad.clone();
    let mut o = Opener::new(&key(), prefix());
    assert_eq!(o.next(AAD, &mut buf), Err(CryptoError::Authentication));
    assert!(buf.is_empty(), "next left {} bytes", buf.len());
    let mut buf = bad;
    assert_eq!(
        Opener::new(&key(), prefix()).last(AAD, &mut buf),
        Err(CryptoError::Authentication)
    );
    assert!(buf.is_empty(), "last left {} bytes", buf.len());
}
