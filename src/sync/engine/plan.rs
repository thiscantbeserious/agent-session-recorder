//! Pure decision table, three-way compare of local hash, remote hash and last synced hash.
//! File mtime is never an input.

/// Why a local session is not synced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    Plain,
    Locked,
    OverCap,
    Unreadable,
    /// A requested session name matched nothing.
    NotFound,
    /// The remote holds a tombstone, only `push --overwrite` restores it.
    DeletedOnRemote,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Local {
    Absent,
    Hash(String),
    Skipped(SkipReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Remote {
    Absent,
    Hash(String),
    OtherKey,
    UnknownVersion,
    /// Several remote entries for one path with different content.
    Duplicate,
    /// A tombstone at the top gen of the merged view.
    Deleted {
        gen: u64,
    },
    /// Several hashes at the top gen of the merged view.
    Fork,
}

/// Facts about one `agent/name.cast.gz` path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    pub local: Local,
    pub remote: Remote,
    /// Hash recorded in the index at the last successful sync.
    pub last: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Equal hashes, `refresh_index` when the index lacks or has a stale entry.
    InSync {
        refresh_index: bool,
    },
    Push,
    Pull,
    Conflict,
    /// Several versions at the top gen, resolved only by `push --overwrite`.
    Fork,
    OtherKey,
    UnknownVersion,
    /// Never acted on, also not with `--overwrite`.
    Duplicate,
    Skip(SkipReason),
}

/// Which command runs, decides what `--overwrite` means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Push,
    Pull,
}

/// Decide one path.
pub fn decide(input: &Input) -> Decision {
    match (&input.local, &input.remote) {
        (Local::Skipped(reason), _) => Decision::Skip(*reason),
        (_, Remote::OtherKey) => Decision::OtherKey,
        (_, Remote::UnknownVersion) => Decision::UnknownVersion,
        (_, Remote::Duplicate) => Decision::Duplicate,
        (_, Remote::Fork) => Decision::Fork,
        (Local::Hash(l), Remote::Deleted { .. }) => deleted(l, input.last.as_deref()),
        (Local::Absent, Remote::Absent | Remote::Deleted { .. }) => Decision::InSync {
            refresh_index: false,
        },
        (Local::Absent, Remote::Hash(_)) => Decision::Pull,
        (Local::Hash(_), Remote::Absent) => Decision::Push,
        (Local::Hash(l), Remote::Hash(r)) => compare(l, r, input.last.as_deref()),
    }
}

/// A tombstone on top: an edit after the last sync is a fork, anything else waits for `--overwrite`.
fn deleted(local: &str, last: Option<&str>) -> Decision {
    match last {
        Some(last) if last != local => Decision::Fork,
        _ => Decision::Skip(SkipReason::DeletedOnRemote),
    }
}

/// Three-way compare when both sides exist.
fn compare(local: &str, remote: &str, last: Option<&str>) -> Decision {
    if local == remote {
        return Decision::InSync {
            refresh_index: last != Some(local),
        };
    }
    match last {
        Some(last) if remote == last => Decision::Push,
        Some(last) if local == last => Decision::Pull,
        _ => Decision::Conflict,
    }
}

/// Apply `--overwrite`, a conflict becomes the command's direction, nothing else changes.
pub fn resolve(decision: Decision, command: Command, overwrite: bool) -> Decision {
    match (decision, overwrite, command) {
        (Decision::Fork | Decision::Skip(SkipReason::DeletedOnRemote), true, Command::Push) => {
            Decision::Push
        }
        (Decision::Conflict, true, Command::Push) => Decision::Push,
        (Decision::Conflict, true, Command::Pull) => Decision::Pull,
        _ => decision,
    }
}
