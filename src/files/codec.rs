//! Transparent plain (`.cast`) and gzip (`.cast.gz`) session I/O.

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const SESSION_SUFFIX: &str = ".cast";
pub const GZIP_SUFFIX: &str = ".cast.gz";

/// Wrap-plausibility ratio for the 32-bit gzip ISIZE field.
const MAX_RATIO: u64 = 64;
const ISIZE_MODULUS: u64 = 1 << 32;

/// Uncompressed size of a session, `Over4GiB` when the gzip ISIZE may have wrapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UncompressedSize {
    Exact(u64),
    Over4GiB,
}

fn file_name(path: &Path) -> &str {
    path.file_name().and_then(OsStr::to_str).unwrap_or("")
}

/// True when the path ends in `.cast.gz`.
pub fn is_gzip_path(path: &Path) -> bool {
    file_name(path).ends_with(GZIP_SUFFIX)
}

/// True for visible `.cast` or `.cast.gz` files (hidden dot files are excluded).
pub fn is_session_path(path: &Path) -> bool {
    let name = file_name(path);
    !name.starts_with('.') && (name.ends_with(GZIP_SUFFIX) || name.ends_with(SESSION_SUFFIX))
}

/// The session suffix of a path, `.cast.gz` or `.cast` (`.cast` when neither matches).
pub fn session_suffix(path: &Path) -> &'static str {
    if is_gzip_path(path) {
        GZIP_SUFFIX
    } else {
        SESSION_SUFFIX
    }
}

/// Strip `.cast.gz` or `.cast` from a file name.
pub fn session_stem(name: &str) -> &str {
    name.strip_suffix(GZIP_SUFFIX)
        .or_else(|| name.strip_suffix(SESSION_SUFFIX))
        .unwrap_or(name)
}

/// Hidden temp path next to the target, `dir/.x.cast.gz.tmp`.
pub fn temp_path_for(path: &Path) -> PathBuf {
    super::hidden_auxiliary_path(path, ".tmp")
}

/// Reader that adds the file path to every I/O error.
struct PathReader<R> {
    inner: R,
    path: PathBuf,
}

impl<R: Read> Read for PathReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner
            .read(buf)
            .map_err(|e| io::Error::new(e.kind(), format!("{}: {}", self.path.display(), e)))
    }
}

/// Open a session for reading, gunzipping when the suffix is `.cast.gz`.
pub fn open_reader(path: &Path) -> Result<Box<dyn BufRead>> {
    let file = File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
    if !is_gzip_path(path) {
        return Ok(Box::new(BufReader::new(file)));
    }
    let inner = GzDecoder::new(BufReader::new(file));
    Ok(Box::new(BufReader::new(PathReader {
        inner,
        path: path.to_path_buf(),
    })))
}

type WriteFn<'a> = &'a mut dyn FnMut(&mut dyn Write) -> Result<()>;

fn write_target(path: &Path, tmp: &Path, write_fn: WriteFn) -> Result<()> {
    let file = File::create(tmp).with_context(|| format!("Failed to create {}", tmp.display()))?;
    let mut out = BufWriter::new(file);
    if is_gzip_path(path) {
        // Default GzEncoder header: mtime 0, no FNAME, so output is deterministic.
        let mut enc = GzEncoder::new(&mut out, Compression::default());
        write_fn(&mut enc)?;
        enc.finish()?;
    } else {
        write_fn(&mut out)?;
    }
    let file = out.into_inner().map_err(io::IntoInnerError::into_error)?;
    file.sync_all()?;
    std::fs::rename(tmp, path).with_context(|| format!("Failed to write {}", path.display()))
}

/// Write via a hidden temp file and rename, gzip-encoding when the target is `.cast.gz`.
pub fn atomic_write(
    path: &Path,
    write_fn: impl FnOnce(&mut dyn Write) -> Result<()>,
) -> Result<()> {
    let tmp = temp_path_for(path);
    let mut once = Some(write_fn);
    let result = write_target(path, &tmp, &mut |w| once.take().map_or(Ok(()), |f| f(w)));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Copy `src` to `dst`, converting format by suffix (compress, decompress, import).
pub fn transcode(src: &Path, dst: &Path) -> Result<()> {
    let mut reader = open_reader(src)?;
    atomic_write(dst, |w| {
        io::copy(&mut reader, w)?;
        Ok(())
    })
}

/// Compare decoded byte streams of two sessions.
pub fn same_content(a: &Path, b: &Path) -> Result<bool> {
    let (mut ra, mut rb) = (open_reader(a)?, open_reader(b)?);
    loop {
        let (ba, bb) = (ra.fill_buf()?, rb.fill_buf()?);
        let n = ba.len().min(bb.len());
        if n == 0 {
            return Ok(ba.is_empty() && bb.is_empty());
        }
        if ba[..n] != bb[..n] {
            return Ok(false);
        }
        ra.consume(n);
        rb.consume(n);
    }
}

/// Decide whether the 32-bit ISIZE may have wrapped given the on-disk size.
// ponytail: sessions over 4GiB compressing better than 64x show a wrong exact number,
// upgrade path is storing the 64-bit size in the gzip FEXTRA field.
pub fn size_from_trailer(isize: u64, on_disk: u64) -> UncompressedSize {
    if isize + ISIZE_MODULUS <= on_disk.saturating_mul(MAX_RATIO) {
        UncompressedSize::Over4GiB
    } else {
        UncompressedSize::Exact(isize)
    }
}

/// Uncompressed size from the gzip trailer (ISIZE) or the plain file length.
pub fn uncompressed_size(path: &Path) -> Result<UncompressedSize> {
    let mut file =
        File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
    let len = file.metadata()?.len();
    if !is_gzip_path(path) {
        return Ok(UncompressedSize::Exact(len));
    }
    let mut buf = [0u8; 4];
    file.seek(SeekFrom::End(-4))
        .and_then(|_| file.read_exact(&mut buf))
        .with_context(|| format!("Failed to read gzip trailer of {}", path.display()))?;
    Ok(size_from_trailer(u32::from_le_bytes(buf) as u64, len))
}
