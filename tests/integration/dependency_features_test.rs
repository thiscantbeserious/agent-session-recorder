//! Security-relevant cargo features must stay enabled, checked on the resolved build.

use std::process::Command;

fn resolved_features(krate: &str) -> Vec<String> {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--locked"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata runs");
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).expect("metadata is json");
    let ids: Vec<&str> = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"] == krate)
        .filter_map(|p| p["id"].as_str())
        .collect();
    meta["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| ids.contains(&n["id"].as_str().unwrap_or("")))
        .flat_map(|n| n["features"].as_array().cloned().unwrap_or_default())
        .filter_map(|f| f.as_str().map(str::to_string))
        .collect()
}

#[test]
fn poly1305_wipes_its_one_time_key_on_drop() {
    // poly1305 0.9 has no ZeroizeOnDrop marker, its Drop wipes only with this feature.
    let features = resolved_features("poly1305");
    assert!(features.iter().any(|f| f == "zeroize"), "{features:?}");
}

#[test]
fn chacha20_wipes_its_key_on_drop() {
    let features = resolved_features("chacha20");
    assert!(features.iter().any(|f| f == "zeroize"), "{features:?}");
}
