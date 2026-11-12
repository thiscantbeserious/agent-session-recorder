//! Pure prune plan over a merged view, no store and no IO.

use crate::sync::pack::merge::View;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub delete: Vec<String>,
    pub repack: Vec<String>,
    pub copy_tombstones: usize,
    pub move_bytes: u64,
    pub free_bytes: u64,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.delete.is_empty() && self.repack.is_empty()
    }

    pub fn touches(&self, id: &str) -> bool {
        self.delete.iter().chain(&self.repack).any(|p| p == id)
    }
}

pub fn plan(_view: &View) -> Plan {
    Plan::default()
}
