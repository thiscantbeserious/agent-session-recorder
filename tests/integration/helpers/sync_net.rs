//! Simulated machines on one shared remote, for the multi-machine sync tests.

use agr::files::{backup, codec};
use agr::sync::testing::{
    index, open_meta, open_stream, seal_stream, sha256_hex, BlobError, MemoryBackend,
};
use agr::sync::{BackendEngine, Key, Options, Report};
use agr::sync::{BackendError, BlobSource, RemoteEntry, SyncBackend};
use agr::{Config, StorageManager};
use std::cell::{Cell, RefCell};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub const PATH: &str = "claude/a.cast.gz";

/// One machine, its own storage dir and index.
pub struct Mach {
    temp: TempDir,
    storage: StorageManager,
}

pub fn mach() -> Mach {
    let temp = TempDir::new().unwrap();
    let mut config = Config::default();
    config.storage.directory = temp.path().to_string_lossy().to_string();
    Mach {
        storage: StorageManager::new(config),
        temp,
    }
}

pub fn overwrite() -> Options {
    Options {
        overwrite: true,
        ..Options::default()
    }
}

impl Mach {
    pub fn dir(&self) -> &Path {
        self.temp.path()
    }

    pub fn file(&self) -> PathBuf {
        self.dir().join(PATH)
    }

    /// Write a session whose text is `text`.
    pub fn write(&self, text: &str) {
        fs::create_dir_all(self.dir().join("claude")).unwrap();
        let body = format!("{{\"version\":3}}\n[0.1,\"o\",\"{text}\"]\n");
        codec::atomic_write(&self.file(), |w| Ok(w.write_all(body.as_bytes())?)).unwrap();
    }

    /// Plaintext text of the session, `None` when the file is absent.
    pub fn text(&self) -> Option<String> {
        let mut text = String::new();
        codec::open_reader(&self.file())
            .ok()?
            .read_to_string(&mut text)
            .ok()?;
        Some(text)
    }

    pub fn bytes(&self) -> Option<Vec<u8>> {
        fs::read(self.file()).ok()
    }

    pub fn bak(&self) -> Option<Vec<u8>> {
        fs::read(backup::backup_path_for(&self.file())).ok()
    }

    fn engine<'a>(&'a self, backend: &'a dyn SyncBackend, key: &'a Key) -> BackendEngine<'a> {
        BackendEngine {
            backend,
            key,
            storage: &self.storage,
        }
    }

    /// Push never touches a local file.
    pub fn push(&self, backend: &dyn SyncBackend, key: &Key, opts: &Options) -> Report {
        let before = self.bytes();
        let report = self.engine(backend, key).push(opts).unwrap();
        assert_eq!(self.bytes(), before, "a push changed a local file");
        report
    }

    /// A local file only changes with `--overwrite` or when it still equals the last sync, and
    /// the old content is in the `.bak` then.
    pub fn pull(&self, backend: &dyn SyncBackend, key: &Key, opts: &Options) -> Report {
        let before = self.bytes();
        let last = self.index_entry().map(|e| e.sha256);
        let report = self.engine(backend, key).pull(opts).unwrap();
        if let Some(old) = before.filter(|old| Some(old) != self.bytes().as_ref()) {
            assert!(
                opts.overwrite || last == Some(sha256_hex(&old)),
                "silent overwrite"
            );
            assert_eq!(self.bak(), Some(old), "overwrite without a .bak");
        }
        report
    }

    pub fn index_entry(&self) -> Option<agr::sync::testing::index::IndexEntry> {
        index::load(self.dir()).0.entries.get(PATH).cloned()
    }

    pub fn pending(&self) -> bool {
        self.index_entry().is_some_and(|e| e.pending)
    }
}

/// Plaintext (the gzip file bytes) of a remote gist at its manifest gen.
pub fn remote_bytes(backend: &MemoryBackend, key: &Key, id: &str) -> Vec<u8> {
    let meta = open_meta(key, &backend.manifest(id).unwrap()).unwrap();
    let blob = backend.blob(id).unwrap();
    open_blob(key, &meta.path, meta.gen, &blob).unwrap()
}

/// Whole-blob fixture over the streaming sealer, never on the production path.
pub fn seal_blob(key: &Key, path: &str, gen: u64, plain: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    seal_stream(key, path, gen, plain, &mut out).unwrap();
    out
}

pub fn open_blob(key: &Key, path: &str, gen: u64, blob: &[u8]) -> Result<Vec<u8>, BlobError> {
    let mut out = Vec::new();
    open_stream(key, path, gen, blob, &mut out)?;
    Ok(out)
}

pub fn only_id(backend: &MemoryBackend) -> String {
    let mut ids: Vec<String> = backend.list().unwrap().into_iter().map(|e| e.id).collect();
    assert_eq!(ids.len(), 1, "{ids:?}");
    ids.remove(0)
}

/// One more round on every machine: no stuck gist, nothing pending, `gists` entries remain.
pub fn assert_settled(backend: &MemoryBackend, key: &Key, machines: &[&Mach], gists: usize) {
    for m in machines {
        m.push(backend, key, &Options::default());
        m.pull(backend, key, &Options::default());
    }
    assert_eq!(backend.len(), gists, "gist count");
    for (i, m) in machines.iter().enumerate() {
        assert!(!m.pending(), "machine {i} still pending");
    }
}

type Hook<'a> = Box<dyn FnOnce() + 'a>;

/// Memory backend with injected races: `before` runs once before the first push or describe,
/// so another machine's whole push lands after this machine listed.
#[derive(Default)]
pub struct Net<'a> {
    pub inner: Option<&'a MemoryBackend>,
    before: RefCell<Option<Hook<'a>>>,
    pub drop_describe: Cell<bool>,
    pub fail_push: RefCell<Option<BackendError>>,
    /// The push lands, the reply is lost.
    pub lose_reply: Cell<bool>,
}

impl<'a> Net<'a> {
    pub fn new(inner: &'a MemoryBackend) -> Self {
        Self {
            inner: Some(inner),
            ..Self::default()
        }
    }

    pub fn before(&self, hook: impl FnOnce() + 'a) {
        *self.before.borrow_mut() = Some(Box::new(hook));
    }

    fn mem(&self) -> &MemoryBackend {
        self.inner.unwrap()
    }

    fn race(&self) {
        if let Some(hook) = self.before.borrow_mut().take() {
            hook();
        }
    }
}

impl SyncBackend for Net<'_> {
    fn name(&self) -> &'static str {
        "net"
    }
    fn check(&self) -> Result<(), BackendError> {
        self.mem().check()
    }
    fn list(&self) -> Result<Vec<RemoteEntry>, BackendError> {
        self.mem().list()
    }
    fn create(&self) -> Result<String, BackendError> {
        self.mem().create()
    }
    fn push(&self, id: &str, e: Option<&str>, m: &str, b: BlobSource) -> Result<(), BackendError> {
        self.race();
        if let Some(err) = self.fail_push.borrow_mut().take() {
            return Err(err);
        }
        let landed = self.mem().push(id, e, m, b);
        match landed {
            Ok(()) if self.lose_reply.take() => Err(BackendError::Unreachable("reply lost".into())),
            other => other,
        }
    }
    fn describe(&self, id: &str, t: &str) -> Result<(), BackendError> {
        self.race();
        if self.drop_describe.get() {
            return Ok(());
        }
        self.mem().describe(id, t)
    }
    fn pull(&self, id: &str) -> Result<(Option<String>, Box<dyn std::io::Read>), BackendError> {
        self.mem().pull(id)
    }
    fn delete(&self, id: &str) -> Result<(), BackendError> {
        self.mem().delete(id)
    }
}
