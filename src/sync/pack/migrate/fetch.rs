//! Reads one old gist into a scratch file and checks it against its manifest.

use super::{Env, LegacyGist};
use crate::sync::crypto::blob::open_stream;
use crate::sync::crypto::{open_meta, Meta};
use crate::sync::store::StoreError;
use std::path::PathBuf;

pub(super) enum Fetched {
    /// The plaintext session sits in the file, hash and size match the manifest.
    Ready(PathBuf),
    /// Not moved and not deleted, the text says why.
    Kept(String),
    /// GitHub asked to wait or the read failed in a way a later run may get past, the gist
    /// stays untouched and counts as left.
    Limit,
}

fn unreadable(why: impl std::fmt::Display) -> Fetched {
    Fetched::Kept(format!("cannot be read ({why})"))
}

pub(super) fn fetch(env: &Env, g: &LegacyGist, meta: &Meta) -> Fetched {
    let (manifest, reader) = match env.legacy.pull(&g.id) {
        Ok(got) => got,
        Err(StoreError::NotFound) => return unreadable(StoreError::NotFound),
        Err(_) => return Fetched::Limit,
    };
    match manifest.map(|t| open_meta(env.key, &t)) {
        Some(Ok(m)) if &m == meta => {}
        _ => return unreadable("manifest missing or not the description"),
    }
    let file = env.scratch.join(&g.id);
    let result = std::fs::File::create(&file)
        .map_err(|e| e.to_string())
        .and_then(|out| {
            open_stream(env.key, &meta.path, meta.gen, reader, out).map_err(|e| e.to_string())
        });
    let fetched = match result {
        Ok((hash, size)) if hash == meta.sha256 && size == meta.size => {
            return Fetched::Ready(file)
        }
        Ok(_) => Fetched::Kept("its hash does not match its manifest".into()),
        Err(e) => unreadable(e),
    };
    let _ = std::fs::remove_file(&file);
    fetched
}
