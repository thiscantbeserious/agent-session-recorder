//! Release JSON, `SHA256SUMS` parsing and the checksum gate.

use super::version::{self, Version};
use super::{CHECKSUM_FILE, MIN_UPDATABLE, RELEASES_URL};
use anyhow::{anyhow, bail, Result};
use serde::Deserialize;

/// One downloadable file of a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    pub url: String,
}

/// The latest release as far as the updater cares.
#[derive(Debug, Clone)]
pub struct Release {
    pub tag: String,
    pub version: Version,
    pub assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<RawAsset>,
}

/// Parse the release JSON. Every asset URL must start with `prefix`.
pub fn parse_release(json: &str, prefix: &str) -> Result<Release> {
    let raw: RawRelease =
        serde_json::from_str(json).map_err(|e| anyhow!("malformed release response: {e}"))?;
    if raw.draft || raw.prerelease {
        bail!(
            "release {} is a draft or prerelease, refusing it",
            raw.tag_name
        );
    }
    let version =
        version::parse(&raw.tag_name).map_err(|e| anyhow!("malformed release response: {e}"))?;
    let mut assets = Vec::new();
    for a in raw.assets {
        if !a.browser_download_url.starts_with(prefix) {
            bail!(
                "malformed release response: asset URL {} is outside {prefix}",
                a.browser_download_url
            );
        }
        assets.push(Asset {
            name: a.name,
            size: a.size,
            url: a.browser_download_url,
        });
    }
    Ok(Release {
        tag: raw.tag_name,
        version,
        assets,
    })
}

/// Find the digest of `name` in a `SHA256SUMS` text. Every line is checked.
pub fn checksum_for(text: &str, name: &str) -> Result<String> {
    let mut found: Option<String> = None;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let (digest, file) = split_line(line)?;
        if file == name && found.replace(digest.to_string()).is_some() {
            bail!("{CHECKSUM_FILE} lists {name} twice");
        }
    }
    found.ok_or_else(|| anyhow!("{CHECKSUM_FILE} has no entry for {name}"))
}

fn split_line(line: &str) -> Result<(&str, &str)> {
    let bad = || anyhow!("bad {CHECKSUM_FILE} line: {line:?}");
    let (digest, rest) = line.split_at_checked(64).ok_or_else(bad)?;
    let ok = digest
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    let file = rest.strip_prefix("  ").or_else(|| rest.strip_prefix(" *"));
    match file {
        Some(f) if ok && !f.is_empty() => Ok((digest, f)),
        _ => Err(bad()),
    }
}

/// R7: only releases from the minimum on, with a checksum file, can be applied.
pub fn gate(release: &Release) -> Result<()> {
    let has_sums = release.assets.iter().any(|a| a.name == CHECKSUM_FILE);
    if release.version < MIN_UPDATABLE || !has_sums {
        bail!(
            "Release {} has no checksums, so agr update cannot verify it. \
             Install it manually from {RELEASES_URL}.",
            release.tag
        );
    }
    Ok(())
}

/// The asset called `name`, or an error naming it and the release.
pub fn find_asset<'a>(release: &'a Release, name: &str) -> Result<&'a Asset> {
    release
        .assets
        .iter()
        .find(|a| a.name == name)
        .ok_or_else(|| anyhow!("Release {} has no asset {name}", release.tag))
}
