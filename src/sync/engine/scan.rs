//! Builds plan inputs from the storage dir, the index and the remote list.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::Result;

use super::index::Index;
use super::plan::{Input, Local, Remote, SkipReason};
use crate::files::{codec, lock};
use crate::storage::SessionInfo;
use crate::sync::backend::RemoteEntry;
use crate::sync::crypto::{open_meta, sha256_reader, CryptoError, Key, Meta};
use crate::StorageManager;

/// Manifest version this build reads, older tokens are never touched.
const META_VERSION: u8 = 2;

/// On-disk size cap per session, from GitHub repository size guidance.
pub(super) const MAX_BYTES: u64 = 1024 * 1024 * 1024;

/// One path with everything the engine needs to act on it.
#[derive(Debug, Clone)]
pub struct Row {
    /// `agent/name.cast.gz`, or `remote:<id>` for entries that cannot be opened.
    pub path: String,
    pub input: Input,
    pub remote_id: Option<String>,
    /// Sealed token from the listing, the `expected` of the compare-and-swap push.
    pub token: Option<String>,
    pub meta: Option<Meta>,
    /// Generation of the last sync, from the index.
    pub gen: u64,
    /// The index holds an id whose first push is not recorded, resume it.
    pub pending: bool,
    /// Other listed gists for this path, the picked one is `remote_id`.
    pub duplicates: Vec<Duplicate>,
    /// The listed description is older than the index, copy the newest token into it.
    pub repair_description: bool,
}

/// A listed gist that lost the pick for its path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicate {
    pub id: String,
    /// Same SHA-256 as the picked one, a harmless extra copy.
    pub same_hash: bool,
}

fn row(path: &str, local: Local, index: &Index) -> Row {
    Row {
        path: path.to_string(),
        input: Input {
            local,
            remote: Remote::Absent,
            last: index
                .entries
                .get(path)
                .filter(|e| !e.pending)
                .map(|e| e.sha256.clone()),
        },
        remote_id: None,
        token: None,
        meta: None,
        gen: 0,
        pending: false,
        duplicates: Vec::new(),
        repair_description: false,
    }
}

/// Join local sessions, remote entries and the index into rows sorted by path.
pub fn scan(
    storage: &StorageManager,
    key: &Key,
    remote: &[RemoteEntry],
    index: &Index,
    names: Option<&[String]>,
) -> Result<Vec<Row>> {
    let mut rows = BTreeMap::new();
    for session in storage.list_sessions(None)? {
        let path = format!("{}/{}", session.agent, session.filename);
        rows.insert(path.clone(), row(&path, local_state(&session), index));
    }
    merge_remotes(&mut rows, key, remote, index);
    fall_back_to_index(&mut rows, index);
    Ok(rows.into_values().collect())
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

/// Entries of one path, the pick is first.
type Group<'a> = Vec<(&'a RemoteEntry, Meta)>;

fn merge_remotes(
    rows: &mut BTreeMap<String, Row>,
    key: &Key,
    remote: &[RemoteEntry],
    index: &Index,
) {
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    for entry in remote {
        match classify(key, entry) {
            Ok(meta) => groups
                .entry(meta.path.clone())
                .or_default()
                .push((entry, meta)),
            Err(state) => opaque(rows, entry, state, index),
        }
    }
    for (path, mut group) in groups {
        // Oldest server creation wins, the id breaks a tie, the same on every machine.
        group.sort_by(|a, b| (&a.0.created_at, &a.0.id).cmp(&(&b.0.created_at, &b.0.id)));
        let row = rows
            .entry(path.clone())
            .or_insert_with(|| row(&path, Local::Absent, index));
        attach(row, group, index);
    }
}

fn classify(key: &Key, entry: &RemoteEntry) -> Result<Meta, Remote> {
    match open_meta(key, &entry.meta_token) {
        Ok(meta) if meta.v == META_VERSION && meta.gen > 0 && valid_path(&meta.path) => Ok(meta),
        Err(CryptoError::OtherKey) => Err(Remote::OtherKey),
        _ => Err(Remote::UnknownVersion),
    }
}

fn attach(row: &mut Row, group: Group, index: &Index) {
    let mut group = group.into_iter();
    let Some((entry, meta)) = group.next() else {
        return;
    };
    row.duplicates = group
        .map(|(e, m)| Duplicate {
            id: e.id.clone(),
            same_hash: m.sha256 == meta.sha256,
        })
        .collect();
    let differs = row.duplicates.iter().any(|d| !d.same_hash);
    row.input.remote = if differs {
        Remote::Duplicate
    } else {
        Remote::Hash(meta.sha256.clone())
    };
    let synced = index.entries.get(&row.path).map_or(0, |e| e.gen);
    row.repair_description = meta.gen < synced;
    row.remote_id = Some(entry.id.clone());
    row.token = Some(entry.meta_token.clone());
    row.gen = meta.gen;
    row.meta = Some(meta);
}

/// The listing lags or a pending create never got a description, the index id is still ours,
/// unless it is listed as another key or an unknown version.
fn fall_back_to_index(rows: &mut BTreeMap<String, Row>, index: &Index) {
    for (path, entry) in &index.entries {
        let taken = rows
            .get(&format!("remote:{}", entry.remote_id))
            .map(|r| r.input.remote.clone());
        let Some(row) = rows.get_mut(path).filter(|r| r.remote_id.is_none()) else {
            continue;
        };
        if let Some(state) = taken {
            // listed but not ours to open, pushing into it would clobber it
            row.input.remote = state;
            continue;
        }
        row.remote_id = Some(entry.remote_id.clone());
        row.pending = entry.pending;
        row.gen = entry.gen;
        row.token = entry.token.clone().filter(|_| !entry.pending);
    }
}

fn opaque(rows: &mut BTreeMap<String, Row>, entry: &RemoteEntry, remote: Remote, index: &Index) {
    let path = format!("remote:{}", entry.id);
    let mut opaque = row(&path, Local::Absent, index);
    opaque.input.remote = remote;
    opaque.remote_id = Some(entry.id.clone());
    opaque.token = Some(entry.meta_token.clone());
    rows.insert(path, opaque);
}

/// `agent/name.cast.gz`, nothing that can escape the storage dir or hit hidden files.
fn valid_path(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    parts.len() == 2
        && parts
            .iter()
            .all(|p| !p.is_empty() && !p.starts_with('.') && !p.contains(['\\', ':']))
        && parts[1].ends_with(".cast.gz")
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

#[cfg(test)]
mod tests;
