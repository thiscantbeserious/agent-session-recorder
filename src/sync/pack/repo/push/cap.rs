//! The index cap: how many entries one append may add before its index text passes the cap.

use super::stage::{tomb_entry, Staged};
use crate::sync::crypto::Key;
use crate::sync::pack::index::{text_len, Entry, Index};
use crate::sync::pack::rules::should_seal_index;
use crate::sync::store::StoreError;

/// JSON bytes one entry adds, offset at its widest and a comma, so the bound is never below the real text.
fn cost(e: &Entry) -> usize {
    serde_json::to_vec(e).map_or(0, |j| j.len() + 1)
}

pub(super) fn session_costs(staged: &[Staged]) -> Vec<usize> {
    staged.iter().map(|s| cost(&s.entry(u64::MAX))).collect()
}

pub(super) fn tomb_costs(tombs: &[(String, u64)]) -> Vec<usize> {
    let one = |(path, gen): &(String, u64)| cost(&tomb_entry(path, *gen, u64::MAX));
    tombs.iter().map(one).collect()
}

/// How many of the first `costs` fit `idx` under the cap. Zero means the pack is full already.
pub(super) fn take_within(key: &Key, idx: &Index, costs: &[usize]) -> Result<usize, StoreError> {
    let base = text_len(key, idx).map_err(|e| StoreError::Other(e.to_string()))?;
    let mut sum = 0;
    for (n, c) in costs.iter().enumerate() {
        sum += c;
        if should_seal_index(base.with_extra(sum)) {
            return Ok(n);
        }
    }
    Ok(costs.len())
}
