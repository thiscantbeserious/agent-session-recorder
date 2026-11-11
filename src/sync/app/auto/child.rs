//! The `--auto` child run and the error line it leaves in the state file.

use crate::config::state::State;
use crate::sync::engine::pack_run::Engine;
use crate::sync::engine::run::{Outcome, Report, SyncError};

/// One line for the state file after an `--auto` run for `session`, `None` when it
/// went fine. A busy lock is not an error, the session is queued and the holder takes it.
pub fn error_line(result: &Result<Report, SyncError>) -> Option<String> {
    let report = match result {
        Err(SyncError::Busy(_)) => return None,
        Err(e) => return Some(format!("auto push failed: {e}")),
        Ok(report) => report,
    };
    let found = report.items.iter().find(|i| {
        matches!(i.outcome, Outcome::Failed(_)) || matches!(i.outcome, Outcome::Skipped(_))
    });
    let Some(item) = found else {
        let warning = report.warning.as_deref().unwrap_or("unchecked");
        return report
            .unchecked
            .then(|| format!("auto push could not verify: {warning}"));
    };
    let (Outcome::Failed(why) | Outcome::Skipped(why)) = &item.outcome else {
        return None;
    };
    Some(format!("auto push of {} failed: {why}", item.path))
}

/// The stored error as a warning line, clearing it from `state` (caller saves).
pub fn take_error_line(state: &mut State) -> Option<String> {
    let error = state.last_auto_push_error.take()?;
    Some(format!(
        "Last automatic push failed: {error} (see `agr sync status`)"
    ))
}

/// The `--auto` run, drains the queue (this session included) and the synced sessions that
/// changed since (markers, optimize), never pulls or deletes, a conflict is skipped (no
/// overwrite) and reported.
// ponytail: only the same path with other content counts as changed. A renamed file is a new
// path with no index entry and its old path is gone, so a rename is left to a manual
// `agr sync push` (pushing it here would upload a second copy). Selection is size and mtime from
// the index, so only those files get hashed, a touched but identical file is hashed each time.
pub fn run(engine: &Engine<'_>, session: &str) -> Result<Report, SyncError> {
    engine.drain_auto(session, &mut |_: &crate::sync::Event<'_>| {})
}
