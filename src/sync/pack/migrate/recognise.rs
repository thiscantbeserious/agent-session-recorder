//! Pure recognition of one gist of the user: ours (v1), an empty stub, or something to leave alone.

use crate::sync::crypto::{Key, Meta};

/// One gist as the old listing shows it, files as (name, size in bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyGist {
    pub id: String,
    pub description: String,
    pub files: Vec<(String, u64)>,
    pub created_at: String,
}

/// What migrate does with a gist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// An `agr:v1` session of our key, moved and then deleted.
    Ours(Meta),
    /// An empty `agr:pending` stub older than an hour, deleted.
    Stub,
    /// Looks like agr but is not ours to touch, listed and kept.
    Kept,
    /// Not an agr gist at all, not even counted.
    Ignored,
}

pub fn recognise(g: &LegacyGist, _key: &Key, _now: i64) -> Kind {
    if g.description == "agr:pending" {
        return Kind::Stub;
    }
    Kind::Kept
}
