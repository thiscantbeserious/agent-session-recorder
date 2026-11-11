//! Push and delete of the pack engine: one append per push, one tombstone append per delete.

use std::collections::BTreeMap;

use super::index::record_synced;
use super::item::{build_item, strip_done};
use super::lock::SyncLock;
use super::observe::{counted, no_progress, Observer, Step};
use super::pack_load::Loaded;
use super::pack_run::Engine;
use super::plan::{decide, Command, Local};
use super::queue::Held;
use super::run::{Options, Outcome, Report, SyncError};
use super::scan::Row;
use crate::sync::pack::repo::{Outcome as Landed, PushError, PushItem, PushReport};
use crate::sync::store::StoreError;

/// Report text when somebody else's data was ahead when the append was planned.
const CHANGED: &str = "changed on the remote, run again";

/// Outcome per path of one push.
pub(super) type Landings = BTreeMap<String, Outcome>;

impl Engine<'_> {
    pub(super) fn push_item(&self, row: &Row, known_gen: u64) -> PushItem {
        PushItem {
            path: row.path.clone(),
            gen: row.gen.max(known_gen) + 1,
            file: self.storage.storage_dir().join(&row.path),
        }
    }

    /// One append for the whole batch, the index is saved here, also on a failure, so what landed
    /// is never pushed twice.
    pub(super) fn push_batch(
        &self,
        items: &[PushItem],
        rows: &[&Row],
        loaded: &mut Loaded<'_>,
    ) -> Result<Landings, SyncError> {
        if items.is_empty() {
            return Ok(Landings::new());
        }
        let (landed, failure) = match loaded.repo.push_batch(items) {
            Ok(report) => (report, None),
            Err(PushError { error, pushed }) => (pushed, Some(error)),
        };
        let mut out = self.record_push(&landed, rows, loaded);
        self.save(&loaded.index)?;
        if let Some(error) = failure {
            return self.stopped(error, items, &mut out);
        }
        Ok(out)
    }

    /// Index entries and outcomes of what landed, forks and open pack of the report.
    fn record_push(&self, landed: &PushReport, rows: &[&Row], loaded: &mut Loaded<'_>) -> Landings {
        let dir = self.storage.storage_dir();
        let mut out = Landings::new();
        for (path, gen, outcome) in &landed.items {
            let row = rows.iter().find(|r| &r.path == path);
            let verdict = match (outcome, row) {
                (Landed::Pushed { .. }, Some(row)) => {
                    let sha = pushed_sha(loaded, row, *gen);
                    record_synced(&mut loaded.index, &dir, path, (&sha, *gen));
                    Outcome::Done(std::fs::metadata(dir.join(path)).map_or(0, |m| m.len()))
                }
                _ => Outcome::Failed(CHANGED.into()),
            };
            out.insert(path.clone(), verdict);
        }
        loaded.index.open_pack = landed.open.as_ref().map(|o| o.id.clone());
        loaded.report.forks.extend(landed.forks.iter().cloned());
        if landed.check_failed {
            loaded.report.unchecked = true;
            loaded.report.warning = Some("pushed, could not check for forks".into());
        }
        out
    }

    /// A push that stopped halfway: a rate limit ends the run, any other error fails the rest.
    fn stopped(
        &self,
        error: StoreError,
        items: &[PushItem],
        out: &mut Landings,
    ) -> Result<Landings, SyncError> {
        let left = items.iter().filter(|i| !out.contains_key(&i.path)).count();
        if error == StoreError::RateLimited {
            return Err(SyncError::RateLimited { left });
        }
        let open: Vec<&PushItem> = items
            .iter()
            .filter(|i| !out.contains_key(&i.path))
            .collect();
        for item in open {
            out.insert(item.path.clone(), Outcome::Failed(error.to_string()));
        }
        Ok(std::mem::take(out))
    }

    pub fn delete(&self, sessions: &[String]) -> Result<Report, SyncError> {
        self.delete_with(sessions, 0, &mut no_progress)
    }

    /// One tombstone per session that is on the remote, in one append. Local files stay.
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
        let mut loaded = self.load(sessions, &mut observer)?;
        let rows = std::mem::take(&mut loaded.rows);
        let paths: Vec<String> = (rows.iter().filter(|r| on_remote(r)))
            .map(|r| r.path.clone())
            .collect();
        let done = self.tombstones(&paths, &mut loaded)?;
        for row in &rows {
            let outcome = match done.get(&row.path) {
                Some(outcome) => outcome.clone(),
                None => Outcome::Skipped("not on the remote".into()),
            };
            let moved = matches!(outcome, Outcome::Done(_));
            let item = build_item(self.storage, row, decide(&row.input), outcome);
            observer(Step::Finished(&item));
            loaded.report.deleted += usize::from(moved);
            loaded.report.add(Command::Push, strip_done(item));
        }
        Ok(loaded.report)
    }

    fn tombstones(&self, paths: &[String], loaded: &mut Loaded<'_>) -> Result<Landings, SyncError> {
        if paths.is_empty() {
            return Ok(Landings::new());
        }
        let (landed, failure) = match loaded.repo.tombstone_batch(paths) {
            Ok(report) => (report, None),
            Err(PushError { error, pushed }) => (pushed, Some(error)),
        };
        let mut out = landed_tombstones(&landed, loaded);
        self.save(&loaded.index)?;
        match failure {
            Some(error) => stop_rest(error, paths, &mut out),
            None => Ok(out),
        }
    }
}

/// Outcome per tombstone that was reached. The index entry of a landed one is dropped only when
/// the merged view agrees it is deleted, a rival live copy at the same gen is a fork.
fn landed_tombstones(landed: &PushReport, loaded: &mut Loaded<'_>) -> Landings {
    let mut out = Landings::new();
    for (path, _, outcome) in &landed.items {
        let verdict = match outcome {
            Landed::Pushed { .. } if still_live(landed, loaded, path) => {
                Outcome::Failed("a copy is still live on the remote".into())
            }
            Landed::Pushed { .. } => {
                loaded.index.entries.remove(path);
                Outcome::Done(0)
            }
            Landed::Superseded => Outcome::Failed(CHANGED.into()),
        };
        out.insert(path.clone(), verdict);
    }
    loaded.index.open_pack = landed.open.as_ref().map(|o| o.id.clone());
    if landed.check_failed {
        loaded.report.unchecked = true;
        loaded.report.warning = Some("deleted, could not check for forks".into());
    }
    out
}

/// The merged view after the tombstone still has a live top or a fork for `path`. Nothing to
/// compare with when the reload failed, that case is reported as unchecked.
fn still_live(landed: &PushReport, loaded: &Loaded<'_>, path: &str) -> bool {
    use crate::sync::pack::merge::Top;
    if landed.check_failed {
        return false;
    }
    let view = loaded.repo.view();
    let deleted = matches!(view.get(path).map(|m| &m.top), Some(Top::Deleted { .. }));
    !deleted || view.forks().iter().any(|f| f == path)
}

/// A delete that stopped halfway: a rate limit ends the run, any other error fails the rest.
fn stop_rest(
    error: StoreError,
    paths: &[String],
    out: &mut Landings,
) -> Result<Landings, SyncError> {
    let open: Vec<&String> = paths.iter().filter(|p| !out.contains_key(*p)).collect();
    if error == StoreError::RateLimited {
        return Err(SyncError::RateLimited { left: open.len() });
    }
    for path in open {
        out.insert(path.clone(), Outcome::Failed(error.to_string()));
    }
    Ok(std::mem::take(out))
}

/// The remote holds a live copy (or a fork) of the row's session.
fn on_remote(row: &Row) -> bool {
    use super::plan::Remote;
    matches!(
        row.input.remote,
        Remote::Hash(_) | Remote::Fork | Remote::Duplicate
    )
}

/// The hash the append wrote: the merged view after the push when it still holds our gen with a
/// live top, else the planned local hash. A rival at a higher gen is never recorded as ours.
fn pushed_sha(loaded: &Loaded<'_>, row: &Row, gen: u64) -> String {
    use crate::sync::pack::merge::Top;
    let at_gen = loaded.repo.view().get(&row.path).filter(|m| m.gen == gen);
    match (at_gen.map(|m| &m.top), &row.input.local) {
        (Some(Top::Live { sha256 }), _) => sha256.clone(),
        (_, Local::Hash(h)) => h.clone(),
        _ => String::new(),
    }
}

impl Held for Engine<'_> {
    fn storage_dir(&self) -> std::path::PathBuf {
        self.storage.storage_dir()
    }

    fn push_held(
        &self,
        opts: &Options,
        observer: &mut dyn FnMut(Step<'_>),
    ) -> Result<Report, SyncError> {
        self.sync_held(Command::Push, opts, observer)
    }
}
