//! Fake GitHub release on disk, served through `AGR_UPDATE_RELEASE_BASE=file://<dir>`.
//!
//! Layout: `<dir>/latest.json`, `<dir>/download/<asset>`, `<dir>/download/SHA256SUMS`.
//! The asset is a tiny Rust stub compiled with `rustc`, so it has real magic bytes.

use agr::update::platform;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// What the fake release looks like. `Default` is a good 0.5.0 release.
#[derive(Default, Clone)]
pub struct Spec {
    /// Release version, empty means 0.5.0.
    pub version: String,
    /// Leave `SHA256SUMS` out of the release.
    pub no_sums: bool,
    /// Write a wrong digest into `SHA256SUMS`.
    pub wrong_sum: bool,
    /// Flip one byte of the asset after the digest was taken.
    pub flip_byte: bool,
    /// Advertise this many bytes more than the asset has.
    pub size_extra: u64,
    /// Use a shell script as asset, with a correct digest.
    pub script_asset: bool,
    /// Version the stub prints, empty means the release version.
    pub stub_prints: String,
    /// Exit code of the stub.
    pub stub_exit: i32,
}

pub struct FakeRelease {
    pub dir: TempDir,
}

impl FakeRelease {
    pub fn base(&self) -> String {
        format!("file://{}", self.dir.path().display())
    }

    pub fn asset_path(&self) -> PathBuf {
        self.dir
            .path()
            .join("download")
            .join(platform::current_asset().unwrap())
    }
}

fn compile_stub(dir: &Path, prints: &str, exit: i32) -> Vec<u8> {
    let src = dir.join("stub.rs");
    let out = dir.join("stub");
    let code = format!(
        "fn main() {{ if std::env::args().nth(1).as_deref() == Some(\"--version\") {{ \
         println!(\"agr {prints}-dev+abc1234 (x, built y)\"); std::process::exit({exit}); }} }}"
    );
    fs::write(&src, code).unwrap();
    let status = Command::new("rustc")
        .arg(&src)
        .arg("-o")
        .arg(&out)
        .status()
        .expect("rustc");
    assert!(status.success(), "stub compile failed");
    fs::read(&out).unwrap()
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn build(spec: &Spec) -> FakeRelease {
    let dir = TempDir::new().unwrap();
    let version = if spec.version.is_empty() {
        "0.5.0"
    } else {
        &spec.version
    };
    let prints = if spec.stub_prints.is_empty() {
        version
    } else {
        &spec.stub_prints
    };
    let dl = dir.path().join("download");
    fs::create_dir(&dl).unwrap();
    let name = platform::current_asset().unwrap();
    let mut bytes = if spec.script_asset {
        b"#!/bin/sh\necho agr 0.5.0\n".to_vec()
    } else {
        compile_stub(dir.path(), prints, spec.stub_exit)
    };
    let sum = if spec.wrong_sum {
        "0".repeat(64)
    } else {
        digest(&bytes)
    };
    if spec.flip_byte {
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0xff;
    }
    fs::write(dl.join(name), &bytes).unwrap();
    let mut assets = vec![(name.to_string(), bytes.len() as u64 + spec.size_extra)];
    if !spec.no_sums {
        let text = format!("{sum}  {name}\n");
        fs::write(dl.join("SHA256SUMS"), &text).unwrap();
        assets.push(("SHA256SUMS".to_string(), text.len() as u64));
    }
    write_json(dir.path(), version, &assets);
    FakeRelease { dir }
}

fn write_json(dir: &Path, version: &str, assets: &[(String, u64)]) {
    let list: Vec<String> = assets
        .iter()
        .map(|(n, s)| {
            format!(
                "{{\"name\":\"{n}\",\"size\":{s},\"browser_download_url\":\"file://{}/download/{n}\"}}",
                dir.display()
            )
        })
        .collect();
    let json = format!(
        "{{\"tag_name\":\"v{version}\",\"draft\":false,\"prerelease\":false,\"assets\":[{}]}}",
        list.join(",")
    );
    fs::write(dir.join("latest.json"), json).unwrap();
}
