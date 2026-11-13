//! The release workflow must publish SHA256SUMS next to the binaries (ADR 5.1).

use std::fs;
use std::path::PathBuf;

fn workflow() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml");
    fs::read_to_string(path).expect("release.yml readable")
}

/// Text of one step, from its `- name:` line up to the next step.
fn step(text: &str, name: &str) -> String {
    let marker = format!("- name: {name}");
    let start = text.find(&marker).unwrap_or_else(|| panic!("step {name}"));
    let rest = &text[start + marker.len()..];
    let end = rest.find("\n      - name:").unwrap_or(rest.len());
    rest[..end].to_string()
}

#[test]
fn release_assets_step_writes_sha256sums() {
    let step = step(&workflow(), "Prepare release assets");
    assert!(
        step.contains("sha256sum agr-* > SHA256SUMS"),
        "step text: {step}"
    );
    assert!(step.contains("cd release-assets"), "step text: {step}");
}

#[test]
fn release_upload_covers_sha256sums() {
    let step = step(&workflow(), "Create GitHub Release");
    assert!(
        step.contains("files: release-assets/*"),
        "step text: {step}"
    );
}

#[test]
#[ignore = "needs agr::update::platform from Stage 2"]
fn release_matrix_matches_update_assets() {
    unimplemented!("filled in Stage 2");
}
