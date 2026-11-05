//! Pull of one session, verify before anything touches the disk.

use super::crypto::{blob_aad, open, sha256_hex};
use super::index::{self, Index, IndexEntry};
use super::run::Engine;
use super::scan::Row;
use crate::files::{backup, codec};

/// Download, open, verify the hash, back up an existing file, write atomically.
pub(super) fn pull_one(engine: &Engine<'_>, row: &Row, index: &mut Index) -> Result<u64, String> {
    let (Some(id), Some(meta)) = (&row.remote_id, &row.meta) else {
        return Err("remote entry is missing".into());
    };
    let blob = engine.backend.pull(id).map_err(|e| e.to_string())?;
    let plain = open(engine.key, &blob_aad(&meta.path), &blob).map_err(|e| e.to_string())?;
    if sha256_hex(&plain) != meta.sha256 {
        return Err("downloaded session does not match its recorded hash".into());
    }
    let target = engine.storage.storage_dir().join(&meta.path);
    write_session(&target, &plain)?;
    let entry = IndexEntry {
        sha256: meta.sha256.clone(),
        remote_id: id.clone(),
        synced_at: chrono::Utc::now().to_rfc3339(),
    };
    index.entries.insert(meta.path.clone(), entry);
    index::save(&engine.storage.storage_dir(), index).map_err(|e| e.to_string())?;
    Ok(plain.len() as u64)
}

fn write_session(target: &std::path::Path, plain: &[u8]) -> Result<(), String> {
    let err = |e: &dyn std::fmt::Display| e.to_string();
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(|e| err(&e))?;
    }
    if target.exists() {
        backup_existing(target)?;
    }
    codec::atomic_write_raw(target, plain).map_err(|e| err(&e))
}

/// A hidden `.bak` is kept, an older different backup is never replaced silently.
fn backup_existing(target: &std::path::Path) -> Result<(), String> {
    let bak = backup::backup_path_for(target);
    if bak.exists() {
        let same = std::fs::read(&bak).ok() == std::fs::read(target).ok();
        if !same {
            return Err(format!("a different backup exists: {}", bak.display()));
        }
        return Ok(());
    }
    backup::create_backup(target)
        .map(|_| ())
        .map_err(|e| e.to_string())
}
