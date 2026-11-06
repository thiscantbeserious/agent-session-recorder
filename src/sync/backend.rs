//! Backend-neutral transport, backends only ever see opaque tokens and sealed bytes.

use thiserror::Error;

/// One remote session, `meta_token` is the sealed metadata from `crypto::seal_meta`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    pub id: String,
    pub meta_token: String,
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
    #[error("{0}")]
    Other(String),
}

/// Remote store for sealed sessions.
pub trait SyncBackend {
    fn name(&self) -> &'static str;
    /// Tools present, logged in, scope granted.
    fn check(&self) -> Result<(), BackendError>;
    fn list(&self) -> Result<Vec<RemoteEntry>, BackendError>;
    /// Create (`id` is `None`) or replace, returns the id.
    fn push(&self, id: Option<&str>, meta_token: &str, blob: &[u8])
        -> Result<String, BackendError>;
    fn pull(&self, id: &str) -> Result<Vec<u8>, BackendError>;
    fn delete(&self, id: &str) -> Result<(), BackendError>;
}
