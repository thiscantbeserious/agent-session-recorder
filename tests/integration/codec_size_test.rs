//! Tests for the exact uncompressed size stored in the gzip header (FEXTRA `AG`)

use agr::files::codec::{self, UncompressedSize};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::{Compression, GzBuilder};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const SIZE_OFFSET: usize = 16;

fn write_bytes(path: &Path, bytes: &[u8]) {
    codec::atomic_write(path, |w| Ok(w.write_all(bytes)?)).unwrap();
}

fn header_size(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes[SIZE_OFFSET..SIZE_OFFSET + 8].try_into().unwrap())
}

/// Gzip bytes with the `AG` subfield claiming `claimed`, body is one tiny block.
fn gz_with_size(claimed: u64) -> Vec<u8> {
    let mut extra = b"AG".to_vec();
    extra.extend_from_slice(&8u16.to_le_bytes());
    extra.extend_from_slice(&claimed.to_le_bytes());
    let mut enc = GzBuilder::new()
        .extra(extra)
        .write(Vec::new(), Compression::default());
    enc.write_all(b"tiny").unwrap();
    enc.finish().unwrap()
}

/// Foreign gzip (no FEXTRA): a 10 byte header, `body_len` zero bytes, CRC, given ISIZE.
fn foreign_gz(body_len: usize, isize: u32) -> Vec<u8> {
    let mut bytes = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3];
    bytes.resize(10 + body_len, 0);
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&isize.to_le_bytes());
    bytes
}

fn put(dir: &TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.path().join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

#[test]
fn gzip_header_stores_exact_size_in_ag_subfield() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("a.cast.gz");
    write_bytes(&p, &vec![b'a'; 1234]);
    let bytes = std::fs::read(&p).unwrap();
    assert_eq!(bytes[3], 0x04, "only FEXTRA set");
    assert_eq!(&bytes[10..12], &12u16.to_le_bytes(), "XLEN");
    assert_eq!(&bytes[12..16], &[b'A', b'G', 8, 0], "subfield id and len");
    assert_eq!(header_size(&bytes), 1234);
}

#[test]
fn gzip_header_size_counts_multiple_writes() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("a.cast.gz");
    codec::atomic_write(&p, |w| {
        w.write_all(b"abc")?;
        w.write_all(b"defg")?;
        Ok(())
    })
    .unwrap();
    assert_eq!(header_size(&std::fs::read(&p).unwrap()), 7);
}

#[test]
fn gzip_with_extra_decodes_and_stays_deterministic() {
    let dir = TempDir::new().unwrap();
    let (a, b) = (dir.path().join("a.cast.gz"), dir.path().join("b.cast.gz"));
    write_bytes(&a, b"hello\n");
    write_bytes(&b, b"hello\n");
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    let mut out = String::new();
    GzDecoder::new(std::fs::File::open(&a).unwrap())
        .read_to_string(&mut out)
        .unwrap();
    assert_eq!(out, "hello\n");
}

#[test]
fn gzip_tool_accepts_header_with_extra() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("a.cast.gz");
    write_bytes(&p, b"hello\n");
    let Ok(status) = std::process::Command::new("gzip")
        .arg("-t")
        .arg(&p)
        .status()
    else {
        return;
    };
    assert!(status.success());
}

#[test]
fn uncompressed_size_plain_is_file_length() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("a.cast");
    write_bytes(&p, b"12345");
    let got = codec::uncompressed_size(&p).unwrap();
    assert_eq!(got, UncompressedSize::Exact(5));
}

#[test]
fn uncompressed_size_reads_subfield_for_our_files() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("a.cast.gz");
    write_bytes(&p, &vec![b'a'; 1000]);
    let got = codec::uncompressed_size(&p).unwrap();
    assert_eq!(got, UncompressedSize::Exact(1000));
}

#[test]
fn uncompressed_size_subfield_beats_trailer_for_1gb() {
    let dir = TempDir::new().unwrap();
    let p = put(&dir, "a.cast.gz", &gz_with_size(1_000_000_000));
    let got = codec::uncompressed_size(&p).unwrap();
    assert_eq!(got, UncompressedSize::Exact(1_000_000_000));
}

#[test]
fn uncompressed_size_subfield_handles_over_4gib() {
    let dir = TempDir::new().unwrap();
    let p = put(&dir, "a.cast.gz", &gz_with_size(5_000_000_000));
    let got = codec::uncompressed_size(&p).unwrap();
    assert_eq!(got, UncompressedSize::Exact(5_000_000_000));
}

#[test]
fn trailer_fallback_is_exact_for_foreign_gzip() {
    let dir = TempDir::new().unwrap();
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(&vec![b'a'; 1000]).unwrap();
    let p = put(&dir, "a.cast.gz", &enc.finish().unwrap());
    let got = codec::uncompressed_size(&p).unwrap();
    assert_eq!(got, UncompressedSize::Exact(1000));
}

#[test]
fn trailer_fallback_1gb_at_10x_is_exact() {
    let dir = TempDir::new().unwrap();
    let p = put(&dir, "a.cast.gz", &foreign_gz(100_000_000, 1_000_000_000));
    let got = codec::uncompressed_size(&p).unwrap();
    assert_eq!(got, UncompressedSize::Exact(1_000_000_000));
}

#[test]
fn trailer_fallback_over_4gib_when_isize_below_on_disk() {
    let dir = TempDir::new().unwrap();
    let p = put(&dir, "a.cast.gz", &foreign_gz(5_000, 100));
    let got = codec::uncompressed_size(&p).unwrap();
    assert_eq!(got, UncompressedSize::Over4GiB);
}

#[test]
fn trailer_fallback_on_multi_member_gzip_reports_last_member_only() {
    // Documented limit: foreign multi-member gzip shows the last member's ISIZE.
    let dir = TempDir::new().unwrap();
    let mut bytes = foreign_gz(0, 1000);
    bytes.extend(foreign_gz(0, 1000));
    let path = put(&dir, "m.cast.gz", &bytes);

    assert_eq!(
        codec::uncompressed_size(&path).unwrap(),
        UncompressedSize::Exact(1000)
    );
}
