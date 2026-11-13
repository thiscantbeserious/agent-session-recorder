//! Install location, locking, staging, verification and the single rename.

use super::fetch::Source;
use super::release::{checksum_for, find_asset, gate, parse_release};
use super::version::{parse_version_output, Version};
use super::{platform, Outcome, CHECKSUM_FILE, RELEASES_URL};
use crate::utils::process_guard::ProcessGuard;
use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Who owns the install, so agr must not replace it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Managed {
    Cargo,
    Homebrew,
}

impl Managed {
    pub fn message(self, path: &Path) -> String {
        let how = match self {
            Managed::Cargo => {
                "cargo install --git https://github.com/thiscantbeserious/agent-session-recorder"
            }
            Managed::Homebrew => "brew upgrade",
        };
        format!(
            "agr at {} is managed by {self:?}, update it with: {how}",
            path.display()
        )
    }
}

/// Test seams of `run_hooked`.
pub struct Hooks<'a> {
    pub guard: ProcessGuard,
    /// Called with the staged path right before the rename.
    pub before_rename: Option<&'a dyn Fn(&Path)>,
}

/// Which package manager owns `path`, if any. Paths are expected to be canonical.
pub fn managed_install(
    path: &Path,
    home: Option<&Path>,
    cargo_home: Option<&Path>,
    _brew_prefix: Option<&Path>,
) -> Option<Managed> {
    let cargo_bin = cargo_home
        .map(Path::to_path_buf)
        .or_else(|| home.map(|h| h.join(".cargo")))
        .map(|c| c.join("bin"))
        .map(|b| fs::canonicalize(&b).unwrap_or(b));
    if cargo_bin.is_some_and(|b| path.starts_with(b)) {
        return Some(Managed::Cargo);
    }
    // Homebrew owns only what resolves into its Cellar, a prefix like /usr/local is shared.
    let cellar = path.components().any(|c| c.as_os_str() == "Cellar");
    if cellar {
        return Some(Managed::Homebrew);
    }
    None
}

/// Fail when the real path of `path` is cargo or Homebrew managed.
pub fn check_managed(
    path: &Path,
    home: Option<&Path>,
    cargo_home: Option<&Path>,
    brew_prefix: Option<&Path>,
) -> Result<()> {
    let real = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let canon =
        |p: Option<&Path>| p.map(|p| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()));
    let (home, cargo, brew) = (canon(home), canon(cargo_home), canon(brew_prefix));
    match managed_install(&real, home.as_deref(), cargo.as_deref(), brew.as_deref()) {
        Some(m) => bail!("{}", m.message(&real)),
        None => Ok(()),
    }
}

/// The directory must be writable and the binary owned by the current user.
pub fn ensure_writable(target: &Path) -> Result<()> {
    let dir = target
        .parent()
        .ok_or_else(|| anyhow!("no directory for {}", target.display()))?;
    let cdir = std::ffi::CString::new(dir.as_os_str().as_encoded_bytes())?;
    let writable = unsafe { libc::access(cdir.as_ptr(), libc::W_OK) } == 0;
    let owned = fs::metadata(target)?.uid() == unsafe { libc::geteuid() };
    if !writable || !owned {
        bail!(
            "{} is not writable by you, re-run with the right permissions or install agr elsewhere, \
             or install manually from {RELEASES_URL}",
            dir.display()
        );
    }
    Ok(())
}

/// Hold an exclusive non-blocking flock on the binary for the update.
fn lock(target: &Path) -> Result<File> {
    let file = File::open(target)?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        bail!("another agr update is running for {}", target.display());
    }
    Ok(file)
}

/// Removes the staged file on drop, also on every error path.
struct Staged(PathBuf);

impl Drop for Staged {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn stage(dir: &Path) -> Result<Staged> {
    let path = dir.join(format!(".agr.update-{}", std::process::id()));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .with_context(|| format!("cannot create {}", path.display()))?;
    Ok(Staged(path))
}

fn magic_ok(bytes: &[u8]) -> bool {
    let magic: &[u8] = if cfg!(target_os = "macos") {
        &[0xcf, 0xfa, 0xed, 0xfe]
    } else {
        &[0x7f, b'E', b'L', b'F']
    };
    bytes.starts_with(magic)
}

/// Checks 1 to 3 of the ADR: size, SHA-256, magic bytes.
fn verify_file(path: &Path, size: u64, want: &str) -> Result<()> {
    let bytes = fs::read(path)?;
    if bytes.len() as u64 != size {
        bail!(
            "download is {} bytes, expected {size}, truncated or oversized",
            bytes.len()
        );
    }
    let got: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if got != want {
        bail!("checksum mismatch: got {got}, expected {want}");
    }
    if !magic_ok(&bytes) {
        bail!("download is not an executable for this platform");
    }
    Ok(())
}

/// Spawn `--version`, retrying while the file is still open for writing elsewhere (ETXTBSY).
fn spawn_version(path: &Path) -> Result<std::process::Child> {
    let mut tries = 0;
    loop {
        let spawned = Command::new(path)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        match spawned {
            Err(e) if e.raw_os_error() == Some(libc::ETXTBSY) && tries < 50 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(20));
            }
            other => return other.context("cannot run the downloaded agr"),
        }
    }
}

/// Check 5 and 6: run `--version` with a 10 s limit and compare it with the release.
fn smoke_run(path: &Path, want: Version) -> Result<()> {
    let mut child = spawn_version(path)?;
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if start.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("downloaded agr --version timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    if !status.success() {
        bail!("downloaded agr --version failed with {status}");
    }
    let mut out = String::new();
    child.stdout.take().map(|mut s| s.read_to_string(&mut out));
    let got = parse_version_output(&out)?;
    if got != want {
        bail!("downloaded agr reports {got}, release is {want}");
    }
    Ok(())
}

fn interrupted(hooks: &Hooks) -> Result<()> {
    if hooks.guard.is_interrupted() {
        bail!("interrupted");
    }
    Ok(())
}

/// The update flow after the managed check, see ADR 2. `target` is the real binary path.
pub fn run_hooked(
    source: &Source,
    target: &Path,
    current: Version,
    hooks: &Hooks,
) -> Result<Outcome> {
    ensure_writable(target)?;
    let _lock = lock(target)?;
    let release = parse_release(&source.fetch_api()?, &source.prefix)?;
    if release.version <= current {
        return Ok(Outcome::UpToDate(current));
    }
    gate(&release)?;
    let name = platform::current_asset()?;
    let asset = find_asset(&release, name)?;
    let sums = source.fetch_text(&find_asset(&release, CHECKSUM_FILE)?.url)?;
    let want = checksum_for(&sums, name)?;
    interrupted(hooks)?;
    let staged = stage(target.parent().unwrap_or(Path::new(".")))?;
    source.fetch_file(&asset.url, &staged.0)?;
    verify_file(&staged.0, asset.size, &want)?;
    let meta = fs::metadata(target)?;
    let mode = meta.permissions().mode() & 0o7777;
    // The staged file gets our effective group, give it the group of the target.
    if fs::metadata(&staged.0)?.gid() != meta.gid() {
        std::os::unix::fs::chown(&staged.0, None, Some(meta.gid()))
            .context("cannot keep the group of the installed binary, it is unchanged")?;
    }
    fs::set_permissions(&staged.0, fs::Permissions::from_mode(mode))?;
    smoke_run(&staged.0, release.version)?;
    File::open(&staged.0)?.sync_all()?;
    interrupted(hooks)?;
    if let Some(hook) = hooks.before_rename {
        hook(&staged.0);
    }
    fs::rename(&staged.0, target)
        .with_context(|| format!("cannot replace {}, it is unchanged", target.display()))?;
    if let Some(dir) = target.parent() {
        let _ = File::open(dir).and_then(|d| d.sync_all());
    }
    Ok(Outcome::Updated {
        from: current,
        to: release.version,
    })
}

/// `run_hooked` with no hooks, for tests and simple callers.
pub fn run_at(source: &Source, target: &Path, current: Version) -> Result<Outcome> {
    run_hooked(
        source,
        target,
        current,
        &Hooks {
            guard: ProcessGuard::new(),
            before_rename: None,
        },
    )
}
