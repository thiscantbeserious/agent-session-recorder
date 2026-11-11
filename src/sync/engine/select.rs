//! Pattern selection for `agr sync delete`, pure: `*` and `?` against the live remote paths.

use std::collections::BTreeSet;
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
    let mut hits = BTreeSet::new();
    if all {
        hits.extend(remote.iter().cloned());
    }
    for pattern in patterns {
        let found: Vec<&String> = remote.iter().filter(|p| matches(pattern, p)).collect();
        if found.is_empty() {
            return Err(SelectError::NoMatch(pattern.clone()));
        }
        hits.extend(found.into_iter().cloned());
    }
    Ok(hits.into_iter().collect())
}

/// `*` is any run of characters (also `/`), `?` is exactly one, everything else is literal.
pub fn matches(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        match p.get(pi) {
            Some('*') => {
                star = Some((pi, ti));
                pi += 1;
            }
            Some(&c) if c == '?' || c == t[ti] => {
                pi += 1;
                ti += 1;
            }
            _ => match star {
                // ponytail: greedy backtrack to the last star, O(n*m) worst case on paths
                Some((sp, st)) => {
                    pi = sp + 1;
                    ti = st + 1;
                    star = Some((sp, st + 1));
                }
                None => return false,
            },
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}
