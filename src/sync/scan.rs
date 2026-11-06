//! Builds plan inputs from the storage dir, the index and the remote list.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::Result;

use super::backend::RemoteEntry;
use super::crypto::{open_meta, sha256_hex, CryptoError, Key, Meta};
use super::index::Index;
use super::plan::{Input, Local, Remote, SkipReason};
use crate::files::{codec, lock};
use crate::storage::SessionInfo;
use crate::StorageManager;

/// Manifest version this build reads, older tokens are never touched.
const META_VERSION: u8 = 2;

/// On-disk size cap per session, from GitHub repository size guidance.
pub const MAX_BYTES: u64 = 1024 * 1024 * 1024;

/// One path with everything the engine needs to act on it.
#[derive(Debug, Clone)]
pub struct Row {
    /// `agent/name.cast.gz`, or `remote:<id>` for entries that cannot be opened.
    pub path: String,
    pub input: Input,
    pub remote_id: Option<String>,
    pub meta: Option<Meta>,
}

fn row(path: &str, local: Local, index: &Index) -> Row {
    Row {
        path: path.to_string(),
        input: Input {
            local,
            remote: Remote::Absent,
            last: index.entries.get(path).map(|e| e.sha256.clone()),
        },
        remote_id: None,
        meta: None,
    }
}

/// Join local sessions, remote entries and the index into rows sorted by path.
pub fn scan(
    storage: &StorageManager,
    key: &Key,
    remote: &[RemoteEntry],
    index: &Index,
) -> Result<Vec<Row>> {
    let mut rows = BTreeMap::new();
    for session in storage.list_sessions(None)? {
        let path = format!("{}/{}", session.agent, session.filename);
        rows.insert(path.clone(), row(&path, local_state(&session), index));
    }
    for entry in remote {
        merge_remote(&mut rows, key, entry, index);
    }
    Ok(rows.into_values().collect())
}

fn local_state(session: &SessionInfo) -> Local {
    if !codec::is_gzip_path(&session.path) {
        return Local::Skipped(SkipReason::Plain);
    }
    if lock::read_lock(&session.path).is_some() {
        return Local::Skipped(SkipReason::Locked);
    }
    // Checked on the on-disk size before the file is read into memory.
    if session.size > MAX_BYTES {
        return Local::Skipped(SkipReason::OverCap);
    }
    match std::fs::read(&session.path) {
        Ok(bytes) => Local::Hash(sha256_hex(&bytes)),
        Err(_) => Local::Skipped(SkipReason::Unreadable),
    }
}

fn merge_remote(rows: &mut BTreeMap<String, Row>, key: &Key, entry: &RemoteEntry, index: &Index) {
    match open_meta(key, &entry.meta_token) {
        Ok(meta) if meta.v == META_VERSION && valid_path(&meta.path) => {
            attach(rows, entry, meta, index)
        }
        Ok(_) | Err(CryptoError::UnsupportedVersion(_) | CryptoError::BadMeta) => {
            opaque(rows, entry, Remote::UnknownVersion, index)
        }
        Err(CryptoError::OtherKey) => opaque(rows, entry, Remote::OtherKey, index),
        Err(_) => opaque(rows, entry, Remote::UnknownVersion, index),
    }
}

fn attach(rows: &mut BTreeMap<String, Row>, entry: &RemoteEntry, meta: Meta, index: &Index) {
    let row = rows
        .entry(meta.path.clone())
        .or_insert_with(|| row(&meta.path, Local::Absent, index));
    if row.remote_id.is_some() {
        return; // ponytail: a duplicate remote entry for one path is ignored, first wins
    }
    row.input.remote = Remote::Hash(meta.sha256.clone());
    row.remote_id = Some(entry.id.clone());
    row.meta = Some(meta);
}

fn opaque(rows: &mut BTreeMap<String, Row>, entry: &RemoteEntry, remote: Remote, index: &Index) {
    let path = format!("remote:{}", entry.id);
    let mut opaque = row(&path, Local::Absent, index);
    opaque.input.remote = remote;
    opaque.remote_id = Some(entry.id.clone());
    rows.insert(path, opaque);
}

/// `agent/name.cast.gz`, nothing that can escape the storage dir or hit hidden files.
fn valid_path(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    parts.len() == 2
        && parts
            .iter()
            .all(|p| !p.is_empty() && !p.starts_with('.') && !p.contains('\\'))
        && parts[1].ends_with(".cast.gz")
}

/// Keep rows matching `names`, returns the names that matched nothing.
pub fn select(
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
    let wanted = storage
        .resolve_cast_path(name)
        .and_then(|p| relative(storage, &p))
        .unwrap_or_else(|| slashed(&codec::gzip_sibling(Path::new(name))));
    rows.iter()
        .find(|r| r.path == wanted)
        .map(|r| r.path.clone())
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
