//! Plan a compress run: which plain sessions, which are live, projected size.

use crate::files::{codec, lock};
use crate::storage::StorageManager;
use anyhow::Result;
use flate2::{Compression, GzBuilder};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Bytes of a file head compressed to project the whole file.
const SAMPLE_BYTES: u64 = 64 * 1024;

/// A plain session eligible for compression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    pub path: PathBuf,
    pub agent: String,
    pub size: u64,
    /// Estimated `.cast.gz` size, see `estimate`.
    pub projected: u64,
}

/// Plain sessions to compress and live-locked ones to skip.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CompressPlan {
    pub files: Vec<PlannedFile>,
    pub live: Vec<PathBuf>,
}

impl CompressPlan {
    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    pub fn total_projected(&self) -> u64 {
        self.files.iter().map(|f| f.projected).sum()
    }
}

/// Collect plain sessions (also leftovers next to a `.cast.gz`), optionally for one agent.
/// `check_not_locked` cleans stale locks, a live lock moves the file to `live`.
pub fn plan(storage: &StorageManager, agent: Option<&str>) -> Result<CompressPlan> {
    let mut plan = CompressPlan::default();
    for (agent, path) in plain_sessions(&storage.storage_dir(), agent)? {
        if lock::check_not_locked(&path).is_err() {
            plan.live.push(path);
            continue;
        }
        let size = fs::metadata(&path)?.len();
        let projected = estimate(&path)?;
        plan.files.push(PlannedFile {
            path,
            agent,
            size,
            projected,
        });
    }
    Ok(plan)
}

fn plain_sessions(storage_dir: &Path, agent: Option<&str>) -> Result<Vec<(String, PathBuf)>> {
    let mut found = Vec::new();
    if !storage_dir.exists() {
        return Ok(found);
    }
    for dir in fs::read_dir(storage_dir)?.filter_map(Result::ok) {
        let name = dir.file_name().to_string_lossy().into_owned();
        if dir.path().is_dir() && (agent.is_none() || agent == Some(name.as_str())) {
            found.extend(plain_in(&dir.path(), &name)?);
        }
    }
    found.sort();
    Ok(found)
}

fn plain_in(dir: &Path, agent: &str) -> Result<Vec<(String, PathBuf)>> {
    Ok(fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| codec::is_session_path(p) && !codec::is_gzip_path(p))
        .map(|p| (agent.to_string(), p))
        .collect())
}

/// Projected gzip size: compress the first 64KiB in memory and scale by the file size.
// ponytail: sample-based, a session whose head compresses very differently from its tail
// gets a rough number. Upgrade path is a larger sample.
pub fn estimate(path: &Path) -> Result<u64> {
    let size = fs::metadata(path)?.len();
    let mut sample = Vec::new();
    File::open(path)?
        .take(SAMPLE_BYTES)
        .read_to_end(&mut sample)?;
    if sample.is_empty() {
        return Ok(0);
    }
    let mut enc = GzBuilder::new().write(Vec::new(), Compression::default());
    enc.write_all(&sample)?;
    let compressed = enc.finish()?.len() as u64;
    Ok(compressed * size / sample.len() as u64)
}
