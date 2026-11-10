//! The read side `agr sync migrate` needs from the old per-session layout.

use super::{parse, GistBackend, Runner, LIST_ENDPOINT};
use crate::sync::backend::SyncBackend;
use crate::sync::store::StoreError as StoreErr;
use std::io::Read;

/// The read side migrate needs, over the same `gh` and `git` calls as the sync backend.
impl<R: Runner> crate::sync::pack::migrate::LegacyRead for GistBackend<R> {
    fn list_all(&self) -> Result<Vec<crate::sync::pack::migrate::LegacyGist>, StoreErr> {
        let out = self
            .run_ok("gh", &["api", LIST_ENDPOINT, "--paginate"], None)
            .map_err(StoreErr::from)?;
        parse::parse_all(&out.stdout).map_err(StoreErr::from)
    }

    fn pull(&self, id: &str) -> Result<(Option<String>, Box<dyn Read>), StoreErr> {
        SyncBackend::pull(self, id).map_err(StoreErr::from)
    }

    fn delete(&self, id: &str) -> Result<(), StoreErr> {
        SyncBackend::delete(self, id).map_err(StoreErr::from)
    }
}
