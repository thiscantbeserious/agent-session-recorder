//! Pure seal rules, no store and no IO.

use thiserror::Error;

const MIB: u64 = 1024 * 1024;
const MIN_MIB: u64 = 16;
const MAX_MIB: u64 = 512;
/// The largest target a repo accepts.
pub const MAX_TARGET: u64 = MAX_MIB * MIB;
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

/// `agent/name.cast.gz`, nothing that can escape the storage dir or hit hidden files.
pub fn valid_path(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    parts.len() == 2
        && parts
            .iter()
            .all(|p| !p.is_empty() && !p.starts_with('.') && !p.contains(['\\', ':']))
        && parts[1].ends_with(".cast.gz")
}
