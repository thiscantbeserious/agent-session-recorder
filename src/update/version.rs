//! Version parsing and comparison, plain `MAJOR.MINOR.PATCH` as three numbers.

use anyhow::{anyhow, bail, Result};
use std::fmt;

/// A release version, ordered numerically field by field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u64, pub u64, pub u64);

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// Parse `0.4.0` or `v0.4.0`. Anything else is an error.
pub fn parse(text: &str) -> Result<Version> {
    let core = text.strip_prefix('v').unwrap_or(text);
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        bail!("not a MAJOR.MINOR.PATCH version: {text:?}");
    }
    let num = |s: &str| {
        s.parse::<u64>()
            .map_err(|_| anyhow!("bad version {text:?}"))
    };
    Ok(Version(num(parts[0])?, num(parts[1])?, num(parts[2])?))
}

/// Parse the output of `agr --version`, for example `agr 0.4.0-dev+abc1234 (x, built y)`.
pub fn parse_version_output(output: &str) -> Result<Version> {
    let word = output
        .trim()
        .strip_prefix("agr ")
        .and_then(|rest| rest.split_whitespace().next())
        .ok_or_else(|| anyhow!("not agr version output: {:?}", output.trim()))?;
    let core = word.split(['-', '+']).next().unwrap_or(word);
    parse(core)
}
