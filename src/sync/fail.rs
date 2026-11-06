//! Why one push or pull did not finish.

use super::backend::BackendError;
use super::run::Outcome;

/// Error text for a `map_err`, instead of a closure per call site.
pub(super) fn msg(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// The remote copy is gone, or any other failure with its message.
#[derive(Debug)]
pub(super) enum Fail {
    Gone,
    /// GitHub stopped us, the run ends here.
    RateLimited,
    Other(String),
}

impl From<BackendError> for Fail {
    fn from(e: BackendError) -> Self {
        match e {
            BackendError::RateLimited => Self::RateLimited,
            BackendError::NotFound => Self::Gone,
            other => Self::Other(other.to_string()),
        }
    }
}

/// Status text when the gist was deleted on GitHub.
pub(super) const GONE_TEXT: &str = "remote copy gone, `agr sync push --overwrite` uploads it again";

impl From<String> for Fail {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

impl From<&str> for Fail {
    fn from(message: &str) -> Self {
        Self::Other(message.into())
    }
}

impl Fail {
    /// The outcome of this session, or `Err` when the whole run has to stop.
    pub(super) fn outcome(self) -> Result<Outcome, Stop> {
        match self {
            Self::Gone => Ok(Outcome::Skipped(GONE_TEXT.into())),
            Self::RateLimited => Err(Stop),
            Self::Other(message) => Ok(Outcome::Failed(message)),
        }
    }
}

/// Marker, the run ends with `SyncError::RateLimited`.
#[derive(Debug)]
pub(super) struct Stop;
