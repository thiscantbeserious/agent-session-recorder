//! `gh api` side of the gist store: listing, index and pack creation.

use super::{GistStore, Listing, PackInfo, StoreError};
use crate::sync::backend::gist::runner::Runner;

/// One file of a gist as the API shows it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct FileRef {
    pub name: String,
    pub size: u64,
    pub raw_url: String,
}

/// What the store knows about one pack gist.
#[derive(Clone)]
pub(super) struct Seen {
    pub updated_at: String,
    pub files: Vec<FileRef>,
}

impl<R: Runner> GistStore<R> {
    pub(super) fn check_tools(&self) -> Result<(), StoreError> {
        Ok(())
    }

    pub(super) fn list_packs(&self) -> Result<Listing, StoreError> {
        Ok(Listing::default())
    }

    pub(super) fn index_of(&self, _id: &str) -> Result<String, StoreError> {
        Ok(String::new())
    }

    pub(super) fn create_pack(&self, _tag: &str, _index: &str) -> Result<String, StoreError> {
        let _ = (&self.runner, &self.temp_base, &self.cache_file, &self.pacer);
        Ok(String::new())
    }

    pub(super) fn fits_files(&self, _info: &PackInfo, lens: &[u64]) -> bool {
        super::layout::fits(0, lens)
    }
}
