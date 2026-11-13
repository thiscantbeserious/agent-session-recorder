//! Whole-run failures from the store.

use super::run::SyncError;
use crate::sync::pack::StoreError;

/// A store failure that stops the whole run.
pub(super) fn sync_error(e: StoreError) -> SyncError {
    SyncError::Backend(e)
}
