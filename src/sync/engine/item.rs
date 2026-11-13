//! Item helpers of the engine, pure functions over decisions and rows.

use super::observe::Step;
use super::plan::{Command, Decision, Local, Remote, SkipReason};
use super::run::{Item, Outcome, Report};
use super::scan::Row;
use crate::StorageManager;

impl Report {
    /// Add the counts and items of a later round of the same run.
    pub(super) fn absorb(&mut self, other: Report) {
        self.pushed += other.pushed;
        self.skipped += other.skipped;
        self.conflicts += other.conflicts;
        self.failed += other.failed;
        self.bytes += other.bytes;
        self.warning = self.warning.take().or(other.warning);
        self.unchecked |= other.unchecked;
        self.forks.extend(other.forks);
        self.refused.extend(other.refused);
        self.items.extend(other.items);
    }

    pub(super) fn add(&mut self, command: Command, item: Item) {
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

/// Outcome of a decision that does not act.
pub(super) fn passive(decision: Decision) -> Outcome {
    match decision {
        Decision::Conflict => Outcome::Skipped("conflict, use --overwrite".into()),
        Decision::Skip(reason) => Outcome::Skipped(skip_text(reason).into()),
        Decision::OtherKey => Outcome::Skipped("sealed with another key".into()),
        Decision::UnknownVersion => Outcome::Skipped("unknown remote version".into()),
        Decision::Duplicate => Outcome::Skipped("duplicate remote copies".into()),
        Decision::Fork => Outcome::Skipped("fork, run `agr sync push --overwrite`".into()),
        _ => Outcome::Nothing,
    }
}

pub(super) fn not_found(name: String) -> Item {
    Item {
        path: name,
        decision: Decision::Skip(SkipReason::NotFound),
        outcome: Outcome::Failed("no such session".into()),
        size: 0,
        on_remote: false,
        on_local: false,
        local_hash: None,
        remote_hash: None,
        remote_size: 0,
    }
}

/// Delete counts in `deleted`, not in `pushed`.
pub(super) fn strip_done(item: Item) -> Item {
    match item.outcome {
        Outcome::Done(_) => Item {
            outcome: Outcome::Nothing,
            ..item
        },
        _ => item,
    }
}

pub(super) fn hash_of(local: &Local) -> Option<String> {
    match local {
        Local::Hash(h) => Some(h.clone()),
        _ => None,
    }
}

pub(super) fn remote_hash_of(remote: &Remote) -> Option<String> {
    match remote {
        Remote::Hash(h) => Some(h.clone()),
        _ => None,
    }
}

pub(super) fn skip_text(reason: SkipReason) -> &'static str {
    match reason {
        SkipReason::Plain => "plain .cast, run `agr compress` first",
        SkipReason::Locked => "still recording",
        SkipReason::OverCap => "over the 1 GiB limit",
        SkipReason::Unreadable => "cannot read the file",
        SkipReason::NotFound => "no such session",
        SkipReason::DeletedOnRemote => {
            "deleted on the remote, run `agr sync push --overwrite` to restore"
        }
    }
}

/// Tells the observer which session is about to move and how big it is.
pub(super) fn announce(
    storage: &StorageManager,
    row: &Row,
    observer: &mut dyn FnMut(Step<'_>),
    pull: bool,
) {
    let remote = row.meta.as_ref().map(|m| m.size);
    let local = std::fs::metadata(storage.storage_dir().join(&row.path))
        .map(|m| m.len())
        .ok();
    let size = if pull {
        remote.or(local)
    } else {
        local.or(remote)
    };
    observer(Step::Started {
        path: &row.path,
        size: size.unwrap_or(0),
    });
}

pub(super) fn build_item(
    storage: &StorageManager,
    row: &Row,
    decision: Decision,
    outcome: Outcome,
) -> Item {
    let local = std::fs::metadata(storage.storage_dir().join(&row.path))
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
        on_remote: !matches!(row.input.remote, Remote::Absent | Remote::Deleted { .. }),
        on_local: local.is_some(),
        local_hash: hash_of(&row.input.local),
        remote_hash: remote_hash_of(&row.input.remote),
        remote_size: remote.unwrap_or(0),
    }
}
