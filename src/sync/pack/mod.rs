//! Pack layer: pack index, seal rules and merge (more follows in later stages).

pub(in crate::sync) mod index;
#[allow(dead_code)] // used by Stage 51 Repo
pub(in crate::sync) mod merge;
#[cfg(test)]
mod merge_tests;
pub(in crate::sync) mod rules;
pub(in crate::sync) mod stream;
