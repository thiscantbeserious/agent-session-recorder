//! Scratch clone directory of the gist store, private and removed on drop.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::sync::store::StoreError;

static CLONE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Scratch directory under the temp dir, removed on drop (also on error paths).
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn create(base: &Path) -> Result<Self, StoreError> {
        let n = CLONE_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self::at(&base.join(format!("agr-sync-{}-{n}", std::process::id())))
    }

    /// A private directory at exactly `dir`, an existing one is refused.
    pub fn at(dir: &Path) -> Result<Self, StoreError> {
        private_dir_builder()
            .create(dir)
            .map_err(|e| StoreError::Other(format!("temp dir: {e}")))?;
        Ok(Self(dir.to_path_buf()))
    }

    pub(crate) fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn private_dir_builder() -> std::fs::DirBuilder {
    #[allow(unused_mut)]
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder
}
