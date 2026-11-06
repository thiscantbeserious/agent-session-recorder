//! The sealed blob as gist part files `data.NNN`, a rolling writer and a chaining reader.

use std::any::Any;
use std::fs::{self, File};
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

/// Largest part, under git's 50 MiB warning.
pub(super) const PART_SIZE: usize = 45 * 1024 * 1024;
/// Prefix of every part file, the name is `data.NNN`.
pub(super) const PART_PREFIX: &str = "data.";

pub(super) fn part_name(index: usize) -> String {
    format!("{PART_PREFIX}{index:03}")
}

/// `names` must be sorted and be exactly `part_name(0..n)`, anything else is an error.
pub(super) fn check_sequence(names: &[String]) -> Result<(), String> {
    if names.is_empty() {
        return Err("has no data parts".into());
    }
    for (i, name) in names.iter().enumerate() {
        if *name != part_name(i) {
            return Err(format!("expected {} but found {name}", part_name(i)));
        }
    }
    Ok(())
}

/// Names of existing parts that a push of `count` parts leaves behind.
pub(super) fn stale(existing: &[String], count: usize) -> Vec<String> {
    let keep: Vec<String> = (0..count).map(part_name).collect();
    existing
        .iter()
        .filter(|n| n.starts_with(PART_PREFIX) && !keep.contains(n))
        .cloned()
        .collect()
}

/// Sorted names of the `data.*` files in `dir`.
pub(super) fn part_names(dir: &Path) -> io::Result<Vec<String>> {
    let mut names: Vec<String> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with(PART_PREFIX))
        .collect();
    names.sort();
    Ok(names)
}

/// Writes into `data.000`, `data.001`, ... inside `dir`, a part is full at `size` bytes and the
/// next one is only opened by the next byte, so an exact multiple leaves no empty part.
pub(super) struct PartWriter {
    dir: PathBuf,
    size: usize,
    count: usize,
    used: usize,
    current: Option<File>,
}

impl PartWriter {
    pub(super) fn new(dir: &Path, size: usize) -> Self {
        Self {
            dir: dir.to_path_buf(),
            size,
            count: 0,
            used: 0,
            current: None,
        }
    }

    /// The part to write into, opens the next one when the current is full.
    fn part(&mut self) -> io::Result<&mut File> {
        if self.current.is_some() && self.used >= self.size {
            self.current = None;
        }
        if self.current.is_none() {
            self.current = Some(File::create(self.dir.join(part_name(self.count)))?);
            self.count += 1;
            self.used = 0;
        }
        Ok(self.current.as_mut().expect("opened above"))
    }

    /// Closes the last part (an empty blob still gets `data.000`), removes the stale higher
    /// parts and returns every touched name, written ones first.
    pub(super) fn finish(mut self) -> io::Result<Vec<String>> {
        if self.count == 0 {
            self.part()?;
        }
        self.current = None;
        let mut names: Vec<String> = (0..self.count).map(part_name).collect();
        for old in stale(&part_names(&self.dir)?, self.count) {
            fs::remove_file(self.dir.join(&old))?;
            names.push(old);
        }
        Ok(names)
    }
}

impl Write for PartWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let room = self.size - self.used.min(self.size);
        let take = if room == 0 { self.size } else { room }.min(buf.len());
        let n = self.part()?.write(&buf[..take])?;
        self.used += n;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.current.as_mut().map_or(Ok(()), Write::flush)
    }
}

/// Reads `files` one after the other as one stream, holds `guard` (the scratch clone) until
/// it is dropped, the file handle closes before the guard.
pub(super) struct PartReader {
    files: Vec<PathBuf>,
    next: usize,
    current: Option<File>,
    _guard: Box<dyn Any>,
}

impl PartReader {
    pub(super) fn new(files: Vec<PathBuf>, guard: Box<dyn Any>) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn names(n: usize) -> Vec<String> {
        (0..n).map(part_name).collect()
    }

    #[test]
    fn names_are_zero_padded() {
        assert_eq!(part_name(0), "data.000");
        assert_eq!(part_name(12), "data.012");
    }

    #[test]
    fn sequence_accepts_contiguous_from_zero() {
        assert_eq!(check_sequence(&names(1)), Ok(()));
        assert_eq!(check_sequence(&names(5)), Ok(()));
    }

    #[test]
    fn sequence_rejects_empty_gap_extra_and_out_of_order() {
        assert!(check_sequence(&[]).is_err());
        let mut gap = names(3);
        gap.remove(1);
        assert!(check_sequence(&gap).is_err());
        let mut no_start = names(3);
        no_start.remove(0);
        assert!(check_sequence(&no_start).is_err());
        let mut extra = names(2);
        extra.push("data.junk".into());
        assert!(check_sequence(&extra).is_err());
        let mut shuffled = names(3);
        shuffled.swap(0, 2);
        assert!(check_sequence(&shuffled).is_err());
    }

    #[test]
    fn stale_are_the_parts_at_or_above_the_new_count() {
        assert_eq!(stale(&names(4), 2), [part_name(2), part_name(3)]);
        assert!(stale(&names(2), 2).is_empty());
        assert!(stale(&names(2), 5).is_empty());
    }

    struct OneByte<R>(R);

    impl<R: Read> Read for OneByte<R> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = buf.len().min(1);
            self.0.read(&mut buf[..n])
        }
    }

    fn payload(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    fn write_all_one_byte(w: &mut PartWriter, data: &[u8]) {
        for byte in data {
            w.write_all(std::slice::from_ref(byte)).unwrap();
        }
    }

    fn contents(dir: &Path) -> Vec<(String, usize)> {
        part_names(dir)
            .unwrap()
            .into_iter()
            .map(|n| (n.clone(), fs::read(dir.join(&n)).unwrap().len()))
            .collect()
    }

    fn written(dir: &Path, size: usize, data: &[u8]) -> Vec<String> {
        let mut w = PartWriter::new(dir, size);
        w.write_all(data).unwrap();
        w.finish().unwrap()
    }

    #[test]
    fn writer_rolls_over_at_the_part_size() {
        let dir = tempfile::tempdir().unwrap();
        let touched = written(dir.path(), 10, &payload(25));
        assert_eq!(touched, ["data.000", "data.001", "data.002"]);
        let sizes = [("data.000".to_string(), 10), ("data.001".into(), 10)];
        assert_eq!(contents(dir.path())[..2], sizes);
        assert_eq!(contents(dir.path())[2], ("data.002".to_string(), 5));
    }

    #[test]
    fn an_exact_multiple_leaves_no_empty_trailing_part() {
        let dir = tempfile::tempdir().unwrap();
        written(dir.path(), 10, &payload(20));
        assert_eq!(contents(dir.path()).len(), 2);
        let again = tempfile::tempdir().unwrap();
        written(again.path(), 10, &payload(21));
        assert_eq!(contents(again.path()).len(), 3);
    }

    #[test]
    fn an_empty_blob_still_writes_data_000() {
        let dir = tempfile::tempdir().unwrap();
        let touched = written(dir.path(), 10, b"");
        assert_eq!(touched, ["data.000"]);
        assert_eq!(contents(dir.path()), [("data.000".to_string(), 0)]);
    }

    #[test]
    fn stale_parts_are_removed_on_shrink_and_kept_names_overwritten_on_grow() {
        let dir = tempfile::tempdir().unwrap();
        written(dir.path(), 10, &payload(45));
        let touched = written(dir.path(), 10, b"tiny");
        assert_eq!(touched[0], "data.000");
        assert_eq!(
            touched[1..],
            ["data.001", "data.002", "data.003", "data.004"]
        );
        assert_eq!(contents(dir.path()), [("data.000".to_string(), 4)]);
        written(dir.path(), 10, &payload(12));
        assert_eq!(contents(dir.path()).len(), 2);
    }

    #[test]
    fn writer_one_byte_per_write_equals_one_big_write() {
        let data = payload(37);
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        written(a.path(), 8, &data);
        let mut w = PartWriter::new(b.path(), 8);
        write_all_one_byte(&mut w, &data);
        w.finish().unwrap();
        assert_eq!(contents(a.path()), contents(b.path()));
        let names = part_names(b.path()).unwrap();
        let joined: Vec<u8> = names
            .iter()
            .flat_map(|n| fs::read(b.path().join(n)).unwrap())
            .collect();
        assert_eq!(joined, data);
    }

    fn reader_over(dir: &Path) -> PartReader {
        let files = part_names(dir)
            .unwrap()
            .iter()
            .map(|n| dir.join(n))
            .collect();
        PartReader::new(files, Box::new(()))
    }

    #[test]
    fn reader_yields_every_boundary_in_order_also_one_byte_at_a_time() {
        for size in [1, 7, 10, 25, 40] {
            let dir = tempfile::tempdir().unwrap();
            let data = payload(25);
            written(dir.path(), size, &data);
            let mut all = Vec::new();
            reader_over(dir.path()).read_to_end(&mut all).unwrap();
            assert_eq!(all, data, "size {size}");
            let mut slow = Vec::new();
            OneByte(reader_over(dir.path()))
                .read_to_end(&mut slow)
                .unwrap();
            assert_eq!(slow, data, "one byte, size {size}");
        }
    }

    #[test]
    fn reader_never_returns_zero_before_all_parts_are_consumed() {
        let dir = tempfile::tempdir().unwrap();
        for (i, body) in [&b"ab"[..], b"", b"", b"cd"].iter().enumerate() {
            fs::write(dir.path().join(part_name(i)), body).unwrap();
        }
        let mut reader = reader_over(dir.path());
        let mut buf = [0u8; 8];
        assert_eq!(reader.read(&mut buf).unwrap(), 2);
        assert_eq!(reader.read(&mut buf).unwrap(), 2, "empty parts are skipped");
        assert_eq!(reader.read(&mut buf).unwrap(), 0);
        assert_eq!(reader.read(&mut buf).unwrap(), 0, "stays at the end");
    }

    #[test]
    fn reader_holds_the_guard_until_it_is_dropped() {
        struct Flag(std::rc::Rc<std::cell::Cell<bool>>);
        impl Drop for Flag {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }
        let dropped = std::rc::Rc::new(std::cell::Cell::new(false));
        let mut reader = PartReader::new(Vec::new(), Box::new(Flag(dropped.clone())));
        assert_eq!(reader.read(&mut [0u8; 4]).unwrap(), 0);
        assert!(!dropped.get(), "alive after reaching the end");
        drop(reader);
        assert!(dropped.get());
    }

    #[test]
    fn reader_reports_an_unreadable_part_as_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let reader = PartReader::new(vec![dir.path().join("data.000")], Box::new(()));
        let got = OneByte(reader).read_to_end(&mut Vec::new());
        assert_eq!(got.map_err(|e| e.kind()), Err(ErrorKind::NotFound));
    }
}
