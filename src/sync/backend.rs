//! Backend-neutral transport, backends only ever see opaque tokens and sealed bytes.

use thiserror::Error;

/// One remote session, `meta_token` is the sealed metadata from `crypto::seal_meta`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    pub id: String,
    pub meta_token: String,
    /// Server creation time, sortable text, the oldest entry of a path wins later.
    pub created_at: String,
}

/// Backend failures, one clear case each so commands print one line.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum BackendError {
    #[error("{0} is not installed or not on PATH")]
    NotInstalled(&'static str),
    #[error("not logged in, run `gh auth login`")]
    NotLoggedIn,
    #[error("the gh token has no `gist` scope, run `gh auth refresh -s gist`")]
    MissingScope,
    #[error("remote not reachable: {0}")]
    Unreachable(String),
    #[error("GitHub rate limit reached")]
    RateLimited,
    /// The stored manifest differs from the one the push expected, carries the current one.
    /// A rejected `git push` (the gist moved on during the push) is `Changed(None)`.
    #[error("the remote changed since it was planned")]
    Changed(Option<String>),
    #[error("remote entry not found")]
    NotFound,
    #[error("{0}")]
    Other(String),
}

/// Remote store for sealed sessions.
pub trait SyncBackend {
    fn name(&self) -> &'static str;
    /// Tools present, logged in, scope granted.
    fn check(&self) -> Result<(), BackendError>;
    fn list(&self) -> Result<Vec<RemoteEntry>, BackendError>;
    /// New pending entry, not listed until `describe`, returns the id.
    fn create(&self) -> Result<String, BackendError>;
    /// Compare-and-swap, stores `manifest` and `blob` only when the stored manifest equals
    /// `expected` (`None` for a new entry), else `Changed(Some(current))` and nothing is written.
    fn push(
        &self,
        id: &str,
        expected: Option<&str>,
        manifest: &str,
        blob: &[u8],
    ) -> Result<(), BackendError>;
    /// Set the listing hint to `token`, makes a pending entry visible.
    fn describe(&self, id: &str, token: &str) -> Result<(), BackendError>;
    /// Stored manifest (`None` when there is none) and blob, read together.
    fn pull(&self, id: &str) -> Result<(Option<String>, Vec<u8>), BackendError>;
    fn delete(&self, id: &str) -> Result<(), BackendError>;
}
