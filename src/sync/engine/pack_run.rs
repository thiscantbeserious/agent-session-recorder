//! Sync engine on a `Repo`, push, pull, delete and status against the merged pack view.

use super::item::{announce, announce_batch, build_item, passive};
use super::lock::SyncLock;
use super::observe::{counted, no_progress, Observer, Step};
use super::pack_load::Loaded;
use super::pack_pull::{needs_refresh, pull_one, refresh, PullFail};
use super::plan::{decide, resolve, Command, Decision, Local};
use super::queue;
use super::run::{Options, Outcome, Report, SyncError};
use super::scan::Row;
use crate::sync::crypto::Key;
use crate::sync::pack::repo::PushItem;
use crate::sync::store::PackStore;
use crate::StorageManager;

/// Engine over any pack store, the key, the storage dir and the writer id of this machine.
pub struct Engine<'a> {
    pub store: &'a dyn PackStore,
    pub key: &'a Key,
    pub storage: &'a StorageManager,
    pub writer: String,
    /// Target pack size in bytes.
    pub target: u64,
}

impl<'a> Engine<'a> {
    pub fn new(
        store: &'a dyn PackStore,
        key: &'a Key,
        storage: &'a StorageManager,
        writer: &str,
        target: u64,
    ) -> Self {
        Self {
            store,
            key,
            storage,
            writer: writer.to_string(),
            target,
        }
    }

    /// Decisions only, never changes anything.
    pub fn status(&self, sessions: &[String]) -> Result<Report, SyncError> {
        self.status_with(sessions, &mut no_progress)
    }

    /// Like `status`, the observer gets the load events.
    pub fn status_with(
        &self,
        sessions: &[String],
        observer: &mut Observer<'_>,
    ) -> Result<Report, SyncError> {
        let opts = Options {
            dry_run: true,
            sessions: sessions.to_vec(),
            ..Options::default()
        };
        let mut report = self.sync(Command::Push, &opts, &mut counted(0, observer))?;
        report.queued = queue::list(&self.storage.storage_dir()).len();
        Ok(report)
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

    /// The `--auto` run, see `queue::drain`.
    pub fn drain_auto(
        &self,
        session: &str,
        observer: &mut Observer<'_>,
    ) -> Result<Report, SyncError> {
        queue::drain(self, session, observer)
    }

    fn sync(
        &self,
        command: Command,
        opts: &Options,
        observer: &mut dyn FnMut(Step<'_>),
    ) -> Result<Report, SyncError> {
        let lock = if opts.dry_run {
            None
        } else {
            Some(SyncLock::acquire(&self.storage.storage_dir())?)
        };
        let report = self.sync_held(command, opts, observer);
        drop(lock);
        // an auto child that found the lock held exited, its queued session is ours to push
        let queued = queue::list(&self.storage.storage_dir());
        if !opts.dry_run && !queued.is_empty() {
            // ponytail: a failed drain keeps its entries queued for the next trigger
            let _ = queue::drain(self, &queued[0], &mut no_progress);
        }
        report
    }

    /// `sync` for a caller that already holds the lock (or runs dry).
    pub(super) fn sync_held(
        &self,
        command: Command,
        opts: &Options,
        observer: &mut dyn FnMut(Step<'_>),
    ) -> Result<Report, SyncError> {
        let mut loaded = self.load(&opts.sessions, opts.dry_run, observer)?;
        let rows = std::mem::take(&mut loaded.rows);
        let run = match command {
            Command::Push => self.push_rows(rows, opts, &mut loaded, observer),
            Command::Pull => self.pull_rows(rows, opts, &mut loaded, observer),
        };
        // saved and settled also when the run stopped, what finished is never done twice
        if !opts.dry_run {
            self.save(&loaded.index)?;
        }
        if command == Command::Push && !opts.dry_run {
            let dir = self.storage.storage_dir();
            loaded
                .report
                .items
                .iter()
                .for_each(|i| queue::settle(&dir, i));
        }
        run?;
        Ok(loaded.report)
    }

    fn decide_row(&self, row: &Row, command: Command, opts: &Options) -> Decision {
        let decision = decide(&row.input);
        // A fork with no local file has nothing to push, `--overwrite` leaves it a fork.
        if decision == Decision::Fork && row.input.local == Local::Absent {
            return decision;
        }
        resolve(decision, command, opts.overwrite)
    }

    fn pull_rows(
        &self,
        rows: Vec<Row>,
        opts: &Options,
        loaded: &mut Loaded<'_>,
        observer: &mut dyn FnMut(Step<'_>),
    ) -> Result<(), SyncError> {
        let mut rows = rows.into_iter();
        while let Some(row) = rows.next() {
            let decision = self.decide_row(&row, Command::Pull, opts);
            let outcome = self.pull_step(&row, decision, opts, loaded, observer);
            let outcome = outcome.map_err(|_| SyncError::RateLimited {
                left: rows.len() + 1,
            })?;
            let item = build_item(self.storage, &row, decision, outcome);
            observer(Step::Finished(&item));
            loaded.report.add(Command::Pull, item);
        }
        Ok(())
    }

    fn pull_step(
        &self,
        row: &Row,
        decision: Decision,
        opts: &Options,
        loaded: &mut Loaded<'_>,
        observer: &mut dyn FnMut(Step<'_>),
    ) -> Result<Outcome, PullFail> {
        Ok(match decision {
            Decision::Pull if opts.dry_run => Outcome::Planned,
            Decision::Pull => {
                announce(self.storage, row, observer, true);
                match pull_one((&loaded.repo, self.storage), row, &mut loaded.index) {
                    Ok(bytes) => Outcome::Done(bytes),
                    Err(PullFail::Stop) => return Err(PullFail::Stop),
                    Err(PullFail::Refused) => {
                        loaded.report.refused.push(row.path.clone());
                        Outcome::Failed(PullFail::Refused.text())
                    }
                    Err(other) => Outcome::Failed(other.text()),
                }
            }
            Decision::InSync { refresh_index } if !opts.dry_run => {
                self.refresh_row(row, refresh_index, loaded)
            }
            other => passive(other),
        })
    }

    /// Writes or drops the index entry of an in-sync row.
    fn refresh_row(&self, row: &Row, refresh_index: bool, loaded: &mut Loaded<'_>) -> Outcome {
        if refresh_index || needs_refresh(row, &loaded.index) {
            refresh(row, &mut loaded.index);
        }
        Outcome::Nothing
    }

    fn push_rows(
        &self,
        rows: Vec<Row>,
        opts: &Options,
        loaded: &mut Loaded<'_>,
        observer: &mut dyn FnMut(Step<'_>),
    ) -> Result<(), SyncError> {
        let (batch, outcomes) = self.plan_push(rows, opts, loaded);
        let pushed: Vec<&Row> = outcomes
            .iter()
            .filter(|(_, d, _)| *d == Decision::Push && !opts.dry_run)
            .map(|(r, _, _)| r)
            .collect();
        if !pushed.is_empty() {
            announce_batch(self.storage, &pushed, observer);
        }
        let landed = self.push_batch(&batch, &pushed, loaded)?;
        for (row, decision, planned) in outcomes {
            let outcome = landed.get(&row.path).cloned().unwrap_or(planned);
            let item = build_item(self.storage, &row, decision, outcome);
            observer(Step::Finished(&item));
            loaded.report.add(Command::Push, item);
        }
        Ok(())
    }

    /// The items to append and the planned outcome of every row, in-sync rows refresh their entry.
    fn plan_push(
        &self,
        rows: Vec<Row>,
        opts: &Options,
        loaded: &mut Loaded<'_>,
    ) -> (Vec<PushItem>, Vec<(Row, Decision, Outcome)>) {
        let (mut batch, mut outcomes) = (Vec::new(), Vec::new());
        for row in rows {
            let decision = self.decide_row(&row, Command::Push, opts);
            let outcome = match decision {
                Decision::Push if opts.dry_run => Outcome::Planned,
                Decision::Push => {
                    let known = loaded.index.entries.get(&row.path).map_or(0, |e| e.gen);
                    batch.push(self.push_item(&row, known));
                    Outcome::Nothing
                }
                Decision::InSync { refresh_index } if !opts.dry_run => {
                    self.refresh_row(&row, refresh_index, loaded)
                }
                other => passive(other),
            };
            outcomes.push((row, decision, outcome));
        }
        (batch, outcomes)
    }
}
