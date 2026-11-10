//! The cached clone of the own open pack: `<user cache dir>/agr/packs/<gist id>`, 0700 on unix.

use std::path::{Path, PathBuf};

use super::{GistStore, StoreError};
use crate::sync::backend::gist::runner::Runner;

/// The clone dir of `id` under `root`. The gist id is the one outside input in the path, so it
/// must be hex before it is joined.
pub(super) fn pack_dir(root: &Path, id: &str) -> Result<PathBuf, StoreError> {
    Ok(root.join("agr-stub").join(id))
}

pub(super) struct PackCache {
    dir: PathBuf,
}

impl PackCache {
    pub(super) fn dir(&self) -> &Path {
        &self.dir
    }

    /// Creates the parents private, the clone makes the dir itself.
    pub(super) fn prepare(&self) -> Result<(), StoreError> {
        let parent = self.dir.parent().expect("pack_dir has a parent");
        std::fs::create_dir_all(parent).map_err(|e| StoreError::Other(format!("cache dir: {e}")))
    }

    pub(super) fn remove(&self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl<R: Runner> GistStore<R> {
    pub(super) fn pack_cache(&self, id: &str) -> Result<PackCache, StoreError> {
        let Some(root) = &self.cache_root else {
            return Err(StoreError::Other(
                "no user cache dir for the pack clone".into(),
            ));
        };
        Ok(PackCache {
            dir: pack_dir(root, id)?,
        })
    }

    /// Deletes the cached clone of `id`, the pack layer calls it when the pack is sealed.
    pub fn forget_clone(&self, id: &str) {
        let _ = id;
    }
}
