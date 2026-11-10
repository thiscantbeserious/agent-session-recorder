use std::io::{Read, Write};
use std::path::Path;

use super::fail::{msg, Fail};
use super::index::Index;
use super::record::{record, save_index, Synced};
use super::run::BackendEngine;
use super::scan::Row;
use crate::files::{backup, codec};
use crate::sync::backend::BackendError;
use crate::sync::crypto::blob::open_stream;
use crate::sync::crypto::{open_meta, Key, Meta};

const OLDER: &str = "remote version is older than the last sync, not pulling";

/// Download manifest and the blob reader together, check the manifest (present and opens, same
/// path, gen not below the index or the planned gen), then stream the blob into the hidden temp
/// in this order, nothing replaces the local file before all pass: every frame opens with the
/// manifest gen, hash and size equal the manifest, the `.bak` is rotated, the temp is renamed.
/// The manifest wins over a lagging description, the description is repaired afterwards.
pub(super) fn pull_one(
    engine: &BackendEngine<'_>,
    row: &Row,
    index: &mut Index,
) -> Result<u64, Fail> {
    let Some(id) = &row.remote_id else {
        return Err(String::from("remote entry is missing").into());
    };
    let (manifest, reader) = engine.backend.pull(id).map_err(Fail::from)?;
    let token = manifest.ok_or("remote has no manifest, not pulling")?;
    let meta = checked_meta(engine, row, index, &token)?;
    let target = engine.storage.storage_dir().join(&meta.path);
    let size = write_session(engine.key, &meta, reader, &target)?;
    let synced = Synced {
        sha256: meta.sha256,
        remote_id: id.clone(),
        gen: meta.gen,
        token: token.clone(),
    };
    record(index, &engine.storage.storage_dir(), &meta.path, synced);
    save_index(engine.storage, index)?;
    if row.token.as_deref() != Some(&token) {
        repair(engine, id, &token)?;
    }
    Ok(size)
}

/// The manifest checks that need no blob.
fn checked_meta(
    engine: &BackendEngine<'_>,
    row: &Row,
    index: &Index,
    token: &str,
) -> Result<Meta, Fail> {
    let meta = open_meta(engine.key, token).map_err(msg)?;
    let synced = index.entries.get(&row.path).map_or(0, |e| e.gen);
    if meta.path != row.path {
        return Err("manifest is for another session, not pulling".into());
    }
    if meta.gen < synced.max(row.gen) {
        return Err(OLDER.into());
    }
    Ok(meta)
}

/// Best effort unlike the push one, the pull is recorded already, only a rate limit stops the run.
fn repair(engine: &BackendEngine<'_>, id: &str, token: &str) -> Result<(), Fail> {
    match engine.backend.describe(id, token) {
        Err(BackendError::RateLimited) => Err(Fail::RateLimited),
        _ => Ok(()),
    }
}

/// Streams the blob into the hidden temp, the reader (and its scratch clone) is dropped when
/// the stream is done, before anything else. Returns the plaintext size.
fn write_session(
    key: &Key,
    meta: &Meta,
    reader: Box<dyn Read>,
    target: &Path,
) -> Result<u64, String> {
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(msg)?;
    }
    let mut size = 0;
    codec::atomic_write_raw_with(target, |out| {
        size = verify_into(key, meta, reader, out)?;
        // Write errors surface before the backup is touched.
        out.flush()?;
        if target.exists() {
            backup_existing(target).map_err(anyhow::Error::msg)?;
        }
        Ok(())
    })
    .map_err(|e| format!("{e:#}"))?;
    Ok(size)
}

/// Opens every frame into `out`, then compares hash and size with the manifest.
fn verify_into(
    key: &Key,
    meta: &Meta,
    reader: Box<dyn Read>,
    out: &mut dyn Write,
) -> anyhow::Result<u64> {
    let (hash, size) = open_stream(key, &meta.path, meta.gen, reader, out)?;
    if hash != meta.sha256 || size != meta.size {
        anyhow::bail!("downloaded session does not match its recorded hash");
    }
    Ok(size)
}

/// A hidden `.bak` is replaced by the current file, the local file is the newer state
/// worth keeping and a stale backup must not block pulls forever.
fn backup_existing(target: &std::path::Path) -> Result<(), String> {
    let bak = backup::backup_path_for(target);
    std::fs::copy(target, &bak)
        .map(|_| ())
        .map_err(|e| format!("backup {}: {e}", bak.display()))
}

#[cfg(test)]
mod tests {
    use super::{verify_into, Key, Meta, Read, Write};
    use crate::sync::crypto::blob::seal_blob;
    use crate::sync::crypto::sha256_hex;

    struct Full;

    impl Write for Full {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk full"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn meta(plain: &[u8]) -> Meta {
        Meta {
            v: 2,
            path: "claude/x.cast.gz".into(),
            sha256: sha256_hex(plain),
            size: plain.len() as u64,
            gen: 1,
        }
    }

    fn sealed(plain: &[u8]) -> Box<dyn Read> {
        let blob = seal_blob(&Key::from_bytes([7; 32]), "claude/x.cast.gz", 1, plain);
        Box::new(std::io::Cursor::new(blob))
    }

    #[test]
    fn a_destination_error_mid_stream_is_reported() {
        let plain = vec![1u8; 200_000];
        let key = Key::from_bytes([7; 32]);
        let got = verify_into(&key, &meta(&plain), sealed(&plain), &mut Full);
        assert!(
            matches!(&got, Err(e) if format!("{e:#}").contains("disk full")),
            "{got:?}"
        );
    }

    #[test]
    fn size_and_hash_are_checked_against_the_manifest() {
        let plain = vec![1u8; 70_000];
        let key = Key::from_bytes([7; 32]);
        let mut sink = Vec::new();
        let ok = verify_into(&key, &meta(&plain), sealed(&plain), &mut sink);
        assert_eq!(ok.ok(), Some(70_000));
        let wrong_size = Meta {
            size: 1,
            ..meta(&plain)
        };
        let got = verify_into(&key, &wrong_size, sealed(&plain), &mut sink);
        assert!(got.is_err(), "size mismatch must fail");
    }
}
