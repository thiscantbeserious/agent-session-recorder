//! Prune: frees the dead bytes of sealed packs.

mod plan;

pub use plan::{plan, Plan};

use crate::sync::crypto::Key;
use crate::sync::pack::repo::RepoError;
use crate::sync::store::{PackStore, StoreError};
use std::path::Path;

pub struct Env<'a> {
    pub store: &'a dyn PackStore,
    pub key: &'a Key,
    pub writer: &'a str,
    pub target: u64,
    pub hint: Option<&'a str>,
    /// Private directory for the sessions in flight.
    pub scratch: &'a Path,
    pub dry_run: bool,
    /// Called before every delete, spaces them out.
    pub pace: &'a dyn Fn(),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub lines: Vec<String>,
    pub deleted: usize,
    pub copied: usize,
    pub move_bytes: u64,
    pub free_bytes: u64,
    pub failed: Option<String>,
}

#[derive(Debug)]
pub enum PruneError {
    Open(RepoError),
    Store(StoreError),
    Local(String),
}

pub fn run(_env: &Env) -> Result<Report, PruneError> {
    Ok(Report::default())
}
