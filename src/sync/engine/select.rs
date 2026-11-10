//! Pattern selection for `agr sync delete`, pure: `*` and `?` against the live remote paths.

use std::fmt;

/// A pattern (or `--all`) that selected nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectError {
    NoMatch(String),
}

impl fmt::Display for SelectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self::NoMatch(pattern) = self;
        write!(f, "no remote session matches `{pattern}`")
    }
}

/// The live remote paths `patterns` select, sorted, each once. Every pattern must match
/// something, `all` selects every path of `remote`.
pub fn select_remote(
    patterns: &[String],
    all: bool,
    remote: &[String],
) -> Result<Vec<String>, SelectError> {
    let _ = (patterns, all, remote);
    Ok(Vec::new())
}
