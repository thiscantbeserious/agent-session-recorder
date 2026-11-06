//! Transparent plain (`.cast`) and gzip (`.cast.gz`) session I/O.

use anyhow::{Context, Result};
use flate2::read::MultiGzDecoder;
use flate2::{Compression, GzBuilder};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const SESSION_SUFFIX: &str = ".cast";
pub const GZIP_SUFFIX: &str = ".cast.gz";

/// Gzip FEXTRA subfield id holding the exact uncompressed size (u64 LE).
const SIZE_SUBFIELD_ID: &[u8; 2] = b"AG";
/// Byte offset of that u64: 10 byte header, 2 byte XLEN, 4 byte subfield header.
const SIZE_OFFSET: u64 = 16;
const FEXTRA: u8 = 0x04;

/// Uncompressed size of a session, `Over4GiB` when only a wrapped gzip ISIZE is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UncompressedSize {
    Exact(u64),
    Over4GiB,
}

static WARNING_SINK: Mutex<Option<fn(&str)>> = Mutex::new(None);

/// Route non-fatal warnings (failed directory fsync) to `sink`, `None` restores stderr.
/// A TUI owning the terminal installs one so a stray `eprintln!` cannot corrupt the screen.
pub fn set_warning_sink(sink: Option<fn(&str)>) {
    *WARNING_SINK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = sink;
}

fn warn(message: &str) {
    let sink = *WARNING_SINK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match sink {
        Some(sink) => sink(message),
        None => eprintln!("Warning: {message}"),
    }
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

/// The `.cast.gz` path of the same session, `x.cast` and `x.cast.gz` both give `x.cast.gz`.
pub fn gzip_sibling(path: &Path) -> PathBuf {
    path.with_file_name(format!("{}{GZIP_SUFFIX}", session_stem(file_name(path))))
}

/// The same session under the other suffix, `x.cast` <-> `x.cast.gz`.
pub fn other_variant(path: &Path) -> PathBuf {
    if is_gzip_path(path) {
        path.with_file_name(format!("{}{SESSION_SUFFIX}", session_stem(file_name(path))))
    } else {
        gzip_sibling(path)
    }
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
    let inner = MultiGzDecoder::new(BufReader::new(file));
    Ok(Box::new(BufReader::new(PathReader {
        inner,
        path: path.to_path_buf(),
    })))
}

/// Counts bytes passed through, so the gzip header can record the uncompressed size.
struct CountingWriter<W> {
    inner: W,
    count: u64,
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.count += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// FEXTRA payload: subfield `AG`, length 8, zero size placeholder.
fn size_placeholder() -> Vec<u8> {
    let mut extra = SIZE_SUBFIELD_ID.to_vec();
    extra.extend_from_slice(&8u16.to_le_bytes());
    extra.extend_from_slice(&0u64.to_le_bytes());
    extra
}

fn write_gzip(file: File, write_fn: impl FnOnce(&mut dyn Write) -> Result<()>) -> Result<File> {
    // Fixed header: mtime 0, no FNAME, so output is deterministic.
    let enc = GzBuilder::new()
        .extra(size_placeholder())
        .write(BufWriter::new(file), Compression::default());
    let mut counter = CountingWriter {
        inner: enc,
        count: 0,
    };
    write_fn(&mut counter)?;
    let out = counter
        .inner
        .finish()
        .context("Failed to finish gzip stream")?;
    let file = out.into_inner().map_err(io::IntoInnerError::into_error)?;
    patch_size(file, counter.count)
}

/// Overwrite the size placeholder in the gzip header with the real uncompressed size.
fn patch_size(mut file: File, size: u64) -> Result<File> {
    file.seek(SeekFrom::Start(SIZE_OFFSET))?;
    file.write_all(&size.to_le_bytes())?;
    Ok(file)
}

fn write_plain(file: File, write_fn: impl FnOnce(&mut dyn Write) -> Result<()>) -> Result<File> {
    let mut out = BufWriter::new(file);
    write_fn(&mut out)?;
    Ok(out.into_inner().map_err(io::IntoInnerError::into_error)?)
}

/// Fill the temp file via `fill`, then fsync, rename over `path` and sync the directory.
fn write_target(path: &Path, tmp: &Path, fill: impl FnOnce(File) -> Result<File>) -> Result<()> {
    let file = File::create(tmp).with_context(|| format!("Failed to create {}", tmp.display()))?;
    let file = fill(file).with_context(|| format!("Failed to write {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("Failed to sync {}", tmp.display()))?;
    std::fs::rename(tmp, path)
        .with_context(|| format!("Failed to rename {} to {}", tmp.display(), path.display()))?;
    // The rename succeeded, so a failed directory fsync only weakens crash safety.
    if let Err(e) = sync_parent(path) {
        warn(&format!("{e:#}"));
    }
    Ok(())
}

/// fsync the directory so the rename survives a crash (unix only).
fn sync_parent(path: &Path) -> Result<()> {
    if !cfg!(unix) {
        return Ok(());
    }
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    File::open(dir)
        .and_then(|d| d.sync_all())
        .with_context(|| format!("Failed to sync directory {}", dir.display()))
}

/// Write via a hidden temp file and rename, gzip-encoding when the target is `.cast.gz`.
// ponytail: fixed temp path, two concurrent writers to the same target clobber each other,
// upgrade path is a unique temp name (pid) if concurrent writes ever happen.
pub fn atomic_write(
    path: &Path,
    write_fn: impl FnOnce(&mut dyn Write) -> Result<()>,
) -> Result<()> {
    commit(path, |file| {
        if is_gzip_path(path) {
            write_gzip(file, write_fn)
        } else {
            write_plain(file, write_fn)
        }
    })
}

/// Write `bytes` verbatim via the same temp and rename path, never gzip-encoding.
pub fn atomic_write_raw(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic_write_raw_with(path, |w| Ok(w.write_all(bytes)?))
}

/// Like `atomic_write_raw`, but `fill` streams into the hidden temp, rename only after `Ok`.
pub fn atomic_write_raw_with(
    path: &Path,
    fill: impl FnOnce(&mut dyn Write) -> Result<()>,
) -> Result<()> {
    commit(path, |file| write_plain(file, fill))
}

/// Run `write_target` on the hidden temp path and remove the temp on failure.
fn commit(path: &Path, fill: impl FnOnce(File) -> Result<File>) -> Result<()> {
    let tmp = temp_path_for(path);
    let result = write_target(path, &tmp, fill);
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

/// Size from our `AG` header subfield, `None` for gzip files written by other tools.
/// Reads only the header bytes, never the deflate body.
fn size_from_header(path: &Path) -> Result<Option<u64>> {
    let mut file =
        File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
    let mut fixed = [0u8; 12];
    let read = file.read_exact(&mut fixed).and_then(|_| {
        let xlen = u16::from_le_bytes([fixed[10], fixed[11]]) as usize;
        let mut extra = vec![0u8; if fixed[3] & FEXTRA == 0 { 0 } else { xlen }];
        file.read_exact(&mut extra).map(|_| extra)
    });
    // A header too short for FEXTRA is not an error, the trailer fallback decides.
    Ok(read.ok().and_then(|extra| find_size_subfield(&extra)))
}

fn find_size_subfield(mut extra: &[u8]) -> Option<u64> {
    while extra.len() >= 4 {
        let len = u16::from_le_bytes([extra[2], extra[3]]) as usize;
        let data = extra.get(4..4 + len)?;
        if extra[..2] == SIZE_SUBFIELD_ID[..] {
            return Some(u64::from_le_bytes(data.try_into().ok()?));
        }
        extra = &extra[4 + len..];
    }
    None
}

/// Trailer ISIZE as `Exact`, `Over4GiB` when it is below the on-disk size (a wrap).
fn size_from_trailer(path: &Path, on_disk: u64) -> Result<UncompressedSize> {
    let mut file =
        File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
    let mut buf = [0u8; 4];
    file.seek(SeekFrom::End(-4))
        .and_then(|_| file.read_exact(&mut buf))
        .with_context(|| format!("Failed to read gzip trailer of {}", path.display()))?;
    let isize = u32::from_le_bytes(buf) as u64;
    Ok(if isize < on_disk {
        UncompressedSize::Over4GiB
    } else {
        UncompressedSize::Exact(isize)
    })
}

/// Uncompressed size: plain file length, gzip `AG` header subfield, else the gzip trailer.
/// Limit: a foreign multi-member gzip reports only the last member's ISIZE (trailer read).
// ponytail: foreign gzip files over 4GiB whose wrapped ISIZE still exceeds the on-disk size
// show a wrong number, rewriting through `atomic_write` fixes them.
pub fn uncompressed_size(path: &Path) -> Result<UncompressedSize> {
    let len = std::fs::metadata(path)
        .with_context(|| format!("Failed to stat {}", path.display()))?
        .len();
    if !is_gzip_path(path) {
        return Ok(UncompressedSize::Exact(len));
    }
    match size_from_header(path)? {
        Some(size) => Ok(UncompressedSize::Exact(size)),
        None => size_from_trailer(path, len),
    }
}
