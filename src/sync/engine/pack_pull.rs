//! Pull and refresh of one session against the merged pack view.

use std::path::Path;

use super::index::{record_synced, stamp, Index};
use super::plan::{Local, Remote};
use super::scan::Row;
use crate::files::backup;
use crate::sync::pack::repo::{ReadError, Repo};
use crate::sync::pack::stream::RangeError;
use crate::sync::store::StoreError;
use crate::StorageManager;

const OLDER: &str = "remote version is older than the last sync, not pulling";

/// Why one pull did not finish.
pub(super) enum PullFail {
    /// The store stopped us, the run ends here.
    Stop,
    /// The merged gen is below the last synced one, nothing was touched.
    Refused,
    Failed(String),
}

impl PullFail {
    pub(super) fn text(&self) -> String {
        match self {
            Self::Refused => OLDER.to_string(),
            Self::Failed(why) => why.clone(),
            Self::Stop => "rate limit reached".to_string(),
        }
    }
}

fn read_fail(e: ReadError) -> PullFail {
    match e {
        ReadError::Store(StoreError::RateLimited)
        | ReadError::Range(RangeError::Store(StoreError::RateLimited)) => PullFail::Stop,
        other => PullFail::Failed(other.to_string()),
    }
}

/// Opens the session into a hidden temp next to its file, rotates the `.bak` and renames the
/// temp over the file, only after hash and size matched. A merged gen below the index is refused.
pub(super) fn pull_one(
    (repo, storage): (&Repo<'_>, &StorageManager),
    row: &Row,
    index: &mut Index,
) -> Result<u64, PullFail> {
    let known = index.entries.get(&row.path).map_or(0, |e| e.gen);
    if row.gen < known {
        return Err(PullFail::Refused);
    }
    let Remote::Hash(sha256) = &row.input.remote else {
        return Err(PullFail::Failed("remote entry is missing".into()));
    };
    let dir = storage.storage_dir();
    let target = dir.join(&row.path);
    place(repo, &row.path, &target)?;
    record_synced(index, &dir, &row.path, (sha256, row.gen));
    Ok(std::fs::metadata(&target).map_or(0, |m| m.len()))
}

fn place(repo: &Repo<'_>, path: &str, target: &Path) -> Result<(), PullFail> {
    let fail = |e: std::io::Error| PullFail::Failed(e.to_string());
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(fail)?;
    }
    let temp = crate::files::hidden_auxiliary_path(target, ".pull");
    let opened = repo.read_session(path, &temp).map_err(read_fail);
    let moved = opened.and_then(|()| {
        rotate_backup(target).map_err(PullFail::Failed)?;
        std::fs::rename(&temp, target).map_err(fail)
    });
    if moved.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    moved
}

/// A hidden `.bak` is replaced by the current file, the local file is the newer state worth keeping.
fn rotate_backup(target: &Path) -> Result<(), String> {
    if !target.exists() {
        return Ok(());
    }
    let bak = backup::backup_path_for(target);
    std::fs::copy(target, &bak)
        .map(|_| ())
        .map_err(|e| format!("backup {}: {e}", bak.display()))
}

/// Hashes agree and the index lacks the entry, has a stale stamp, or the remote tombstone
/// outlived the local file. `true` when the index changed.
pub(super) fn refresh(storage: &StorageManager, row: &Row, index: &mut Index) -> bool {
    let dir = storage.storage_dir();
    match (&row.input.local, &row.input.remote) {
        (Local::Hash(hash), Remote::Hash(_)) => {
            // a rolled back remote never lowers the high-water mark
            let known = index.entries.get(&row.path).map_or(0, |e| e.gen);
            record_synced(index, &dir, &row.path, (hash, row.gen.max(known)));
            true
        }
        (Local::Absent, Remote::Deleted { .. }) => index.entries.remove(&row.path).is_some(),
        _ => false,
    }
}

/// Whether an in-sync row still needs its index entry written or dropped.
pub(super) fn needs_refresh(storage: &StorageManager, row: &Row, index: &Index) -> bool {
    let entry = index.entries.get(&row.path);
    match (&row.input.local, &row.input.remote, entry) {
        (Local::Absent, Remote::Deleted { .. }, entry) => entry.is_some(),
        (_, _, Some(e)) => stamp(&storage.storage_dir().join(&row.path)) != (e.size, e.mtime_ns),
        _ => false,
    }
}
