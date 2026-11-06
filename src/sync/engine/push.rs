//! Push of one session as compare-and-swap, create and pending id first, see the ADR.

use std::cell::Cell;
use std::io::{self, Write};

use super::fail::{msg, Fail};
use super::index::Index;
use super::plan::Remote;
use super::record::{record, record_pending, save_index, Synced};
use super::run::Engine;
use super::scan::Row;
use crate::sync::backend::BackendError;
use crate::sync::crypto::blob::seal_stream;
use crate::sync::crypto::{open_meta, seal_meta, sha256_reader, Meta};

/// Report text when somebody else pushed first, nothing local changes.
const CHANGED: &str = "changed on the remote, run again";

/// Report text when the file moved between the hash pre-pass and the seal pass.
const CHANGED_WHILE: &str = "session changed while pushing, run again";

/// Everything one push needs, the blob is sealed from the file while it is sent.
struct Push<'a> {
    engine: &'a Engine<'a>,
    row: &'a Row,
    sha256: String,
    size: u64,
    gen: u64,
    token: String,
    /// The seal pass saw other bytes than the hash pre-pass.
    moved: Cell<bool>,
}

/// Create when new and store the pending id, compare-and-swap push with the planned token,
/// record gen and token, then the listing hint. A gist deleted on GitHub needs `overwrite`.
pub(super) fn push_one(
    engine: &Engine<'_>,
    row: &Row,
    index: &mut Index,
    overwrite: bool,
) -> Result<u64, Fail> {
    let path = engine.storage.storage_dir().join(&row.path);
    let (sha256, size) = std::fs::File::open(path)
        .and_then(sha256_reader)
        .map_err(msg)?;
    let gen = row.gen + 1;
    let meta = Meta {
        v: 2,
        path: row.path.clone(),
        sha256: sha256.clone(),
        size,
        gen,
    };
    let push = Push {
        engine,
        row,
        sha256,
        size,
        gen,
        token: seal_meta(engine.key, &meta),
        moved: Cell::new(false),
    };
    push.run(index, overwrite).map(|()| size)
}

/// The description of the last recorded push never landed (index gen higher than the listing,
/// or the listing misses the gist), copy the index token into it and plan against the index,
/// as if the listing had healed. Never moves the description to a lower gen.
pub(super) fn heal(
    engine: &Engine<'_>,
    row: &mut Row,
    index: &Index,
    overwrite: bool,
) -> Result<(), Fail> {
    let Some(entry) = index.entries.get(&row.path) else {
        return Ok(());
    };
    let (Some(token), Some(id)) = (&entry.token, &row.remote_id) else {
        return Ok(());
    };
    if entry.pending || *id != entry.remote_id || !(row.repair_description || row.meta.is_none()) {
        return Ok(());
    }
    match describe(engine, id, token) {
        Err(Fail::Gone) if overwrite => return Ok(()),
        other => other?,
    }
    row.token = Some(token.clone());
    row.gen = entry.gen;
    row.repair_description = false;
    if matches!(row.input.remote, Remote::Hash(_) | Remote::Absent) {
        row.input.remote = Remote::Hash(entry.sha256.clone());
    }
    Ok(())
}

/// Reports a failed description, unlike the best effort one of a pull, the push is only
/// finished once the listing knows it.
fn describe(engine: &Engine<'_>, id: &str, token: &str) -> Result<(), Fail> {
    engine
        .backend
        .describe(id, token)
        .map_err(|e| match Fail::from(e) {
            Fail::Other(why) => Fail::Other(format!(
                "pushed, description not updated, repaired on the next run ({why})"
            )),
            other => other,
        })
}

impl Push<'_> {
    fn run(&self, index: &mut Index, overwrite: bool) -> Result<(), Fail> {
        let sent = match &self.row.remote_id {
            Some(id) => self.send(id, self.row.token.as_deref(), index),
            None => self.create_and_send(index),
        };
        match sent {
            Err(Fail::Gone) if overwrite => self.create_and_send(index),
            other => other,
        }
    }

    fn create_and_send(&self, index: &mut Index) -> Result<(), Fail> {
        let id = self.engine.backend.create()?;
        record_pending(index, &self.row.path, &id);
        self.save(index)?;
        self.send(&id, None, index)
    }

    fn send(&self, id: &str, expected: Option<&str>, index: &mut Index) -> Result<(), Fail> {
        self.moved.set(false);
        let blob = |w: &mut dyn Write| self.seal_file(w);
        let sent = self.engine.backend.push(id, expected, &self.token, &blob);
        if self.moved.get() {
            return Err(CHANGED_WHILE.into());
        }
        match sent {
            Ok(()) => self.finish(id, &self.token, self.gen, index),
            Err(BackendError::Changed(Some(theirs))) => self.adopt(id, &theirs, index),
            Err(BackendError::Changed(None)) => Err(CHANGED.into()),
            Err(BackendError::NotFound) => Err(Fail::Gone),
            Err(e) => Err(e.into()),
        }
    }

    /// Opens the file again and seals it into `out`, a different hash or size than the pre-pass
    /// fails the source so the backend commits nothing. Runs again on every retry.
    fn seal_file(&self, out: &mut dyn Write) -> io::Result<()> {
        let file = std::fs::File::open(self.engine.storage.storage_dir().join(&self.row.path))?;
        let (key, path) = (self.engine.key, &self.row.path);
        let sealed = seal_stream(key, path, self.gen, file, out).map_err(io::Error::other)?;
        if sealed != (self.sha256.clone(), self.size) {
            self.moved.set(true);
            return Err(io::Error::other(CHANGED_WHILE));
        }
        Ok(())
    }

    /// The remote holds our own earlier push of this very content, its index save got lost.
    /// Another content of our path at a gen we know of means the description may lag behind
    /// the manifest, so it is set to `theirs` (only a rate limit stops the run), the next run
    /// plans against the real remote.
    fn adopt(&self, id: &str, theirs: &str, index: &mut Index) -> Result<(), Fail> {
        let Ok(m) = open_meta(self.engine.key, theirs) else {
            return Err(CHANGED.into());
        };
        if m.path != self.row.path {
            return Err(CHANGED.into());
        }
        if m.sha256 == self.sha256 && m.gen > self.row.gen {
            return self.finish(id, theirs, m.gen, index);
        }
        let known = index.entries.get(&m.path).map_or(0, |e| e.gen);
        if m.gen >= known.max(self.row.gen) {
            self.heal_description(id, theirs)?;
        }
        Err(CHANGED.into())
    }

    /// D11: GitHub has no conditional description update, so the manifest is read again right
    /// before and the description is only set when it still is the adopted token. A millisecond
    /// window remains (README known limits), a newer describe in it heals on the next run.
    /// ponytail: the re-read is a full `pull` (a clone for the gist), a manifest only read on
    /// the trait would be cheaper, add when adopt shows up in practice.
    fn heal_description(&self, id: &str, theirs: &str) -> Result<(), Fail> {
        let current = self.engine.backend.pull(id).map(|(manifest, _)| manifest);
        if !matches!(&current, Ok(Some(now)) if now == theirs) {
            return Ok(());
        }
        match self.engine.backend.describe(id, theirs) {
            Err(BackendError::RateLimited) => Err(Fail::RateLimited),
            _ => Ok(()),
        }
    }

    fn finish(&self, id: &str, token: &str, gen: u64, index: &mut Index) -> Result<(), Fail> {
        let synced = Synced {
            sha256: self.sha256.clone(),
            remote_id: id.to_string(),
            gen,
            token: token.to_string(),
        };
        record(index, &self.row.path, synced);
        self.save(index)?;
        describe(self.engine, id, token)
    }

    fn save(&self, index: &Index) -> Result<(), String> {
        save_index(self.engine.storage, index)
    }
}
