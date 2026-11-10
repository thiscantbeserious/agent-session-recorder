//! The cached clone of the own open pack: `<user cache dir>/agr/packs/<gist id>`, 0700 on unix.

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::PoisonError;

use super::index_cache::make_private_dir;
use super::{GistStore, StoreError};
use crate::sync::backend::gist::runner::Runner;

/// The clone dir of `id` under `root`. The gist id is the one outside input in the path, so it
/// must be hex before it is joined.
pub(super) fn pack_dir(root: &Path, id: &str) -> Result<PathBuf, StoreError> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(StoreError::Other("the pack id is not a gist id".into()));
    }
    Ok(root.join("agr").join("packs").join(id))
}

/// A hex named dir with a `.git` dir in it, what `git clone` of a pack leaves.
fn is_clone(path: &Path) -> bool {
    let hex = path.file_name().and_then(|n| n.to_str());
    hex.is_some_and(|n| pack_dir(Path::new(""), n).is_ok()) && path.join(".git").is_dir()
}

/// Writes the planned parts one after the other, a segment may span or share parts.
pub(super) struct Cut<'a> {
    pub(super) dir: &'a Path,
    pub(super) todo: std::vec::IntoIter<(String, u64)>,
    pub(super) file: Option<File>,
    pub(super) room: u64,
    pub(super) written: u64,
}

impl Write for Cut<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        while self.room == 0 {
            let (name, size) = self
                .todo
                .next()
                .ok_or_else(|| io::Error::other("wrote past the planned parts"))?;
            self.file = Some(File::create(self.dir.join(name))?);
            self.room = size;
        }
        let take = buf.len().min(self.room as usize);
        let n = self
            .file
            .as_mut()
            .expect("opened above")
            .write(&buf[..take])?;
        (self.room, self.written) = (self.room - n as u64, self.written + n as u64);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.as_mut().map_or(Ok(()), Write::flush)
    }
}

/// `data.*` names and sizes out of `git ls-tree -l`, one `mode blob sha size<TAB>name` line each.
pub(super) fn parse_tree(text: &str) -> Vec<(String, u64)> {
    let rows = text.lines().filter_map(|l| l.split_once('\t'));
    let sized =
        rows.filter_map(|(a, name)| Some((name, a.split_whitespace().nth(3)?.parse().ok()?)));
    let data = sized.filter(|(name, _)| name.starts_with("data."));
    data.map(|(name, size)| (name.to_string(), size)).collect()
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
        make_private_dir(parent).map_err(|e| StoreError::Other(format!("cache dir: {e}")))
    }

    pub(super) fn remove(&self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }

    /// The cache holds the clone of one open pack only, a clone of any other pack is a sealed
    /// or abandoned one and goes. Only a hex named dir with a `.git` in it counts as a clone,
    /// anything else under `packs` is not ours and stays.
    pub(super) fn remove_others(&self) {
        let Some(parent) = self.dir.parent() else {
            return;
        };
        let others = std::fs::read_dir(parent).into_iter().flatten().flatten();
        let clones = others.filter(|e| e.path() != self.dir).map(|e| e.path());
        for path in clones.filter(|p| is_clone(p)) {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

impl<R: Runner> GistStore<R> {
    pub(super) fn pack_cache(&self, id: &str) -> Result<PackCache, StoreError> {
        let Some(root) = &self.cache_root else {
            return Err(StoreError::Other(
                "no user cache dir found, set XDG_CACHE_HOME or HOME (LOCALAPPDATA on Windows)"
                    .into(),
            ));
        };
        Ok(PackCache {
            dir: pack_dir(root, id)?,
        })
    }

    /// Deletes the cached clone of `id`, the pack layer calls it when the pack is sealed.
    pub(super) fn forget_clone(&self, id: &str) {
        self.pack_cache(id).iter().for_each(PackCache::remove);
    }

    pub(super) fn delete_pack(&self, id: &str) -> Result<(), StoreError> {
        self.forget_clone(id);
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(id);
        self.pacer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .wait();
        self.run_ok("gh", &["gist", "delete", id, "--yes"])
            .map(drop)
    }
}
