//! The old per-session layout, read side only: `agr sync migrate` lists, pulls and deletes
//! these gists and nothing else touches them.

mod gist;
mod parse;

pub use gist::GistLegacy;

use std::io::Read;

use super::StoreError;

/// One gist as the old listing shows it, files as (name, size in bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyGist {
    pub id: String,
    pub description: String,
    pub files: Vec<(String, u64)>,
    pub created_at: String,
    /// GitHub cut the file list of this gist, files are missing from `files`.
    pub truncated: bool,
}

/// The read side of the old per-session layout, nothing else of it is needed.
pub trait LegacyRead {
    /// Every gist of the user with its files, all pages, no gist cap.
    fn list_all(&self) -> Result<Vec<LegacyGist>, StoreError>;
    /// Manifest token and blob reader of one gist.
    fn pull(&self, id: &str) -> Result<(Option<String>, Box<dyn Read>), StoreError>;
    fn delete(&self, id: &str) -> Result<(), StoreError>;
}

/// No old gists at all.
pub struct NoLegacy;

impl LegacyRead for NoLegacy {
    fn list_all(&self) -> Result<Vec<LegacyGist>, StoreError> {
        Ok(Vec::new())
    }
    fn pull(&self, _id: &str) -> Result<(Option<String>, Box<dyn Read>), StoreError> {
        Err(StoreError::NotFound)
    }
    fn delete(&self, _id: &str) -> Result<(), StoreError> {
        Err(StoreError::NotFound)
    }
}
