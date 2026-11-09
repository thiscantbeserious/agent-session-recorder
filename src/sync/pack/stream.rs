//! Pack stream: session files sealed as blob v2 segments, and a range read back out of a pack.

#![allow(dead_code)] // Repo of Stage 51 uses it

use crate::sync::crypto::blob::seal_stream;
use crate::sync::crypto::Key;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

/// One session file as a sealed segment, `len` is the exact byte count `write` produces.
pub struct FileSegment<'a> {
    pub len: u64,
    key: &'a Key,
    path: &'a str,
    gen: u64,
    file: &'a Path,
}

/// Seals `file` under `path` and `gen` (both in the AAD) once `write` runs.
pub fn segment<'a>(
    key: &'a Key,
    path: &'a str,
    gen: u64,
    file: &'a Path,
) -> io::Result<FileSegment<'a>> {
    Ok(FileSegment {
        len: 0,
        key,
        path,
        gen,
        file,
    })
}

impl FileSegment<'_> {
    /// Streams the sealed blob into `dst`, one frame in memory at a time.
    pub fn write(&self, dst: &mut dyn Write) -> io::Result<()> {
        let src = File::open(self.file)?;
        seal_stream(self.key, self.path, self.gen, src, dst).map_err(io::Error::other)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::segment;
    use crate::sync::crypto::Key;

    #[test]
    fn segment_len_equals_written_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::generate();
        for size in [0usize, 1, 64 * 1024, 64 * 1024 + 1] {
            let file = dir.path().join("s.cast");
            std::fs::write(&file, vec![7u8; size]).unwrap();
            let seg = segment(&key, "a/s.cast", 1, &file).unwrap();
            let mut out = Vec::new();
            seg.write(&mut out).unwrap();
            assert_eq!(seg.len, out.len() as u64, "plain size {size}");
        }
    }
}
