//! `agr update`: check and replace the running binary from GitHub releases.
//!
//! Library code never prints, the command handler does. See the self-update ADR.

pub mod fetch;
#[cfg(unix)]
pub mod install;
pub mod platform;
pub mod release;
pub mod version;

use anyhow::Result;
use fetch::Source;
use version::Version;

pub const API_URL: &str =
    "https://api.github.com/repos/thiscantbeserious/agent-session-recorder/releases/latest";
pub const ASSET_PREFIX: &str =
    "https://github.com/thiscantbeserious/agent-session-recorder/releases/download/";
pub const RELEASES_URL: &str =
    "https://github.com/thiscantbeserious/agent-session-recorder/releases";
pub const CHECKSUM_FILE: &str = "SHA256SUMS";
/// First release that ships `SHA256SUMS`.
pub const MIN_UPDATABLE: Version = Version(0, 4, 0);

/// Result of `--check`.
#[derive(Debug, Clone)]
pub struct Report {
    pub current: Version,
    pub latest: Version,
    pub tag: String,
    /// False when the release cannot be applied because it has no checksums (R7).
    pub verifiable: bool,
}

impl Report {
    pub fn update_available(&self) -> bool {
        self.latest > self.current
    }
}

/// What `run` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    UpToDate(Version),
    Updated { from: Version, to: Version },
}

/// The running version, from `CARGO_PKG_VERSION`.
pub fn running_version() -> Result<Version> {
    version::parse(env!("CARGO_PKG_VERSION"))
}

/// `agr update --check`: platform check, then the latest release, nothing written.
pub fn check() -> Result<Report> {
    platform::current_asset()?;
    check_with(&Source::from_env()?, running_version()?)
}

/// Compare `current` with the latest release of `source`.
pub fn check_with(source: &Source, current: Version) -> Result<Report> {
    let release = release::parse_release(&source.fetch_api()?, &source.prefix)?;
    Ok(Report {
        current,
        latest: release.version,
        verifiable: release::gate(&release).is_ok(),
        tag: release.tag,
    })
}

/// `agr update`: the full flow of the ADR, for the running binary.
#[cfg(unix)]
pub fn run() -> Result<Outcome> {
    platform::current_asset()?;
    let target = std::fs::canonicalize(std::env::current_exe()?)?;
    let env_path = |k: &str| std::env::var_os(k).map(std::path::PathBuf::from);
    install::check_managed(
        &target,
        dirs::home_dir().as_deref(),
        env_path("CARGO_HOME").as_deref(),
        env_path("HOMEBREW_PREFIX").as_deref(),
    )?;
    let guard = crate::utils::process_guard::ProcessGuard::new();
    guard.register_signal_handlers();
    let hooks = install::Hooks {
        guard,
        before_rename: None,
    };
    install::run_hooked(&Source::from_env()?, &target, running_version()?, &hooks)
}

#[cfg(not(unix))]
pub fn run() -> Result<Outcome> {
    platform::current_asset()?;
    anyhow::bail!("agr update is not supported on this platform")
}
