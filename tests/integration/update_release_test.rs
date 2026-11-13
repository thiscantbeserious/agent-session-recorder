//! Release JSON, SHA256SUMS parsing and the checksum gate.

use agr::update::release::{checksum_for, find_asset, gate, parse_release};
use agr::update::RELEASES_URL;

const PREFIX: &str =
    "https://github.com/thiscantbeserious/agent-session-recorder/releases/download/";

fn json(tag: &str, assets: &[&str], extra: &str) -> String {
    let list: Vec<String> = assets
        .iter()
        .map(|n| {
            format!(
                "{{\"name\":\"{n}\",\"size\":12,\"browser_download_url\":\"{PREFIX}{tag}/{n}\"}}"
            )
        })
        .collect();
    format!(
        "{{\"tag_name\":\"{tag}\"{extra},\"assets\":[{}]}}",
        list.join(",")
    )
}

const HEX: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn latest_json_parses_tag_and_assets() {
    let r = parse_release(&json("v0.5.0", &["agr-linux-x86_64"], ""), PREFIX).unwrap();
    assert_eq!(r.tag, "v0.5.0");
    assert_eq!(r.assets.len(), 1);
    assert_eq!(r.assets[0].name, "agr-linux-x86_64");
    assert_eq!(r.assets[0].size, 12);
    assert_eq!(r.assets[0].url, format!("{PREFIX}v0.5.0/agr-linux-x86_64"));
}

#[test]
fn draft_or_prerelease_is_rejected() {
    for extra in [",\"draft\":true", ",\"prerelease\":true"] {
        assert!(
            parse_release(&json("v0.5.0", &[], extra), PREFIX).is_err(),
            "{extra}"
        );
    }
}

#[test]
fn asset_url_outside_prefix_is_malformed() {
    let bad = "https://github.com/evil/repo/releases/download/v0.5.0/agr-linux-x86_64";
    let text = json("v0.5.0", &["agr-linux-x86_64"], "")
        .replace(&format!("{PREFIX}v0.5.0/agr-linux-x86_64"), bad);
    let msg = parse_release(&text, PREFIX).unwrap_err().to_string();
    assert!(msg.contains(bad), "{msg}");
}

#[test]
fn sha256sums_accepts_text_and_binary_flag() {
    let text = format!("{HEX}  agr-linux-x86_64\n{HEX} *agr-macos-arm64\n");
    assert_eq!(checksum_for(&text, "agr-linux-x86_64").unwrap(), HEX);
    assert_eq!(checksum_for(&text, "agr-macos-arm64").unwrap(), HEX);
}

#[test]
fn sha256sums_rejects_bad_lines() {
    let upper = format!("{}  agr-linux-x86_64\n", HEX.to_uppercase());
    let short = format!("{}  agr-linux-x86_64\n", &HEX[..63]);
    let dup = format!("{HEX}  agr-linux-x86_64\n{HEX}  agr-linux-x86_64\n");
    let other = format!("{HEX}  agr-macos-arm64\n");
    for text in [upper, short, dup, other] {
        assert!(checksum_for(&text, "agr-linux-x86_64").is_err(), "{text}");
    }
}

#[test]
fn gate_refuses_below_minimum() {
    let r = parse_release(&json("v0.3.0", &["SHA256SUMS"], ""), PREFIX).unwrap();
    let msg = gate(&r).unwrap_err().to_string();
    assert!(
        msg.contains("no checksums") && msg.contains(RELEASES_URL),
        "{msg}"
    );
}

#[test]
fn gate_refuses_missing_checksum_file() {
    let r = parse_release(&json("v0.5.0", &["agr-linux-x86_64"], ""), PREFIX).unwrap();
    let msg = gate(&r).unwrap_err().to_string();
    assert!(
        msg.contains("no checksums") && msg.contains(RELEASES_URL),
        "{msg}"
    );
}

#[test]
fn gate_passes_from_minimum() {
    let r = parse_release(&json("v0.4.0", &["SHA256SUMS"], ""), PREFIX).unwrap();
    assert!(gate(&r).is_ok());
}

#[test]
fn missing_platform_asset_names_asset_and_version() {
    let r = parse_release(&json("v0.5.0", &["SHA256SUMS"], ""), PREFIX).unwrap();
    let msg = find_asset(&r, "agr-linux-x86_64").unwrap_err().to_string();
    assert!(
        msg.contains("agr-linux-x86_64") && msg.contains("v0.5.0"),
        "{msg}"
    );
}
