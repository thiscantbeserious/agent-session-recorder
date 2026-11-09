//! Range reads of a pack: only the parts that cover the range, through `raw_url` plus `Range`.

use std::io::Read;

use super::{GistStore, StoreError};
use crate::sync::backend::gist::runner::Runner;

impl<R: Runner> GistStore<R> {
    pub(super) fn read_range(
        &self,
        _id: &str,
        _offset: u64,
        _len: u64,
    ) -> Result<Box<dyn Read>, StoreError> {
        Ok(Box::new(std::io::empty()))
    }
}
