//! Pack layer: pack index, seal rules and merge (more follows in later stages).

pub(in crate::sync) mod index;
pub(in crate::sync) mod merge;
pub(in crate::sync) mod migrate;
pub(in crate::sync) mod repo;
pub(in crate::sync) mod rules;
pub(in crate::sync) mod stream;
