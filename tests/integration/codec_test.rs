//! Tests for the session codec (plain and gzip)

use agr::files::codec;
use flate2::read::GzDecoder;
use std::io::{BufRead, Read};
use std::path::Path;
use tempfile::TempDir;

fn write_bytes(path: &Path, bytes: &[u8]) {
    codec::atomic_write(path, |w| Ok(w.write_all(bytes)?)).unwrap();
}

#[test]
fn session_path_accepts_visible_cast_and_gz() {
    assert!(codec::is_session_path(Path::new("d/x.cast")));
    assert!(codec::is_session_path(Path::new("d/x.cast.gz")));
}

#[test]
fn session_path_rejects_hidden_and_other_files() {
    for p in [
        ".x.cast.gz.bak",
        ".x.cast.gz.tmp",
        ".x.cast.lock",
        "x.gz",
        "x.txt",
    ] {
        assert!(!codec::is_session_path(Path::new(p)), "{p}");
    }
}

#[test]
fn is_gzip_path_checks_suffix() {
    assert!(codec::is_gzip_path(Path::new("x.cast.gz")));
    assert!(!codec::is_gzip_path(Path::new("x.cast")));
}

#[test]
fn session_stem_strips_known_suffixes() {
    assert_eq!(codec::session_stem("a.cast.gz"), "a");
    assert_eq!(codec::session_stem("a.cast"), "a");
    assert_eq!(codec::session_stem("a.txt"), "a.txt");
}

#[test]
fn session_suffix_returns_matching_suffix() {
    assert_eq!(codec::session_suffix(Path::new("a.cast.gz")), ".cast.gz");
    assert_eq!(codec::session_suffix(Path::new("a.cast")), ".cast");
}

#[test]
fn temp_path_is_hidden_with_full_suffix() {
    assert_eq!(
        codec::temp_path_for(Path::new("dir/x.cast.gz")),
        Path::new("dir/.x.cast.gz.tmp")
    );
}

#[test]
fn atomic_write_gzip_decodes_to_input() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("x.cast.gz");
    write_bytes(&p, b"hello\n");
    let mut out = String::new();
    GzDecoder::new(std::fs::File::open(&p).unwrap())
        .read_to_string(&mut out)
        .unwrap();
    assert_eq!(out, "hello\n");
}

#[test]
fn atomic_write_plain_yields_raw_bytes() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("x.cast");
    write_bytes(&p, b"hello\n");
    assert_eq!(std::fs::read(&p).unwrap(), b"hello\n");
}

#[test]
fn atomic_write_removes_temp_on_error() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("x.cast.gz");
    let res = codec::atomic_write(&p, |_| anyhow::bail!("boom"));
    assert!(res.is_err());
    assert!(!codec::temp_path_for(&p).exists());
    assert!(!p.exists());
}

#[test]
fn gzip_output_is_deterministic() {
    let dir = TempDir::new().unwrap();
    let (a, b) = (dir.path().join("a.cast.gz"), dir.path().join("b.cast.gz"));
    write_bytes(&a, b"same content\n");
    write_bytes(&b, b"same content\n");
    assert_eq!(std::fs::read(a).unwrap(), std::fs::read(b).unwrap());
}

#[test]
fn gzip_header_has_zero_mtime_and_no_name() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("a.cast.gz");
    write_bytes(&p, b"x\n");
    let bytes = std::fs::read(p).unwrap();
    assert_eq!(&bytes[..2], &[0x1f, 0x8b]);
    assert_eq!(bytes[3] & 0x08, 0, "FNAME flag set");
    assert_eq!(&bytes[4..8], &[0, 0, 0, 0]);
}

#[test]
fn open_reader_yields_same_lines_for_plain_and_gzip() {
    let dir = TempDir::new().unwrap();
    let (a, b) = (dir.path().join("a.cast"), dir.path().join("a.cast.gz"));
    write_bytes(&a, b"one\ntwo\n");
    write_bytes(&b, b"one\ntwo\n");
    let lines = |p: &Path| {
        codec::open_reader(p)
            .unwrap()
            .lines()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(lines(&a), lines(&b));
}

fn big_gz(dir: &TempDir) -> std::path::PathBuf {
    let p = dir.path().join("big.cast.gz");
    let data: String = (0..2000).map(|i| format!("line {i}\n")).collect();
    write_bytes(&p, data.as_bytes());
    p
}

#[test]
fn truncated_gzip_error_names_path() {
    let dir = TempDir::new().unwrap();
    let p = big_gz(&dir);
    let bytes = std::fs::read(&p).unwrap();
    std::fs::write(&p, &bytes[..bytes.len() / 2]).unwrap();
    let mut s = String::new();
    let err = codec::open_reader(&p)
        .unwrap()
        .read_to_string(&mut s)
        .unwrap_err();
    assert!(err.to_string().contains("big.cast.gz"), "{err}");
}

#[test]
fn corrupt_gzip_trailer_error_names_path() {
    let dir = TempDir::new().unwrap();
    let p = big_gz(&dir);
    let mut bytes = std::fs::read(&p).unwrap();
    let n = bytes.len();
    bytes[n - 6] ^= 0xff;
    std::fs::write(&p, &bytes).unwrap();
    let mut s = String::new();
    let err = codec::open_reader(&p)
        .unwrap()
        .read_to_string(&mut s)
        .unwrap_err();
    assert!(err.to_string().contains("big.cast.gz"), "{err}");
}

#[test]
fn transcode_roundtrip_is_lossless() {
    let dir = TempDir::new().unwrap();
    let (a, b, c) = (
        dir.path().join("a.cast"),
        dir.path().join("a.cast.gz"),
        dir.path().join("b.cast"),
    );
    write_bytes(&a, b"{\"v\":3}\nx\n");
    codec::transcode(&a, &b).unwrap();
    codec::transcode(&b, &c).unwrap();
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&c).unwrap());
}

#[test]
fn same_content_compares_decoded_bytes() {
    let dir = TempDir::new().unwrap();
    let (a, b, c) = (
        dir.path().join("a.cast"),
        dir.path().join("a.cast.gz"),
        dir.path().join("c.cast.gz"),
    );
    write_bytes(&a, b"abc\n");
    write_bytes(&b, b"abc\n");
    write_bytes(&c, b"abd\n");
    assert!(codec::same_content(&a, &b).unwrap());
    assert!(!codec::same_content(&a, &c).unwrap());
}

fn chain(err: &anyhow::Error) -> String {
    format!("{err:#}")
}

#[test]
fn atomic_write_create_error_names_temp_path() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("missing").join("x.cast.gz");
    let err = codec::atomic_write(&p, |_| Ok(())).unwrap_err();
    assert!(chain(&err).contains(".x.cast.gz.tmp"), "{}", chain(&err));
}

#[test]
fn atomic_write_closure_error_names_target_path() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("x.cast.gz");
    let err = codec::atomic_write(&p, |_| anyhow::bail!("boom")).unwrap_err();
    let msg = chain(&err);
    assert!(msg.contains("x.cast.gz") && msg.contains("boom"), "{msg}");
}

#[test]
fn atomic_write_rename_error_names_target_path() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("x.cast");
    std::fs::create_dir(&p).unwrap();
    std::fs::write(p.join("keep"), b"k").unwrap();
    let err = codec::atomic_write(&p, |w| Ok(w.write_all(b"a")?)).unwrap_err();
    assert!(chain(&err).contains("x.cast"), "{}", chain(&err));
    assert!(!codec::temp_path_for(&p).exists());
}

#[test]
fn open_reader_missing_file_names_path() {
    let err = codec::open_reader(Path::new("/nope/gone.cast.gz"))
        .err()
        .unwrap();
    assert!(chain(&err).contains("gone.cast.gz"), "{}", chain(&err));
}

#[test]
fn session_path_rejects_hidden_bak_of_gzip() {
    assert!(!codec::is_session_path(Path::new("d/.x.cast.gz.bak")));
}

#[test]
fn same_content_detects_length_mismatch() {
    let dir = TempDir::new().unwrap();
    let (a, b) = (dir.path().join("a.cast"), dir.path().join("b.cast.gz"));
    write_bytes(&a, b"abc\n");
    write_bytes(&b, b"abc\nmore\n");
    assert!(!codec::same_content(&a, &b).unwrap());
    assert!(!codec::same_content(&b, &a).unwrap());
}

#[test]
fn session_path_rejects_hidden_even_with_session_suffix() {
    assert!(!codec::is_session_path(Path::new("d/.x.cast.gz")));
    assert!(!codec::is_session_path(Path::new("d/.x.cast")));
    assert!(!codec::is_session_path(Path::new("d/x.cast.gz.bak")));
}

fn gz_member(body: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(body).unwrap();
    enc.finish().unwrap()
}

#[test]
fn open_reader_decodes_concatenated_gzip_members() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("m.cast.gz");
    let mut bytes = gz_member(b"{\"version\":3}\n");
    bytes.extend(gz_member(b"[0.1,\"o\",\"hi\"]\n"));
    std::fs::write(&path, bytes).unwrap();

    let mut text = String::new();
    let read = codec::open_reader(&path).unwrap().read_to_string(&mut text);

    assert!(read.is_ok(), "{read:?}");
    assert_eq!(text, "{\"version\":3}\n[0.1,\"o\",\"hi\"]\n");
}

#[cfg(unix)]
#[test]
fn failed_dir_fsync_after_rename_still_returns_ok() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    // write+exec without read: rename works, opening the dir for fsync does not
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o300)).unwrap();
    let path = dir.path().join("s.cast");

    let result = codec::atomic_write(&path, |w| Ok(w.write_all(b"x")?));

    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(std::fs::read(&path).unwrap(), b"x");
}

#[cfg(unix)]
mod warning_sink {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Mutex;

    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn record(message: &str) {
        SEEN.lock().unwrap().push(message.to_string());
    }

    #[test]
    fn failed_dir_fsync_goes_to_the_installed_sink() {
        let dir = TempDir::new().unwrap();
        // write+exec but no read: create and rename work, opening the dir for fsync fails.
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o300)).unwrap();
        codec::set_warning_sink(Some(record));

        let result = codec::atomic_write(&dir.path().join("x.cast"), |w| Ok(w.write_all(b"a")?));

        codec::set_warning_sink(None);
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        result.unwrap();
        // The sink is process wide, other fsync tests may add their own messages.
        let dir_name = dir.path().display().to_string();
        let seen = SEEN.lock().unwrap();
        let mine = seen.iter().filter(|m| m.contains(&dir_name)).count();
        assert_eq!(mine, 1, "{seen:?}");
    }
}
