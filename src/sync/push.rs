//! Push of one session as compare-and-swap, create and pending id first, see the ADR.

use std::io::Write;

use super::backend::BackendError;
use super::crypto::{blob_aad, open_meta, seal, seal_meta, sha256_hex, Meta};
use super::fail::Fail;
use super::index::Index;
use super::plan::Remote;
use super::record::{record, record_pending, save_index, Synced};
use super::run::Engine;
use super::scan::Row;

/// Report text when somebody else pushed first, nothing local changes.
const CHANGED: &str = "changed on the remote, run again";

/// Everything one push needs, sealed once.
struct Push<'a> {
    engine: &'a Engine<'a>,
    row: &'a Row,
    sha256: String,
    gen: u64,
    token: String,
    blob: Vec<u8>,
}

/// Create when new and store the pending id, compare-and-swap push with the planned token,
/// record gen and token, then the listing hint. A gist deleted on GitHub needs `overwrite`.
pub(super) fn push_one(
    engine: &Engine<'_>,
    row: &Row,
    index: &mut Index,
    overwrite: bool,
) -> Result<u64, Fail> {
    let plain =
        std::fs::read(engine.storage.storage_dir().join(&row.path)).map_err(|e| e.to_string())?;
    let sha256 = sha256_hex(&plain);
    let gen = row.gen + 1;
    let meta = Meta {
        v: 2,
        path: row.path.clone(),
        sha256: sha256.clone(),
        size: plain.len() as u64,
        gen,
    };
    let push = Push {
        engine,
        row,
        sha256,
        gen,
        token: seal_meta(engine.key, &meta),
        blob: seal(engine.key, &blob_aad(&row.path, gen), &plain),
    };
    push.run(index, overwrite).map(|()| plain.len() as u64)
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
        let blob = |w: &mut dyn Write| w.write_all(&self.blob);
        match self.engine.backend.push(id, expected, &self.token, &blob) {
            Ok(()) => self.finish(id, &self.token, self.gen, index),
            Err(BackendError::Changed(Some(theirs))) => self.adopt(id, &theirs, index),
            Err(BackendError::Changed(None)) => Err(CHANGED.into()),
            Err(BackendError::NotFound) => Err(Fail::Gone),
            Err(e) => Err(e.into()),
        }
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
            let healed = self.engine.backend.describe(id, theirs);
            if matches!(healed, Err(BackendError::RateLimited)) {
                return Err(Fail::RateLimited);
            }
        }
        Err(CHANGED.into())
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
