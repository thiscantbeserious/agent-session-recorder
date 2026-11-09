//! Range reads of a pack: only the parts that cover the range, through `gh api` on `raw_url` plus `Range`.
//! The one read path, there is no clone fallback.

use std::io::Read;
use std::path::Path;

use super::api::{be, FileRef};
use super::layout::check_parts;
use super::{GistStore, StoreError};
use crate::sync::backend::gist::parse::map_failure;
use crate::sync::backend::gist::parts::PartReader;
use crate::sync::backend::gist::runner::Runner;
use crate::sync::backend::gist::scratch::Scratch;
use crate::sync::backend::gist::{io_result, strings};

/// `(part, first byte, last byte)` of every part the range `offset..offset + len` touches.
fn covering(parts: &[FileRef], offset: u64, len: u64) -> Result<Vec<(&FileRef, u64, u64)>, String> {
    let (end, mut start, mut out) = (offset + len, 0, Vec::new());
    for part in parts {
        let stop = start + part.size;
        if start < end && stop > offset {
            out.push((part, offset.max(start) - start, end.min(stop) - 1 - start));
        }
        start = stop;
    }
    if start < end {
        return Err("the range is past the end of the pack".into());
    }
    Ok(out)
}

impl<R: Runner> GistStore<R> {
    /// Downloads one range of one part into `file`, `gh api` streams the raw bytes there.
    fn fetch_range(
        &self,
        url: &str,
        (from, to): (u64, u64),
        file: &Path,
    ) -> Result<(), StoreError> {
        let range = format!("Range: bytes={from}-{to}");
        let args = strings(&["api", "-H", &range, url]);
        let out = io_result("gh", self.runner.run_to_file("gh", &args, &[], file)).map_err(be)?;
        match out.success() {
            true => Ok(()),
            false => Err(be(map_failure("gh", &out.stderr, out.code))),
        }
    }

    pub(super) fn read_range(
        &self,
        id: &str,
        offset: u64,
        len: u64,
    ) -> Result<Box<dyn Read>, StoreError> {
        let files = self.files_of(id)?;
        let parts: Vec<FileRef> = files
            .into_iter()
            .filter(|f| f.name.starts_with("data."))
            .collect();
        let names: Vec<&str> = parts.iter().map(|p| p.name.as_str()).collect();
        check_parts(&names).map_err(|e| StoreError::Other(format!("pack {id} {e}")))?;
        let wanted = covering(&parts, offset, len).map_err(StoreError::Other)?;
        let scratch = Scratch::create(&self.temp_base).map_err(be)?;
        let mut paths = Vec::new();
        for (n, (part, from, to)) in wanted.into_iter().enumerate() {
            let path = scratch.join(&format!("range.{n}"));
            self.fetch_range(&part.raw_url, (from, to), &path)?;
            paths.push(path);
        }
        Ok(Box::new(PartReader::new(paths, Box::new(scratch))))
    }
}
