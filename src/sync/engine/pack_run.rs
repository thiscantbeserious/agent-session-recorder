//! Sync engine on a `Repo`, push, pull, delete and status against the merged pack view.

use super::observe::Observer;
use super::run::{Options, Report, SyncError};
use crate::sync::crypto::Key;
use crate::sync::store::PackStore;
use crate::StorageManager;

/// Engine over any pack store, the key, the storage dir and the writer id of this machine.
pub struct Engine<'a> {
    pub store: &'a dyn PackStore,
    pub key: &'a Key,
    pub storage: &'a StorageManager,
    pub writer: String,
    /// Target pack size in bytes.
    pub target: u64,
}

impl<'a> Engine<'a> {
    pub fn new(
        store: &'a dyn PackStore,
        key: &'a Key,
        storage: &'a StorageManager,
        writer: &str,
        target: u64,
    ) -> Self {
        Self {
            store,
            key,
            storage,
            writer: writer.to_string(),
            target,
        }
    }

    pub fn status(&self, _sessions: &[String]) -> Result<Report, SyncError> {
        Ok(Report::default())
    }

    pub fn status_with(
        &self,
        _sessions: &[String],
        _observer: &mut Observer<'_>,
    ) -> Result<Report, SyncError> {
        Ok(Report::default())
    }

    pub fn push(&self, _opts: &Options) -> Result<Report, SyncError> {
        Ok(Report::default())
    }

    pub fn pull(&self, _opts: &Options) -> Result<Report, SyncError> {
        Ok(Report::default())
    }

    pub fn push_with(
        &self,
        _opts: &Options,
        _total: usize,
        _observer: &mut Observer<'_>,
    ) -> Result<Report, SyncError> {
        Ok(Report::default())
    }

    pub fn pull_with(
        &self,
        _opts: &Options,
        _total: usize,
        _observer: &mut Observer<'_>,
    ) -> Result<Report, SyncError> {
        Ok(Report::default())
    }

    pub fn delete(&self, _sessions: &[String]) -> Result<Report, SyncError> {
        Ok(Report::default())
    }

    pub fn delete_with(
        &self,
        _sessions: &[String],
        _total: usize,
        _observer: &mut Observer<'_>,
    ) -> Result<Report, SyncError> {
        Ok(Report::default())
    }
}
