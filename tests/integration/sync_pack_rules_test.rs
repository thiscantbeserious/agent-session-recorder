//! Pack sealing and sizing rules.

use agr::sync::testing::{pack_size, place, should_seal, should_seal_index, Place};

const TARGET: u64 = 64 * 1024 * 1024;

#[test]
fn seal_at_target() {
    assert!(should_seal(TARGET, TARGET));
    assert!(!should_seal(TARGET, TARGET - 1));
}

#[test]
fn big_session_gets_own_pack() {
    assert_eq!(place(TARGET, TARGET + 1), Place::OwnPack);
    assert_eq!(place(TARGET, TARGET), Place::OpenPack);
}

#[test]
fn index_over_768_kib_seals_early() {
    assert!(should_seal_index(768 * 1024 + 1));
    assert!(!should_seal_index(768 * 1024));
}

#[test]
fn pack_size_mib_range_16_to_512() {
    assert!(pack_size(8).is_err());
    assert!(pack_size(1024).is_err());
    assert!(pack_size(16).is_ok());
    assert!(pack_size(15).is_err());
    assert!(pack_size(512).is_ok());
    assert!(pack_size(513).is_err());
}
