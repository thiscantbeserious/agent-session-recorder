//! curl wrapper: allowlist, failure mapping, redirects, argument bounds, override.

use crate::helpers::update_fake::{build, Spec};
use agr::update::fetch::{
    classify_exit, classify_status, curl_args, follow, host_allowed, run_curl, Hop, Kind, Source,
};
use agr::update::version::Version;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn host_allowlist() {
    for ok in [
        "https://api.github.com/repos/x",
        "https://github.com/x",
        "https://release-assets.githubusercontent.com/x",
        "https://objects.githubusercontent.com/x",
    ] {
        assert!(host_allowed(ok), "{ok}");
    }
    for bad in [
        "https://evil.com/x",
        "https://github.com.evil.com/x",
        "https://user@github.com/x",
        "http://github.com/x",
        "https://github.com:443/x",
        "https:///x",
    ] {
        assert!(!host_allowed(bad), "{bad}");
    }
}

#[test]
fn classify_curl_exit_codes() {
    for (code, word) in [
        (6, "DNS"),
        (7, "connect"),
        (28, "timeout"),
        (63, "too large"),
    ] {
        let msg = classify_exit(code, "");
        assert!(msg.contains(word), "{code}: {msg}");
        assert_eq!(msg.lines().count(), 1, "{msg}");
    }
}

#[test]
fn classify_http_status() {
    assert!(classify_status(403).unwrap().contains("rate limit"));
    assert!(classify_status(429).unwrap().contains("rate limit"));
    assert!(classify_status(404).unwrap().contains("404"));
    assert!(classify_status(200).is_none());
}

#[test]
fn redirect_limit() {
    let to = |_: &str| Ok(Hop::Redirect("https://github.com/next".to_string()));
    let msg = follow("https://github.com/a", to).unwrap_err().to_string();
    assert!(msg.contains("redirect"), "{msg}");
    let mut left = 3;
    let three = |_: &str| {
        if left > 0 {
            left -= 1;
            Ok(Hop::Redirect("https://github.com/next".to_string()))
        } else {
            Ok(Hop::Done(b"ok".to_vec()))
        }
    };
    assert_eq!(follow("https://github.com/a", three).unwrap(), b"ok");
    let evil = |_: &str| Ok(Hop::Redirect("https://evil.com/x".to_string()));
    let msg = follow("https://github.com/a", evil)
        .unwrap_err()
        .to_string();
    assert!(msg.contains("evil.com"), "{msg}");
}

#[test]
fn missing_curl_says_so() {
    let empty = TempDir::new().unwrap();
    let err = run_curl(&["--version".to_string()], Some(empty.path().as_os_str())).unwrap_err();
    assert_eq!(
        err.to_string(),
        "agr update needs curl, install it and retry"
    );
}

#[test]
fn curl_args_are_bounded() {
    let out = Path::new("/tmp/x");
    let args = curl_args("https://github.com/a", Kind::Binary, Some(out));
    let has = |a: &str, b: &str| args.windows(2).any(|w| w[0] == a && w[1] == b);
    assert!(has("--proto", "=https"));
    assert!(args.iter().any(|a| a == "--connect-timeout"));
    assert!(has("--max-time", "300"));
    assert!(has("--max-filesize", "64M"));
    assert!(!args.iter().any(|a| a == "--location" || a == "-L"));
    assert!(!args.iter().any(|a| a.contains("Authorization")));
}

#[test]
fn override_rejects_http() {
    assert!(Source::from_override("http://x").is_err());
    assert!(Source::from_override("https://x").is_ok());
    assert!(Source::from_override("file:///tmp/x").is_ok());
}

#[test]
fn file_override_fetches_fixture() {
    let fake = build(&Spec::default());
    let source = Source::from_override(&fake.base()).unwrap();
    let report = agr::update::check_with(&source, Version(0, 4, 0)).unwrap();
    assert_eq!(report.latest, Version(0, 5, 0));
}
