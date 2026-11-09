//! Gist pack store: one secret gist per pack, `index` plus `data.NNN` parts of at most 8 MiB.

#[cfg_attr(not(test), allow(dead_code))]
mod layout;
