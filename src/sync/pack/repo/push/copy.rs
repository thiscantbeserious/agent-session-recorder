//! Prune copies: a tombstone written again at the same gen, not a new delete.

use super::stage::now;
use super::{PushError, PushReport};
use crate::sync::pack::repo::Repo;

impl Repo<'_> {
    /// Appends each `(path, gen)` as a tombstone at exactly that gen to the open pack, so the
    /// merge sees one tombstone with several places. A retry keeps the gen.
    pub fn copy_tombstones(&mut self, tombs: &[(String, u64)]) -> Result<PushReport, PushError> {
        let mut report = PushReport::default();
        let res = self.append_tombs(tombs.to_vec(), (&now(), true), &mut report);
        report.open = self.open_ref();
        match res {
            Ok(()) => {
                report.check_failed = self.reload().is_err();
                Ok(report)
            }
            Err(error) => Err(PushError {
                error,
                pushed: report,
            }),
        }
    }
}
