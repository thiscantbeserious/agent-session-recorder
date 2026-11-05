//! Plain `.cast` source for the clipboard, decompressing `.cast.gz` to a temp file.

use super::error::ClipboardError;
use crate::files::codec;
use std::path::{Path, PathBuf};

const TEMP_DIR_PREFIX: &str = "agr-clipboard";

/// A plain `.cast` file ready to copy, with its plain size in bytes.
pub struct PlainSource {
    pub path: PathBuf,
    pub size: u64,
    temp_dir: Option<PathBuf>,
}

impl PlainSource {
    /// Remove the decompressed temp copy, if any.
    pub fn discard(&self) {
        if let Some(dir) = &self.temp_dir {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// Plain `.cast` is used as is. A `.cast.gz` is decompressed to
/// `<base>/agr-clipboard-<uid>/<stem>.cast` after clearing that folder, so at most one leftover
/// exists (paste after exit still works). The size limit is checked by the text fallback only.
pub fn prepare(path: &Path, base: &Path) -> Result<PlainSource, ClipboardError> {
    if !codec::is_gzip_path(path) {
        let size = std::fs::metadata(path)?.len();
        return Ok(PlainSource {
            path: path.to_path_buf(),
            size,
            temp_dir: None,
        });
    }
    let dir = fresh_temp_dir(base)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let dest = dir.join(format!(
        "{}{}",
        codec::session_stem(name),
        codec::SESSION_SUFFIX
    ));
    let source = PlainSource {
        path: dest.clone(),
        size: 0,
        temp_dir: Some(dir),
    };
    let decoded = codec::transcode(path, &dest).and_then(|()| Ok(std::fs::metadata(&dest)?.len()));
    match decoded {
        Ok(size) => Ok(PlainSource { size, ..source }),
        Err(e) => {
            source.discard();
            Err(other_error(e))
        }
    }
}

fn other_error(e: anyhow::Error) -> ClipboardError {
    std::io::Error::new(std::io::ErrorKind::Other, format!("{e:#}")).into()
}

/// Ownership decision for the shared temp dir: only our own uid may use it.
pub fn is_owned_by(dir_uid: u32, my_uid: u32) -> bool {
    dir_uid == my_uid
}

/// Empty, private (0700) `<base>/agr-clipboard-<uid>`, refused when it is a symlink or not ours.
#[cfg(unix)]
fn fresh_temp_dir(base: &Path) -> Result<PathBuf, ClipboardError> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    // SAFETY: getuid has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    let dir = base.join(format!("{TEMP_DIR_PREFIX}-{uid}"));
    if let Ok(meta) = std::fs::symlink_metadata(&dir) {
        if meta.file_type().is_symlink() || !is_owned_by(meta.uid(), uid) {
            return Err(other_error(anyhow::anyhow!(
                "Refusing clipboard temp dir {} (symlink or owned by another user)",
                dir.display()
            )));
        }
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    Ok(dir)
}

#[cfg(not(unix))]
fn fresh_temp_dir(base: &Path) -> Result<PathBuf, ClipboardError> {
    let dir = base.join(TEMP_DIR_PREFIX);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
