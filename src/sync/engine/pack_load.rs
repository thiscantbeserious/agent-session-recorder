//! The load step of a pack run: local index, repo over the store, rows of local and remote.

use super::fail::sync_error;
use super::index::{self, load_v2, Index, Refused};
use super::item::not_found;
use super::observe::{scan_tick, Step};
use super::pack_run::Engine;
use super::plan::Command;
use super::remote::attach_view;
use super::run::{Report, SyncError};
use super::scan::{local_rows, select, Row};
use crate::sync::pack::merge::View;
use crate::sync::pack::repo::{Repo, RepoError};

/// Everything a run works on after the load.
pub(super) struct Loaded<'a> {
    pub repo: Repo<'a>,
    pub index: Index,
    pub rows: Vec<Row>,
    pub report: Report,
}

fn open_error(e: RepoError) -> SyncError {
    match e {
        RepoError::NeedsMigrate => SyncError::NeedsMigrate,
        RepoError::Store(s) => sync_error(s),
        other => SyncError::Failed(other.to_string()),
    }
}

/// Dead bytes and total bytes over the readable packs of the view.
fn pack_bytes(view: &View) -> (u64, u64) {
    let dead = view.dead_bytes();
    let total = dead.keys().filter_map(|id| view.pack(id)).map(|p| p.len);
    (dead.values().sum(), total.sum())
}

impl<'a> Engine<'a> {
    /// Reads the local index (a v1 file refuses), opens the repo and joins local sessions with
    /// the merged view. Nothing is written.
    pub(super) fn load(
        &self,
        sessions: &[String],
        dry_run: bool,
        observer: &mut dyn FnMut(Step<'_>),
    ) -> Result<Loaded<'a>, SyncError> {
        let dir = self.storage.storage_dir();
        let mut index = load_v2(&dir).map_err(|e| match e {
            Refused::V1 => SyncError::NeedsMigrate,
            Refused::Corrupt(text) => SyncError::Failed(text),
        })?;
        observer(Step::Listing);
        let hint = index.open_pack.as_deref();
        let sweep = !dry_run;
        let repo = Repo::open_with(
            self.store,
            self.key,
            (&self.writer, self.target, hint),
            sweep,
        )
        .map_err(open_error)?;
        index.open_pack = repo.open_pack().map(String::from);
        let names = (!sessions.is_empty()).then_some(sessions);
        let mut progress = |done, total| {
            if scan_tick(done, total) {
                observer(Step::Scanning { done, total });
            }
        };
        let mut rows = local_rows(self.storage, &index, names, &mut progress)
            .map_err(|e| SyncError::Failed(e.to_string()))?;
        attach_view(&mut rows, repo.view(), &index);
        let (dead_bytes, pack_bytes) = pack_bytes(repo.view());
        let mut report = Report {
            dead_bytes,
            pack_bytes,
            ..Report::default()
        };
        let mut rows: Vec<Row> = rows.into_values().collect();
        if !sessions.is_empty() {
            let (kept, unmatched) = select(rows, self.storage, sessions);
            rows = kept;
            for name in unmatched {
                report.add(Command::Push, not_found(name));
            }
        }
        Ok(Loaded {
            repo,
            index,
            rows,
            report,
        })
    }

    /// The one place a pack run writes the local index.
    pub(super) fn save(&self, index: &Index) -> Result<(), SyncError> {
        index::save(&self.storage.storage_dir(), index)
            .map_err(|e| SyncError::Failed(format!("could not save the sync index: {e}")))
    }
}
