//! Pack store trait: a dumb store of logical byte streams plus one sealed index text each.

use std::fmt;
use std::io::Read;

#[cfg(test)]
#[macro_use]
mod contract;
pub(super) mod dir;
pub(super) mod memory;

pub use crate::sync::backend::BlobSource;

/// Store failures, one clear case each so commands print one line.
#[derive(Clone, PartialEq, Eq)]
pub enum StoreError {
    NotInstalled(&'static str),
    NotLoggedIn,
    MissingScope,
    Unreachable(String),
    RateLimited,
    NotFound,
    /// The pack moved on, carries the index text at the current remote head, never printed.
    Moved(String),
    Other(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInstalled(tool) => write!(f, "{tool} is not installed or not on PATH"),
            Self::NotLoggedIn => f.write_str("not logged in, run `gh auth login`"),
            Self::MissingScope => {
                f.write_str("the gh token has no `gist` scope, run `gh auth refresh -s gist`")
            }
            Self::Unreachable(why) => write!(f, "remote not reachable: {why}"),
            Self::RateLimited => f.write_str("GitHub rate limit reached, run again later"),
            Self::NotFound => f.write_str("remote pack not found"),
            Self::Moved(_) => f.write_str("changed on the remote, run again"),
            Self::Other(msg) => f.write_str(msg),
        }
    }
}

/// Hand written so `Moved` never prints the index text it carries.
impl fmt::Debug for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInstalled(t) => f.debug_tuple("NotInstalled").field(t).finish(),
            Self::NotLoggedIn => f.write_str("NotLoggedIn"),
            Self::MissingScope => f.write_str("MissingScope"),
            Self::Unreachable(w) => f.debug_tuple("Unreachable").field(w).finish(),
            Self::RateLimited => f.write_str("RateLimited"),
            Self::NotFound => f.write_str("NotFound"),
            Self::Moved(_) => f.write_str("Moved(..)"),
            Self::Other(m) => f.debug_tuple("Other").field(m).finish(),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<crate::sync::backend::gist::pace::ClockError> for StoreError {
    fn from(e: crate::sync::backend::gist::pace::ClockError) -> Self {
        Self::Other(e.to_string())
    }
}

/// One pack as listed, `id` and `tag` are opaque to the pack layer and the store respectively.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackInfo {
    pub id: String,
    pub tag: String,
    /// Logical pack length, sum of all parts.
    pub len: u64,
    pub updated_at: String,
}

/// One listing pass: the packs plus the descriptions of old per-session entries, unopened.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    pub packs: Vec<PackInfo>,
    pub legacy: Vec<String>,
}

/// One sealed session in an append, written as one contiguous range of the pack stream.
pub struct Segment<'a> {
    pub len: u64,
    pub write: BlobSource<'a>,
}

pub trait PackStore {
    fn name(&self) -> &'static str;
    fn check(&self) -> Result<(), StoreError>;
    fn list(&self) -> Result<Listing, StoreError>;
    /// The sealed index text of the pack, as stored by the last create or append.
    fn read_index(&self, id: &str) -> Result<String, StoreError>;
    /// New pack with no data, `tag` and `index`, returns its id.
    fn create(&self, tag: &str, index: &str) -> Result<String, StoreError>;
    /// Appends `segments` behind the current end and replaces the index, all or nothing.
    /// Refused with `Moved(current)` when the pack length is not `expected_len` or the current
    /// index text is not `expected_index` byte for byte, nothing is written. Zero segments is valid.
    fn append(
        &self,
        id: &str,
        expected_len: u64,
        expected_index: &str,
        segments: &[Segment],
        index: &str,
    ) -> Result<(), StoreError>;
    /// Whether one more append with these segment lengths fits the pack (file slots on gists).
    fn fits(&self, _info: &PackInfo, _lens: &[u64]) -> bool {
        true
    }
    /// Streams the logical range `offset..offset + len` of the pack.
    fn read(&self, id: &str, offset: u64, len: u64) -> Result<Box<dyn Read>, StoreError>;
    fn delete(&self, id: &str) -> Result<(), StoreError>;
}

#[cfg(test)]
mod tests {
    use super::StoreError;

    #[test]
    fn store_error_messages_name_the_next_step() {
        assert_eq!(
            StoreError::Moved("idx".into()).to_string(),
            "changed on the remote, run again"
        );
        let msg = StoreError::RateLimited.to_string();
        assert!(msg.contains("run again later"), "{msg}");
        let msg = StoreError::NotFound.to_string();
        assert!(msg.contains("not found"), "{msg}");
    }

    #[test]
    fn debug_of_moved_hides_the_index() {
        let shown = format!("{:?}", StoreError::Moved("secret".into()));
        assert!(!shown.contains("secret"), "{shown}");
    }
}
