//! Release asset name per operating system and architecture.

use super::RELEASES_URL;
use anyhow::{bail, Result};

/// Asset names of the release build matrix, `agr-` prefix included.
pub const SUPPORTED: [&str; 3] = ["agr-macos-arm64", "agr-macos-x86_64", "agr-linux-x86_64"];

/// Map `std::env::consts` OS and ARCH to the release asset name.
pub fn asset_name(os: &str, arch: &str) -> Result<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Ok(SUPPORTED[0]),
        ("macos", "x86_64") => Ok(SUPPORTED[1]),
        ("linux", "x86_64") => Ok(SUPPORTED[2]),
        ("windows", _) => bail!(
            "agr update is not supported on Windows because no Windows release exists. \
             Install manually from {RELEASES_URL}."
        ),
        _ => bail!(
            "agr update does not support {os}-{arch}, supported: {}.",
            SUPPORTED.map(|n| n.trim_start_matches("agr-")).join(", ")
        ),
    }
}

/// Asset name for the running platform.
pub fn current_asset() -> Result<&'static str> {
    asset_name(std::env::consts::OS, std::env::consts::ARCH)
}
