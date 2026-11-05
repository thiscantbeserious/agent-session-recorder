//! Hidden `.x.cast.bak` backups whose session is gone and `.x.cast.lock` files of dead recordings.

use crate::files::{codec, lock};
use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Leftover auxiliary files, `bytes` is the size of all of them.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Orphans {
    pub backups: Vec<PathBuf>,
    pub locks: Vec<PathBuf>,
    pub bytes: u64,
}

impl Orphans {
    pub fn is_empty(&self) -> bool {
        self.backups.is_empty() && self.locks.is_empty()
    }

    fn all(&self) -> impl Iterator<Item = &PathBuf> {
        self.backups.iter().chain(&self.locks)
    }
}

/// Orphaned backups (neither `x.cast` nor `x.cast.gz` exists) and stale locks (owner not alive)
/// in every agent directory. Live locks and backups of existing sessions are never listed.
pub fn find(storage_dir: &Path) -> Result<Orphans> {
    let mut found = Orphans::default();
    if !storage_dir.exists() {
        return Ok(found);
    }
    for dir in fs::read_dir(storage_dir)?.filter_map(Result::ok) {
        if dir.path().is_dir() {
            scan_dir(&dir.path(), &mut found)?;
        }
    }
    found.backups.sort();
    found.locks.sort();
    found.bytes = found
        .all()
        .filter_map(|p| fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();
    Ok(found)
}

fn scan_dir(dir: &Path, found: &mut Orphans) -> Result<()> {
    for entry in fs::read_dir(dir)?.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(hidden) = name.strip_prefix('.') else {
            continue;
        };
        if let Some(session) = hidden.strip_suffix(".bak").filter(|s| is_session_name(s)) {
            let session = dir.join(session);
            if !session.exists() && !codec::other_variant(&session).exists() {
                found.backups.push(path);
            }
        } else if let Some(session) = hidden.strip_suffix(".lock").filter(|s| is_session_name(s)) {
            if lock::read_lock(&dir.join(session)).is_none() {
                found.locks.push(path);
            }
        }
    }
    Ok(())
}

fn is_session_name(name: &str) -> bool {
    codec::session_stem(name) != name
}

/// Delete the listed files, returns how many were removed.
pub fn remove(orphans: &Orphans) -> usize {
    orphans.all().filter(|p| fs::remove_file(p).is_ok()).count()
}
