//! Pull of one session, verify before anything touches the disk.

use super::backend::BackendError;
use super::crypto::{blob_aad, open, open_meta, sha256_hex, Meta};
use super::fail::Fail;
use super::index::Index;
use super::record::{record, save_index, Synced};
use super::run::Engine;
use super::scan::Row;
use crate::files::{backup, codec};

const OLDER: &str = "remote version is older than the last sync, not pulling";

/// Download manifest and blob together, then check in this order, nothing is written before
/// all pass: manifest present and opens, same path, gen not below the index or the planned
/// gen, blob opens with the manifest gen, blob hash equals the manifest hash.
/// The manifest wins over a lagging description, the description is repaired afterwards.
pub(super) fn pull_one(engine: &Engine<'_>, row: &Row, index: &mut Index) -> Result<u64, Fail> {
    let Some(id) = &row.remote_id else {
        return Err(String::from("remote entry is missing").into());
    };
    let (token, meta, plain) = verified(engine, row, index, id)?;
    write_session(&engine.storage.storage_dir().join(&meta.path), &plain)?;
    let synced = Synced {
        sha256: meta.sha256,
        remote_id: id.clone(),
        gen: meta.gen,
        token: token.clone(),
    };
    record(index, &meta.path, synced);
    save_index(engine.storage, index)?;
    if row.token.as_deref() != Some(&token) {
        repair(engine, id, &token)?;
    }
    Ok(plain.len() as u64)
}

/// Every check that has to pass before anything is written.
fn verified(
    engine: &Engine<'_>,
    row: &Row,
    index: &Index,
    id: &str,
) -> Result<(String, Meta, Vec<u8>), Fail> {
    let (manifest, blob) = engine.backend.pull(id).map_err(Fail::from)?;
    let token = manifest.ok_or("remote has no manifest, not pulling")?;
    let meta = open_meta(engine.key, &token).map_err(|e| e.to_string())?;
    let synced = index.entries.get(&row.path).map_or(0, |e| e.gen);
    if meta.path != row.path {
        return Err("manifest is for another session, not pulling".into());
    }
    if meta.gen < synced.max(row.gen) {
        return Err(OLDER.into());
    }
    let aad = blob_aad(&meta.path, meta.gen);
    let plain = open(engine.key, &aad, &blob).map_err(|e| e.to_string())?;
    if sha256_hex(&plain) != meta.sha256 {
        return Err("downloaded session does not match its recorded hash".into());
    }
    Ok((token, meta, plain))
}

/// Best effort unlike the push one, the pull is recorded already, only a rate limit stops the run.
fn repair(engine: &Engine<'_>, id: &str, token: &str) -> Result<(), Fail> {
    match engine.backend.describe(id, token) {
        Err(BackendError::RateLimited) => Err(Fail::RateLimited),
        _ => Ok(()),
    }
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
