//! Builds plan inputs from the storage dir, the index and the remote list.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::Result;

use super::index::{stamp, Index};
use super::plan::{Input, Local, Remote, SkipReason};
use crate::files::{codec, lock};
use crate::storage::SessionInfo;
use crate::sync::pack::{sha256_reader, Meta};
use crate::StorageManager;

/// On-disk size cap per session, from GitHub repository size guidance.
pub(super) const MAX_BYTES: u64 = 1024 * 1024 * 1024;

/// One path with everything the engine needs to act on it.
#[derive(Debug, Clone)]
pub struct Row {
    /// `agent/name.cast.gz`, or `remote:<id>` for packs that cannot be opened.
    pub path: String,
    pub input: Input,
    pub meta: Option<Meta>,
    /// Generation of the last sync, from the index.
    pub gen: u64,
    /// Size and mtime taken before the local file was hashed, `(0, 0)` without a local file.
    pub stamp: (u64, u64),
}

pub(super) fn row(path: &str, local: Local, index: &Index) -> Row {
    Row {
        path: path.to_string(),
        input: Input {
            local,
            remote: Remote::Absent,
            last: index.entries.get(path).map(|e| e.sha256.clone()),
        },
        meta: None,
        gen: 0,
        stamp: (0, 0),
    }
}

/// One row per local session, `names` limits which ones are hashed. `progress(done, total)` runs
/// before the first and after every session.
pub(super) fn local_rows(
    storage: &StorageManager,
    index: &Index,
    names: Option<&[String]>,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<BTreeMap<String, Row>> {
    let wanted: Option<BTreeSet<String>> = names.map(|n| {
        let both = |name: &String| [wanted_path(storage, name), slashed(Path::new(name))];
        n.iter().flat_map(both).collect()
    });
    let mut rows = BTreeMap::new();
    let sessions = storage.list_sessions(None)?;
    let total = sessions.len();
    progress(0, total);
    for (done, session) in sessions.into_iter().enumerate() {
        let path = format!("{}/{}", session.agent, session.filename);
        // Rows that `select` drops are listed but never hashed.
        let hashed = wanted.as_ref().is_none_or(|w| w.contains(&path));
        // the stamp comes first, an edit during the hash then leaves a stale stamp behind
        let at = if hashed { stamp(&session.path) } else { (0, 0) };
        let local = if hashed {
            local_state(&session)
        } else {
            Local::Absent
        };
        let mut r = row(&path, local, index);
        r.stamp = at;
        rows.insert(path.clone(), r);
        progress(done + 1, total);
    }
    Ok(rows)
}

fn local_state(session: &SessionInfo) -> Local {
    if !codec::is_gzip_path(&session.path) {
        return Local::Skipped(SkipReason::Plain);
    }
    if lock::read_lock(&session.path).is_some() {
        return Local::Skipped(SkipReason::Locked);
    }
    // Checked on the on-disk size before the file is read.
    if session.size > MAX_BYTES {
        return Local::Skipped(SkipReason::OverCap);
    }
    match std::fs::File::open(&session.path).and_then(sha256_reader) {
        Ok((hash, _)) => Local::Hash(hash),
        Err(_) => Local::Skipped(SkipReason::Unreadable),
    }
}

/// Keep rows matching `names`, returns the names that matched nothing.
pub(super) fn select(
    rows: Vec<Row>,
    storage: &StorageManager,
    names: &[String],
) -> (Vec<Row>, Vec<String>) {
    let mut keep = BTreeSet::new();
    let mut unmatched = Vec::new();
    for name in names {
        match matching(&rows, storage, name) {
            Some(path) => keep.insert(path),
            None => {
                unmatched.push(name.clone());
                false
            }
        };
    }
    let kept = rows.into_iter().filter(|r| keep.contains(&r.path));
    (kept.collect(), unmatched)
}

/// Same lookup as other commands, falls back to the name as a remote-only path.
fn matching(rows: &[Row], storage: &StorageManager, name: &str) -> Option<String> {
    // The name itself first, a remote-only `a.cast.gz` hides behind a local plain `a.cast`.
    let wanted = wanted_path(storage, name);
    let exact = slashed(Path::new(name));
    [exact, wanted]
        .iter()
        .find_map(|w| rows.iter().find(|r| &r.path == w))
        .map(|r| r.path.clone())
}

/// The path a user-given name stands for.
fn wanted_path(storage: &StorageManager, name: &str) -> String {
    storage
        .resolve_cast_path(name)
        .and_then(|p| relative(storage, &p))
        .unwrap_or_else(|| slashed(&codec::gzip_sibling(Path::new(name))))
}

fn relative(storage: &StorageManager, path: &Path) -> Option<String> {
    path.strip_prefix(storage.storage_dir()).ok().map(slashed)
}

fn slashed(path: &Path) -> String {
    let parts: Vec<_> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect();
    parts.join("/")
}
