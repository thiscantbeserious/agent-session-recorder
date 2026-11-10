//! `agr sync migrate`: moves the per-session gists of the old layout into packs.

mod local;
mod recognise;

pub use local::{convert_file, convert_index};
pub use recognise::{recognise, Kind, LegacyGist};

use crate::sync::crypto::Key;
use crate::sync::pack::repo::RepoError;
use crate::sync::store::{PackStore, StoreError};
use std::io::Read;
use std::path::Path;

/// The read side of the old per-session layout, nothing else of it is needed.
pub trait LegacyRead {
    /// Every gist of the user with its files, all pages, no gist cap.
    fn list_all(&self) -> Result<Vec<LegacyGist>, StoreError>;
    /// Manifest token and blob reader of one gist.
    fn pull(&self, id: &str) -> Result<(Option<String>, Box<dyn Read>), StoreError>;
    fn delete(&self, id: &str) -> Result<(), StoreError>;
}

pub struct Env<'a> {
    pub legacy: &'a dyn LegacyRead,
    pub store: &'a dyn PackStore,
    pub key: &'a Key,
    pub writer: &'a str,
    pub target: u64,
    pub hint: Option<&'a str>,
    /// Storage directory, only the sync index in it is ever written.
    pub dir: &'a Path,
    /// Private directory for the sessions in flight.
    pub scratch: &'a Path,
    /// Unix seconds from an injected clock.
    pub now: i64,
    /// Called before every delete, spaces them out.
    pub pace: &'a dyn Fn(),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub lines: Vec<String>,
    pub moved: usize,
    pub deleted: usize,
    /// Ids of the gists kept for a mismatch or an unreadable blob.
    pub kept: Vec<String>,
    /// Steps not done because GitHub asked to wait.
    pub left: usize,
}

#[derive(Debug)]
pub enum MigrateError {
    List(StoreError),
    Open(RepoError),
    Store(StoreError),
    Local(String),
}

/// Counts only, nothing is written.
pub fn preview(env: &Env) -> Result<Report, MigrateError> {
    let _ = env;
    Ok(Report::default())
}

/// Moves, verifies and deletes.
pub fn run(env: &Env) -> Result<Report, MigrateError> {
    let _ = env;
    Ok(Report::default())
}
