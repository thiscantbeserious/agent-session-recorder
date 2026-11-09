//! Item helpers of the engine, pure functions over decisions and rows.

use super::plan::{Decision, Local, Remote, SkipReason};
use super::run::{Item, Outcome};

/// Outcome of a decision that does not act.
pub(super) fn passive(decision: Decision) -> Outcome {
    match decision {
        Decision::Conflict => Outcome::Skipped("conflict, use --overwrite".into()),
        Decision::Skip(reason) => Outcome::Skipped(skip_text(reason).into()),
        Decision::OtherKey => Outcome::Skipped("sealed with another key".into()),
        Decision::UnknownVersion => Outcome::Skipped("unknown remote version".into()),
        Decision::Duplicate => Outcome::Skipped("duplicate remote copies".into()),
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
        remote_id: None,
        pending: false,
        duplicates: Vec::new(),
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
    }
}
