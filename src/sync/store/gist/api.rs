//! `gh api` side of the gist store: listing, index and pack creation.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::PoisonError;

use serde::Deserialize;

use super::index_cache::{Entry, IndexCache};
use super::{GistStore, Listing, PackInfo, StoreError};
use crate::sync::backend::gist::parse::{check_auth, id_from_create_output, map_failure};
use crate::sync::backend::gist::parse::{DESC_PREFIX, LIST_LIMIT};
use crate::sync::backend::gist::runner::{Output, Runner};
use crate::sync::backend::gist::scratch::Scratch;
use crate::sync::backend::gist::{io_result, strings};
use crate::sync::backend::BackendError;

/// Description prefix of a pack gist, the tag follows.
pub(super) const PACK_PREFIX: &str = "agr:v2 ";
const LIST_ENDPOINT: &str = "gists?per_page=100";
pub(super) const INDEX_FILE: &str = "index";

#[derive(Deserialize)]
struct GFile {
    size: u64,
    #[serde(default)]
    raw_url: String,
}

#[derive(Deserialize)]
struct Gist {
    id: String,
    description: Option<String>,
    #[serde(default)]
    updated_at: String,
    #[serde(default)]
    files: BTreeMap<String, GFile>,
}

/// One file of a pack gist: name, size and where `raw_url` serves it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, Deserialize)]
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

pub(super) fn be(e: BackendError) -> StoreError {
    StoreError::from(e)
}

fn seen_of(gist: &Gist) -> Seen {
    let files = gist.files.iter().map(|(name, f)| FileRef {
        name: name.clone(),
        size: f.size,
        raw_url: f.raw_url.clone(),
    });
    Seen {
        updated_at: gist.updated_at.clone(),
        files: files.collect(),
    }
}

fn pack_len(seen: &Seen) -> u64 {
    let data = seen.files.iter().filter(|f| f.name.starts_with("data."));
    data.map(|f| f.size).sum()
}

fn data_files(seen: &Seen) -> usize {
    seen.files
        .iter()
        .filter(|f| f.name.starts_with("data."))
        .count()
}

fn parse_pages(text: &str) -> Result<Vec<Gist>, StoreError> {
    let pages = serde_json::Deserializer::from_str(text).into_iter::<Vec<Gist>>();
    let mut all = Vec::new();
    for page in pages {
        all.extend(page.map_err(|_| StoreError::Other("unexpected gist list output".into()))?);
    }
    if all.len() >= LIST_LIMIT {
        let msg = format!("GitHub lists at most {LIST_LIMIT} gists, delete old gists and retry");
        return Err(StoreError::Other(msg));
    }
    Ok(all)
}

impl<R: Runner> GistStore<R> {
    pub(super) fn run_ok(&self, tool: &'static str, args: &[&str]) -> Result<Output, StoreError> {
        let out = io_result(tool, self.runner.run(tool, &strings(args), None, &[])).map_err(be)?;
        if out.success() {
            return Ok(out);
        }
        Err(be(map_failure(tool, &out.stderr, out.code)))
    }

    pub(super) fn check_tools(&self) -> Result<(), StoreError> {
        for tool in ["git", "gh"] {
            self.run_ok(tool, &["--version"])?;
        }
        let out = io_result(
            "gh",
            self.runner
                .run("gh", &strings(&["auth", "status"]), None, &[]),
        );
        let out = out.map_err(be)?;
        if !out.success() {
            return Err(StoreError::NotLoggedIn);
        }
        check_auth(&format!("{}\n{}", out.stdout, out.stderr)).map_err(be)
    }

    pub(super) fn list_packs(&self) -> Result<Listing, StoreError> {
        let out = self.run_ok("gh", &["api", LIST_ENDPOINT, "--paginate"])?;
        let mut listing = Listing::default();
        let mut seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        for gist in parse_pages(&out.stdout)? {
            let desc = gist.description.clone().unwrap_or_default();
            // An empty tag is no pack we made, it is ignored like any foreign gist.
            let tag = desc
                .strip_prefix(PACK_PREFIX)
                .filter(|t| !t.trim().is_empty());
            if let Some(tag) = tag {
                let info = seen_of(&gist);
                listing.packs.push(PackInfo {
                    id: gist.id.clone(),
                    tag: tag.trim().to_string(),
                    len: pack_len(&info),
                    updated_at: gist.updated_at.clone(),
                    files: data_files(&info),
                });
                seen.insert(gist.id, info);
            } else if desc.starts_with(DESC_PREFIX) && !desc.starts_with(PACK_PREFIX) {
                listing.legacy.push(desc);
            }
        }
        Ok(listing)
    }

    /// `gh api gists/<id>`, remembers the files, returns the gist.
    fn fetch_gist(&self, id: &str) -> Result<Gist, StoreError> {
        let out = self.run_ok("gh", &["api", &format!("gists/{id}")])?;
        let gist: Gist = serde_json::from_str(&out.stdout)
            .map_err(|_| StoreError::Other("unexpected gist output".into()))?;
        let mut seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        seen.insert(id.to_string(), seen_of(&gist));
        Ok(gist)
    }

    /// The files of a pack, from the listing or one `gh api` call.
    pub(super) fn files_of(&self, id: &str) -> Result<Vec<FileRef>, StoreError> {
        let known = self
            .seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned();
        match known {
            Some(seen) => Ok(seen.files),
            None => self.fetch_gist(id).map(|g| seen_of(&g).files),
        }
    }

    /// The `updated_at` of the last listing or fetch of `id`.
    fn known_update(&self, id: &str) -> Option<String> {
        let seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        seen.get(id).map(|s| s.updated_at.clone())
    }

    /// Downloads the whole file behind `url` into `file` with `gh api`, no `Range`.
    fn fetch_whole(&self, url: &str, size: u64, file: &Path) -> Result<(), StoreError> {
        let args = strings(&["api", url]);
        let out = io_result("gh", self.runner.run_to_file("gh", &args, &[], file)).map_err(be)?;
        if !out.success() {
            return Err(be(map_failure("gh", &out.stderr, out.code)));
        }
        let got = std::fs::metadata(file).map_err(|e| be(BackendError::Other(e.to_string())))?;
        match got.len() == size {
            true => Ok(()),
            false => Err(StoreError::Other(format!(
                "the index came back with {} bytes, the listing says {size}",
                got.len()
            ))),
        }
    }

    /// The index over its `raw_url`, GitHub flags small files `truncated` beside big parts.
    pub(super) fn index_of(&self, id: &str) -> Result<String, StoreError> {
        let files = self.files_of(id)?;
        let Some(file) = files.iter().find(|f| f.name == INDEX_FILE) else {
            return Err(StoreError::Other("the pack has no index".into()));
        };
        let cache = self.cache_file.as_deref().map(IndexCache::new);
        let at = self.known_update(id).unwrap_or_default();
        if let Some(hit) = cache.as_ref().and_then(|c| c.get(id, &at)) {
            return Ok(hit.index);
        }
        let scratch = Scratch::create(&self.temp_base).map_err(be)?;
        let path = scratch.join(INDEX_FILE);
        self.fetch_whole(&file.raw_url, file.size, &path)?;
        let bytes = std::fs::read(&path).map_err(|e| StoreError::Other(e.to_string()))?;
        let index = String::from_utf8(bytes)
            .map_err(|_| StoreError::Other("the index is not text".into()))?;
        if let Some(cache) = &cache {
            let entry = Entry {
                updated_at: at,
                index: index.clone(),
            };
            cache.put(id, entry);
        }
        Ok(index)
    }

    pub(super) fn create_pack(&self, tag: &str, index: &str) -> Result<String, StoreError> {
        let scratch = Scratch::create(&self.temp_base).map_err(be)?;
        let file = scratch.join(INDEX_FILE);
        std::fs::write(&file, index).map_err(|e| StoreError::Other(format!("write index: {e}")))?;
        let (path, desc) = (file.to_string_lossy(), format!("{PACK_PREFIX}{tag}"));
        self.pacer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .wait();
        let out = self.run_ok("gh", &["gist", "create", &path, "--desc", &desc])?;
        id_from_create_output(&out.stdout)
            .ok_or_else(|| StoreError::Other("gh did not print a gist url".into()))
    }

    /// The index file counts on top of the data files of `info`.
    pub(super) fn fits_files(&self, info: &PackInfo, lens: &[u64]) -> bool {
        super::layout::fits(info.files + 1, lens)
    }
}
