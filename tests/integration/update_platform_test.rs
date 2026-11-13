//! Release asset names per platform and the refusal messages.

use agr::update::platform::{asset_name, SUPPORTED};
use agr::update::RELEASES_URL;
use std::fs;
use std::path::PathBuf;

#[test]
fn maps_supported_targets() {
    assert_eq!(asset_name("macos", "aarch64").unwrap(), "agr-macos-arm64");
    assert_eq!(asset_name("macos", "x86_64").unwrap(), "agr-macos-x86_64");
    assert_eq!(asset_name("linux", "x86_64").unwrap(), "agr-linux-x86_64");
}

#[test]
fn windows_is_refused_without_retry() {
    let msg = asset_name("windows", "x86_64").unwrap_err().to_string();
    assert!(msg.contains("not supported on Windows"), "{msg}");
    assert!(msg.contains("no Windows release"), "{msg}");
    assert!(msg.contains(RELEASES_URL), "{msg}");
    assert!(
        !msg.contains("retry") && !msg.contains("try again"),
        "{msg}"
    );
}

#[test]
fn unsupported_names_detected_and_supported() {
    let msg = asset_name("linux", "aarch64").unwrap_err().to_string();
    assert!(msg.contains("linux-aarch64"), "{msg}");
    for name in ["macos-arm64", "macos-x86_64", "linux-x86_64"] {
        assert!(msg.contains(name), "{msg}");
    }
}

#[test]
fn release_matrix_matches_update_assets() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml");
    let text = fs::read_to_string(path).unwrap();
    let mut matrix: Vec<String> = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("artifact: "))
        .map(|a| format!("agr-{a}"))
        .collect();
    let mut want: Vec<String> = SUPPORTED.iter().map(|s| s.to_string()).collect();
    matrix.sort();
    want.sort();
    assert_eq!(matrix, want);
}
