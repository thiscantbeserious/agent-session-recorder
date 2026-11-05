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
    OtherKey,
    UnknownVersion,
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
        (Local::Absent, Remote::Absent) => Decision::InSync {
            refresh_index: false,
        },
        (Local::Absent, Remote::Hash(_)) => Decision::Pull,
        (Local::Hash(_), Remote::Absent) => Decision::Push,
        (Local::Hash(l), Remote::Hash(r)) => compare(l, r, input.last.as_deref()),
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
        (Decision::Conflict, true, Command::Push) => Decision::Push,
        (Decision::Conflict, true, Command::Pull) => Decision::Pull,
        _ => decision,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(s: &str) -> String {
        s.to_string()
    }

    fn input(local: Local, remote: Remote, last: Option<&str>) -> Input {
        Input {
            local,
            remote,
            last: last.map(h),
        }
    }

    fn both(l: &str, r: &str, last: Option<&str>) -> Decision {
        decide(&input(Local::Hash(h(l)), Remote::Hash(h(r)), last))
    }

    #[test]
    fn equal_hashes_are_in_sync() {
        assert_eq!(
            both("a", "a", Some("a")),
            Decision::InSync {
                refresh_index: false
            }
        );
    }

    #[test]
    fn equal_hashes_without_index_refresh_the_index() {
        let expected = Decision::InSync {
            refresh_index: true,
        };
        assert_eq!(both("a", "a", None), expected);
        assert_eq!(both("b", "b", Some("a")), expected);
    }

    #[test]
    fn local_only_pushes() {
        let d = decide(&input(Local::Hash(h("a")), Remote::Absent, None));
        assert_eq!(d, Decision::Push);
    }

    #[test]
    fn remote_only_pulls_also_when_index_remembers_it() {
        let d = decide(&input(Local::Absent, Remote::Hash(h("a")), None));
        assert_eq!(d, Decision::Pull);
        let d = decide(&input(Local::Absent, Remote::Hash(h("a")), Some("a")));
        assert_eq!(d, Decision::Pull);
    }

    #[test]
    fn local_changed_pushes() {
        assert_eq!(both("new", "old", Some("old")), Decision::Push);
    }

    #[test]
    fn remote_changed_pulls() {
        assert_eq!(both("old", "new", Some("old")), Decision::Pull);
    }

    #[test]
    fn both_changed_is_conflict() {
        assert_eq!(both("l", "r", Some("old")), Decision::Conflict);
    }

    #[test]
    fn no_index_and_different_hash_is_conflict() {
        assert_eq!(both("l", "r", None), Decision::Conflict);
    }

    #[test]
    fn mtime_is_not_an_input() {
        // Input has no time field, a newer local file with a changed remote stays a conflict.
        assert_eq!(
            both("local-newer", "remote", Some("old")),
            Decision::Conflict
        );
    }

    #[test]
    fn other_key_and_unknown_version_are_reported() {
        let l = Local::Hash(h("a"));
        assert_eq!(
            decide(&input(l.clone(), Remote::OtherKey, None)),
            Decision::OtherKey
        );
        assert_eq!(
            decide(&input(l, Remote::UnknownVersion, None)),
            Decision::UnknownVersion
        );
        assert_eq!(
            decide(&input(Local::Absent, Remote::OtherKey, None)),
            Decision::OtherKey
        );
    }

    #[test]
    fn local_skips_win_with_their_reason() {
        for reason in [SkipReason::Plain, SkipReason::Locked, SkipReason::OverCap] {
            let d = decide(&input(Local::Skipped(reason), Remote::Hash(h("a")), None));
            assert_eq!(d, Decision::Skip(reason));
        }
    }

    #[test]
    fn overwrite_turns_conflict_into_command_direction() {
        let c = Decision::Conflict;
        assert_eq!(resolve(c, Command::Push, true), Decision::Push);
        assert_eq!(resolve(c, Command::Pull, true), Decision::Pull);
        assert_eq!(resolve(c, Command::Push, false), Decision::Conflict);
    }

    #[test]
    fn overwrite_leaves_other_decisions_alone() {
        for d in [
            Decision::Push,
            Decision::Pull,
            Decision::OtherKey,
            Decision::Skip(SkipReason::Locked),
            Decision::InSync {
                refresh_index: false,
            },
        ] {
            assert_eq!(resolve(d, Command::Pull, true), d);
        }
    }
}
