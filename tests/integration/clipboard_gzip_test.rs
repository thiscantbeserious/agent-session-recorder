//! Clipboard tests for `.cast.gz` sources. Fake tools only, the system clipboard is never touched.
#![cfg(unix)] // the uid named temp dir is unix only, Windows only gets a compile check

use agr::clipboard::copy::Copy;
use agr::clipboard::tool::{CopyTool, CopyToolError};
use agr::clipboard::{ClipboardError, CopyMethod, CopyResult, MAX_CONTENT_SIZE};
use agr::files::codec;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tempfile::TempDir;

const SESSION: &str = "{\"version\":3}\n[0.1,\"o\",\"hi\"]\n";

/// Records what it is handed: the file path plus its content at call time, or the text.
struct Recorder {
    file_ok: bool,
    can_files: bool,
    file: Mutex<Option<(PathBuf, String)>>,
    text: Mutex<Option<String>>,
}

impl Recorder {
    fn new(file_ok: bool, can_files: bool) -> Self {
        Self {
            file_ok,
            can_files,
            file: Mutex::new(None),
            text: Mutex::new(None),
        }
    }
}

impl CopyTool for &'static Recorder {
    fn method(&self) -> CopyMethod {
        CopyMethod::Xclip
    }
    fn is_available(&self) -> bool {
        true
    }
    fn can_copy_files(&self) -> bool {
        self.can_files
    }
    fn try_copy_file(&self, path: &Path) -> Result<(), CopyToolError> {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        *self.file.lock().unwrap() = Some((path.to_path_buf(), content));
        if self.file_ok {
            Ok(())
        } else {
            Err(CopyToolError::Failed("boom".into()))
        }
    }
    fn try_copy_text(&self, text: &str) -> Result<(), CopyToolError> {
        *self.text.lock().unwrap() = Some(text.to_string());
        Ok(())
    }
}

fn leak(r: Recorder) -> &'static Recorder {
    Box::leak(Box::new(r))
}

fn write_gz(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    codec::atomic_write(&path, |w| Ok(w.write_all(content.as_bytes())?)).unwrap();
    path
}

fn copier(rec: &'static Recorder, base: &Path) -> Copy {
    Copy::with_tools(vec![Box::new(rec)]).with_temp_base(base.to_path_buf())
}

fn temp_dir(base: &Path) -> PathBuf {
    base.join(format!("agr-clipboard-{}", my_uid()))
}

fn my_uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

fn leftovers(base: &Path) -> Vec<String> {
    let dir = temp_dir(base);
    match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => vec![],
    }
}

#[test]
fn gzip_file_copy_hands_tool_plain_cast_with_decompressed_content() {
    let dir = TempDir::new().unwrap();
    let base = TempDir::new().unwrap();
    let gz = write_gz(dir.path(), "x.cast.gz", SESSION);
    let rec = leak(Recorder::new(true, true));

    let result = copier(rec, base.path()).file(&gz).unwrap();

    assert!(matches!(result, CopyResult::FileCopied { .. }));
    let (path, content) = rec.file.lock().unwrap().clone().unwrap();
    assert_eq!(path, temp_dir(base.path()).join("x.cast"));
    assert_eq!(content, SESSION);
}

#[test]
fn gzip_temp_dir_is_cleared_before_next_copy() {
    let dir = TempDir::new().unwrap();
    let base = TempDir::new().unwrap();
    let a = write_gz(dir.path(), "a.cast.gz", SESSION);
    let b = write_gz(dir.path(), "b.cast.gz", SESSION);
    let rec = leak(Recorder::new(true, true));
    let copy = copier(rec, base.path());

    copy.file(&a).unwrap();
    copy.file(&b).unwrap();

    assert_eq!(leftovers(base.path()), vec!["b.cast".to_string()]);
}

#[test]
fn gzip_temp_is_removed_when_every_tool_fails() {
    let dir = TempDir::new().unwrap();
    let base = TempDir::new().unwrap();
    let gz = write_gz(dir.path(), "x.cast.gz", SESSION);
    struct AllFail;
    impl CopyTool for AllFail {
        fn method(&self) -> CopyMethod {
            CopyMethod::Xclip
        }
        fn is_available(&self) -> bool {
            true
        }
        fn can_copy_files(&self) -> bool {
            true
        }
        fn try_copy_file(&self, _: &Path) -> Result<(), CopyToolError> {
            Err(CopyToolError::Failed("f".into()))
        }
        fn try_copy_text(&self, _: &str) -> Result<(), CopyToolError> {
            Err(CopyToolError::Failed("t".into()))
        }
    }
    let copy = Copy::with_tools(vec![Box::new(AllFail)]).with_temp_base(base.path().to_path_buf());

    let result = copy.file(&gz);

    assert!(matches!(result, Err(ClipboardError::NoToolAvailable)));
    assert!(leftovers(base.path()).is_empty());
}

#[test]
fn gzip_text_fallback_copies_decompressed_text() {
    let dir = TempDir::new().unwrap();
    let base = TempDir::new().unwrap();
    let gz = write_gz(dir.path(), "x.cast.gz", SESSION);
    let rec = leak(Recorder::new(true, false));

    let result = copier(rec, base.path()).file(&gz);

    assert!(
        matches!(result, Ok(CopyResult::ContentCopied { size_bytes, .. }) if size_bytes == SESSION.len()),
        "{result:?}"
    );
    assert_eq!(rec.text.lock().unwrap().as_deref(), Some(SESSION));
}

#[test]
fn gzip_text_fallback_limit_uses_decompressed_size() {
    let dir = TempDir::new().unwrap();
    let base = TempDir::new().unwrap();
    let big = "a".repeat(MAX_CONTENT_SIZE as usize + 1);
    let gz = write_gz(dir.path(), "big.cast.gz", &big);
    assert!(std::fs::metadata(&gz).unwrap().len() < MAX_CONTENT_SIZE);
    let rec = leak(Recorder::new(true, false));

    let result = copier(rec, base.path()).file(&gz);

    assert!(matches!(result, Err(ClipboardError::FileTooLarge { .. })));
    assert!(rec.text.lock().unwrap().is_none());
    assert!(leftovers(base.path()).is_empty());
}

#[test]
fn gzip_file_reference_has_no_size_limit() {
    let dir = TempDir::new().unwrap();
    let base = TempDir::new().unwrap();
    let big = "a".repeat(MAX_CONTENT_SIZE as usize + 1);
    let gz = write_gz(dir.path(), "big.cast.gz", &big);
    let rec = leak(Recorder::new(true, true));

    let result = copier(rec, base.path()).file(&gz);

    assert!(
        matches!(result, Ok(CopyResult::FileCopied { .. })),
        "{result:?}"
    );
    assert_eq!(leftovers(base.path()), vec!["big.cast".to_string()]);
}

#[cfg(unix)]
#[test]
fn gzip_temp_dir_is_created_0700() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    let base = TempDir::new().unwrap();
    let gz = write_gz(dir.path(), "x.cast.gz", SESSION);
    let rec = leak(Recorder::new(true, true));

    copier(rec, base.path()).file(&gz).unwrap();

    let mode = std::fs::metadata(temp_dir(base.path()))
        .map(|m| m.permissions().mode() & 0o777)
        .ok();
    assert_eq!(mode, Some(0o700));
}

#[cfg(unix)]
#[test]
fn gzip_temp_dir_symlink_is_refused() {
    let dir = TempDir::new().unwrap();
    let base = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), temp_dir(base.path())).unwrap();
    let gz = write_gz(dir.path(), "x.cast.gz", SESSION);
    let rec = leak(Recorder::new(true, true));

    let result = copier(rec, base.path()).file(&gz);

    assert!(result.is_err(), "{result:?}");
    assert!(rec.file.lock().unwrap().is_none());
    assert_eq!(std::fs::read_dir(elsewhere.path()).unwrap().count(), 0);
}

#[test]
fn plain_cast_is_handed_to_tool_unchanged() {
    let dir = TempDir::new().unwrap();
    let base = TempDir::new().unwrap();
    let plain = dir.path().join("p.cast");
    std::fs::write(&plain, SESSION).unwrap();
    let rec = leak(Recorder::new(true, true));

    copier(rec, base.path()).file(&plain).unwrap();

    let (path, _) = rec.file.lock().unwrap().clone().unwrap();
    assert_eq!(path, plain);
    assert!(leftovers(base.path()).is_empty());
}
