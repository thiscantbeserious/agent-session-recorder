//! Sync engine, push, pull, delete and status against any backend. Never prints.

use thiserror::Error;

use super::fail::{Fail, Stop};
use super::index::{self, Index};
use super::item::{hash_of, not_found, passive, remote_hash_of, strip_done};
use super::lock::SyncLock;
use super::observe::{counted, no_progress, Observer, Step};
use super::plan::{decide, resolve, Command, Decision, Local, Remote};
use super::pull::pull_one;
use super::push::{heal, push_one};
use super::record::{record, save_index, Synced};
use super::scan::{scan, select, Duplicate, Row};
use crate::sync::backend::{BackendError, SyncBackend};
use crate::sync::crypto::Key;
use crate::StorageManager;

/// Whole-run failures, per-session failures live in the report.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SyncError {
    #[error("another sync is running")]
    Busy,
    #[error(transparent)]
    Backend(#[from] BackendError),
    #[error("{0}")]
    Failed(String),
    /// GitHub reported a rate limit, `left` sessions were not done.
    #[error("GitHub rate limit reached, sync stopped, {left} left, please run again later")]
    RateLimited { left: usize },
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub overwrite: bool,
    pub dry_run: bool,
    /// Empty means every session.
    pub sessions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Dry run, would act.
    Planned,
    /// Acted, carries the bytes moved.
    Done(u64),
    /// In sync or the other direction, nothing to do for this command.
    Nothing,
    Skipped(String),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub path: String,
    pub decision: Decision,
    pub outcome: Outcome,
    /// Local file size, else the remote plaintext size, 0 when unknown.
    pub size: u64,
    /// A remote entry exists for this path.
    pub on_remote: bool,
    /// The file exists on this machine.
    pub on_local: bool,
    pub local_hash: Option<String>,
    pub remote_hash: Option<String>,
    /// Plaintext size of the remote copy, 0 when unknown.
    pub remote_size: u64,
    /// Id of the picked or resumed remote gist.
    pub remote_id: Option<String>,
    /// Created but never pushed, the next push resumes `remote_id`.
    pub pending: bool,
    /// Other gists for this path, never deleted automatically.
    pub duplicates: Vec<Duplicate>,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub items: Vec<Item>,
    pub pushed: usize,
    pub pulled: usize,
    pub deleted: usize,
    pub skipped: usize,
    pub conflicts: usize,
    pub failed: usize,
    pub bytes: u64,
    /// Index was unparseable and rebuilt.
    pub warning: Option<String>,
}

impl Report {
    fn add(&mut self, command: Command, item: Item) {
        match (&item.outcome, item.decision) {
            (Outcome::Done(bytes), _) => {
                self.bytes += bytes;
                match command {
                    Command::Push => self.pushed += 1,
                    Command::Pull => self.pulled += 1,
                }
            }
            (Outcome::Skipped(_), Decision::Conflict) => self.conflicts += 1,
            (Outcome::Skipped(_), _) => self.skipped += 1,
            (Outcome::Failed(_), _) => self.failed += 1,
            _ => {}
        }
        self.items.push(item);
    }
}

/// Engine over any backend, the key and the storage dir.
pub struct Engine<'a> {
    pub backend: &'a dyn SyncBackend,
    pub key: &'a Key,
    pub storage: &'a StorageManager,
}

struct Loaded {
    rows: Vec<Row>,
    index: Index,
    report: Report,
}

impl Engine<'_> {
    /// Decisions only, never changes anything.
    pub fn status(&self, sessions: &[String]) -> Result<Report, SyncError> {
        let opts = Options {
            dry_run: true,
            sessions: sessions.to_vec(),
            ..Options::default()
        };
        self.sync(Command::Push, &opts, &mut counted(0, &mut no_progress))
    }

    pub fn push(&self, opts: &Options) -> Result<Report, SyncError> {
        self.sync(Command::Push, opts, &mut counted(0, &mut no_progress))
    }

    pub fn pull(&self, opts: &Options) -> Result<Report, SyncError> {
        self.sync(Command::Pull, opts, &mut counted(0, &mut no_progress))
    }

    /// Like `push`, `observer(item, i, total)` runs after each acted-on item, `i` counts from 1.
    pub fn push_with(
        &self,
        opts: &Options,
        total: usize,
        observer: &mut Observer<'_>,
    ) -> Result<Report, SyncError> {
        self.sync(Command::Push, opts, &mut counted(total, observer))
    }

    pub fn pull_with(
        &self,
        opts: &Options,
        total: usize,
        observer: &mut Observer<'_>,
    ) -> Result<Report, SyncError> {
        self.sync(Command::Pull, opts, &mut counted(total, observer))
    }

    /// Remove the remote entry and the index entry, the local file stays.
    pub fn delete(&self, sessions: &[String]) -> Result<Report, SyncError> {
        self.delete_with(sessions, 0, &mut no_progress)
    }

    pub fn delete_with(
        &self,
        sessions: &[String],
        total: usize,
        observer: &mut Observer<'_>,
    ) -> Result<Report, SyncError> {
        let mut observer = counted(total, observer);
        if sessions.is_empty() {
            return Ok(Report::default());
        }
        let _lock = SyncLock::acquire(&self.storage.storage_dir())?;
        let mut loaded = self.load(sessions)?;
        let mut rows = std::mem::take(&mut loaded.rows).into_iter();
        while let Some(row) = rows.next() {
            let outcome = self.delete_one(&row, &mut loaded.index).map_err(|Stop| {
                SyncError::RateLimited {
                    left: rows.len() + 1,
                }
            })?;
            let done = matches!(outcome, Outcome::Done(_));
            let item = self.item(&row, decide(&row.input), outcome);
            observer(Step::Finished(&item));
            loaded.report.deleted += usize::from(done);
            loaded.report.add(Command::Push, strip_done(item));
        }
        Ok(loaded.report)
    }

    fn sync(
        &self,
        command: Command,
        opts: &Options,
        observer: &mut dyn FnMut(Step<'_>),
    ) -> Result<Report, SyncError> {
        let _lock = if opts.dry_run {
            None
        } else {
            Some(SyncLock::acquire(&self.storage.storage_dir())?)
        };
        let mut loaded = self.load(&opts.sessions)?;
        let mut rows = std::mem::take(&mut loaded.rows).into_iter();
        while let Some(row) = rows.next() {
            let item = self
                .one(command, opts, row, &mut loaded.index)
                .map_err(|Stop| SyncError::RateLimited {
                    left: rows.len() + 1,
                })?;
            observer(Step::Finished(&item));
            loaded.report.add(command, item);
        }
        Ok(loaded.report)
    }

    /// Heal a lagging description on push, decide, act.
    fn one(
        &self,
        command: Command,
        opts: &Options,
        mut row: Row,
        index: &mut Index,
    ) -> Result<Item, Stop> {
        let healed = match (command, opts.dry_run) {
            (Command::Push, false) => heal(self, &mut row, index, opts.overwrite),
            _ => Ok(()),
        };
        let decision = resolve(decide(&row.input), command, opts.overwrite);
        let outcome = match healed {
            Ok(()) => self.step(command, opts, decision, &row, index)?,
            Err(fail) => fail.outcome()?,
        };
        Ok(self.item(&row, decision, outcome))
    }

    fn load(&self, sessions: &[String]) -> Result<Loaded, SyncError> {
        let (index, warning) = index::load(&self.storage.storage_dir());
        let remote = self.backend.list()?;
        let names = (!sessions.is_empty()).then_some(sessions);
        let mut rows = scan(self.storage, self.key, &remote, &index, names)
            .map_err(|e| SyncError::Failed(e.to_string()))?;
        let mut report = Report {
            warning,
            ..Report::default()
        };
        if !sessions.is_empty() {
            let (kept, unmatched) = select(rows, self.storage, sessions);
            rows = kept;
            for name in unmatched {
                report.add(Command::Push, not_found(name));
            }
        }
        Ok(Loaded {
            rows,
            index,
            report,
        })
    }

    fn step(
        &self,
        command: Command,
        opts: &Options,
        decision: Decision,
        row: &Row,
        index: &mut Index,
    ) -> Result<Outcome, Stop> {
        Ok(match (decision, command) {
            (Decision::Push, Command::Push) | (Decision::Pull, Command::Pull) if opts.dry_run => {
                Outcome::Planned
            }
            (Decision::Push, Command::Push) => {
                return self.act(push_one(self, row, index, opts.overwrite))
            }
            (Decision::Pull, Command::Pull) => return self.act(pull_one(self, row, index)),
            (Decision::InSync { refresh_index }, _)
                if !opts.dry_run && (refresh_index || self.stale_stamp(row, index)) =>
            {
                self.refresh(row, index)
                    .map_or_else(Outcome::Failed, |()| Outcome::Nothing)
            }
            _ => passive(decision),
        })
    }

    fn item(&self, row: &Row, decision: Decision, outcome: Outcome) -> Item {
        let local = std::fs::metadata(self.storage.storage_dir().join(&row.path))
            .as_ref()
            .map(std::fs::Metadata::len)
            .ok();
        let remote = row.meta.as_ref().map(|m| m.size);
        Item {
            path: row.path.clone(),
            decision,
            outcome,
            size: local.or(remote).unwrap_or(0),
            // a fallback id from the index is not a listed remote copy
            on_remote: row.input.remote != Remote::Absent,
            on_local: local.is_some(),
            local_hash: hash_of(&row.input.local),
            remote_hash: remote_hash_of(&row.input.remote),
            remote_size: remote.unwrap_or(0),
            remote_id: row.remote_id.clone(),
            pending: row.pending,
            duplicates: row.duplicates.clone(),
        }
    }

    fn act(&self, result: Result<u64, Fail>) -> Result<Outcome, Stop> {
        result.map_or_else(Fail::outcome, |bytes| Ok(Outcome::Done(bytes)))
    }

    /// Hashes already agree, only the index lacks the entry.
    /// The index stamp no longer matches the file (old index, or same bytes rewritten),
    /// restamp so the next auto run does not hash it again.
    fn stale_stamp(&self, row: &Row, index: &Index) -> bool {
        let Some(entry) = index.entries.get(&row.path) else {
            return false;
        };
        let now = index::stamp(&self.storage.storage_dir().join(&row.path));
        now != (entry.size, entry.mtime_ns)
    }

    fn refresh(&self, row: &Row, index: &mut Index) -> Result<(), String> {
        let (Local::Hash(hash), Some(id), Some(token)) =
            (&row.input.local, &row.remote_id, &row.token)
        else {
            return Ok(());
        };
        let synced = Synced {
            sha256: hash.clone(),
            remote_id: id.clone(),
            gen: row.gen,
            token: token.clone(),
        };
        record(index, &self.storage.storage_dir(), &row.path, synced);
        save_index(self.storage, index)
    }

    fn delete_one(&self, row: &Row, index: &mut Index) -> Result<Outcome, Stop> {
        let Some(id) = &row.remote_id else {
            return Ok(Outcome::Skipped("not on the remote".into()));
        };
        match self.backend.delete(id) {
            Err(BackendError::RateLimited) => return Err(Stop),
            Err(e) => return Ok(Outcome::Failed(e.to_string())),
            Ok(()) => {}
        }
        index.entries.remove(&row.path);
        Ok(match save_index(self.storage, index) {
            Ok(()) => Outcome::Done(0),
            Err(e) => Outcome::Failed(e),
        })
    }
}
