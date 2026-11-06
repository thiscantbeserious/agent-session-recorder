//! Why one push or pull did not finish.

use super::run::Outcome;

/// The remote copy is gone, or any other failure with its message.
#[derive(Debug)]
pub(super) enum Fail {
    Gone,
    Other(String),
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
    pub(super) fn outcome(self) -> Outcome {
        match self {
            Self::Gone => Outcome::Skipped(GONE_TEXT.into()),
            Self::Other(message) => Outcome::Failed(message),
        }
    }
}
