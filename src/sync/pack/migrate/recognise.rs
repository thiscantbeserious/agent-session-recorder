//! Pure recognition of one gist of the user: ours (v1), an empty stub, or something to leave alone.

use crate::sync::crypto::{open_meta, Key, Meta};

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

const STUB_AFTER: i64 = 3600;
/// The stub body is `agr\n`.
const STUB_BODY_LEN: u64 = 4;

/// Seconds since `created_at`, an unreadable time counts as brand new.
fn age(g: &LegacyGist, now: i64) -> i64 {
    chrono::DateTime::parse_from_rfc3339(&g.created_at).map_or(0, |t| now - t.timestamp())
}

const STUB_FILE: &str = ".keep";
const V1_FILES: [&str; 3] = [".keep", "manifest", "data."];

fn is_stub(g: &LegacyGist) -> bool {
    g.files.len() == 1 && g.files[0] == (STUB_FILE.to_string(), STUB_BODY_LEN)
}

/// `.keep`, `manifest` and parts named `data.NNN`, nothing else.
fn only_v1_files(g: &LegacyGist) -> bool {
    let known = |n: &str| {
        V1_FILES
            .iter()
            .any(|k| n == *k || (k.ends_with('.') && n.starts_with(k)))
    };
    g.files.iter().all(|(n, _)| known(n))
}

/// Decides per gist, `now` is unix seconds from an injected clock.
pub fn recognise(g: &LegacyGist, key: &Key, now: i64) -> Kind {
    if g.description == "agr:pending" {
        let old = age(g, now) > STUB_AFTER;
        return if old && is_stub(g) {
            Kind::Stub
        } else {
            Kind::Kept
        };
    }
    let Some(rest) = g.description.strip_prefix("agr:") else {
        return Kind::Ignored;
    };
    let token = rest.strip_prefix("v1 ").map(str::trim);
    match token.map(|t| open_meta(key, t)) {
        Some(Ok(meta)) if only_v1_files(g) => Kind::Ours(meta),
        _ => Kind::Kept,
    }
}

/// An `agr:v1` description our key opens, the gists `Repo::open` refuses to run beside.
pub fn is_ours_v1(g: &LegacyGist, key: &Key) -> bool {
    let token = g.description.strip_prefix("agr:v1 ").map(str::trim);
    token.is_some_and(|t| open_meta(key, t).is_ok())
}
