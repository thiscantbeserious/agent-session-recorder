//! Pure seal rules, no store and no IO.

use thiserror::Error;

const MIB: u64 = 1024 * 1024;
const MIN_MIB: u64 = 16;
const MAX_MIB: u64 = 512;
/// Every append rewrites the index and git keeps every version, so one index stays small.
const INDEX_CAP: usize = 768 * 1024;

/// Where one session goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    OpenPack,
    OwnPack,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("pack_size_mib must be between {MIN_MIB} and {MAX_MIB}, got {0}")]
pub struct PackSizeError(pub u64);

/// A pack is sealed by the append that brings it to at least the target.
pub fn should_seal(target: u64, len: u64) -> bool {
    len >= target
}

/// A session larger than the target gets its own pack.
pub fn place(target: u64, size: u64) -> Place {
    if size > target {
        Place::OwnPack
    } else {
        Place::OpenPack
    }
}

/// `encoded_len` is the length of the sealed index text, not of the JSON.
pub fn should_seal_index(encoded_len: usize) -> bool {
    encoded_len > INDEX_CAP
}

/// Target pack size in bytes from `[sync] pack_size_mib`.
pub fn pack_size(mib: u64) -> Result<u64, PackSizeError> {
    if (MIN_MIB..=MAX_MIB).contains(&mib) {
        Ok(mib * MIB)
    } else {
        Err(PackSizeError(mib))
    }
}

#[cfg(test)]
mod tests {
    use super::{pack_size, place, should_seal, should_seal_index, Place};

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
}
