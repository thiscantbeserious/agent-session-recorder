//! Pull of one session, verify before anything touches the disk.

use super::backend::BackendError;
use super::crypto::{blob_aad, open, sha256_hex};
use super::fail::Fail;
use super::index::{self, Index};
use super::record::{record, Synced};
use super::run::Engine;
use super::scan::Row;
use crate::files::{backup, codec};

/// Download, open, verify the hash, back up an existing file, write atomically.
pub(super) fn pull_one(engine: &Engine<'_>, row: &Row, index: &mut Index) -> Result<u64, Fail> {
    let (Some(id), Some(meta), Some(token)) = (&row.remote_id, &row.meta, &row.token) else {
        return Err(String::from("remote entry is missing").into());
    };
    let (_manifest, blob) = match engine.backend.pull(id) {
        Err(BackendError::NotFound) => return Err(Fail::Gone),
        other => other.map_err(|e| e.to_string())?,
    };
    let plain =
        open(engine.key, &blob_aad(&meta.path, meta.gen), &blob).map_err(|e| e.to_string())?;
    if sha256_hex(&plain) != meta.sha256 {
        return Err("downloaded session does not match its recorded hash".into());
    }
    let target = engine.storage.storage_dir().join(&meta.path);
    write_session(&target, &plain)?;
    let synced = Synced {
        sha256: meta.sha256.clone(),
        remote_id: id.clone(),
        gen: meta.gen,
        token: token.clone(),
    };
    record(index, &meta.path, synced);
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

/// A hidden `.bak` is replaced by the current file, the local file is the newer state
/// worth keeping and a stale backup must not block pulls forever.
fn backup_existing(target: &std::path::Path) -> Result<(), String> {
    let bak = backup::backup_path_for(target);
    std::fs::copy(target, &bak)
        .map(|_| ())
        .map_err(|e| format!("backup {}: {e}", bak.display()))
}
