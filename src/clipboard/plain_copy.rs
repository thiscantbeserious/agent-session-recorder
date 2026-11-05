//! Plain `.cast` source for the clipboard, decompressing `.cast.gz` to a temp file.

use super::error::{ClipboardError, MAX_CONTENT_SIZE};
use crate::files::codec::{self, UncompressedSize};
use std::path::{Path, PathBuf};

const TEMP_DIR_NAME: &str = "agr-clipboard";

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

/// Plain `.cast` is used as is. A `.cast.gz` is decompressed to `<base>/agr-clipboard/<stem>.cast`
/// after clearing that folder, so at most one leftover exists (paste after exit still works).
pub fn prepare(path: &Path, base: &Path) -> Result<PlainSource, ClipboardError> {
    if !codec::is_gzip_path(path) {
        let size = std::fs::metadata(path)?.len();
        return Ok(PlainSource {
            path: path.to_path_buf(),
            size,
            temp_dir: None,
        });
    }
    reject_if_too_large(path)?;
    let dir = base.join(TEMP_DIR_NAME);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
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
            Err(std::io::Error::new(std::io::ErrorKind::Other, format!("{e:#}")).into())
        }
    }
}

/// Size gate from the gzip header, before anything is written to the temp dir.
fn reject_if_too_large(path: &Path) -> Result<(), ClipboardError> {
    let size = match codec::uncompressed_size(path) {
        Ok(UncompressedSize::Exact(n)) if n <= MAX_CONTENT_SIZE => return Ok(()),
        Ok(UncompressedSize::Exact(n)) => n,
        Ok(UncompressedSize::Over4GiB) => 4 << 30,
        Err(e) => {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("{e:#}")).into())
        }
    };
    Err(ClipboardError::FileTooLarge {
        size_mb: size as f64 / (1024.0 * 1024.0),
        max_mb: MAX_CONTENT_SIZE / (1024 * 1024),
    })
}
