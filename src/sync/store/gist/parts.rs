//! Chains the part files of an old per-session gist into one stream.

use std::any::Any;
use std::fs::File;
use std::io::{self, ErrorKind, Read};
use std::path::PathBuf;

/// Reads `files` one after the other as one stream, holds `guard` (the scratch clone) until
/// it is dropped, the file handle closes before the guard.
pub(crate) struct PartReader {
    files: Vec<PathBuf>,
    next: usize,
    current: Option<File>,
    _guard: Box<dyn Any>,
}

impl PartReader {
    pub(crate) fn new(files: Vec<PathBuf>, guard: Box<dyn Any>) -> Self {
        Self {
            files,
            next: 0,
            current: None,
            _guard: guard,
        }
    }
}

impl Read for PartReader {
    /// `Ok(0)` only once every part is consumed, an exhausted part moves on to the next,
    /// `Interrupted` is retried.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            if self.current.is_none() {
                let Some(path) = self.files.get(self.next) else {
                    return Ok(0);
                };
                self.current = Some(File::open(path)?);
                self.next += 1;
            }
            let file = self.current.as_mut().expect("opened above");
            match file.read(buf) {
                Ok(0) => self.current = None,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                other => return other,
            }
        }
    }
}
