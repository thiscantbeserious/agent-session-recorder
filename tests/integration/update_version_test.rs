//! Version parsing and comparison for `agr update`.

use agr::update::version::{parse, parse_version_output, Version};

#[test]
fn parse_accepts_three_numbers() {
    assert_eq!(parse("0.4.0").unwrap(), Version(0, 4, 0));
    assert_eq!(parse("v0.4.0").unwrap(), Version(0, 4, 0));
}

#[test]
fn parse_rejects_garbage() {
    for bad in ["0.4", "a.b.c", ""] {
        assert!(parse(bad).is_err(), "{bad:?} must be rejected");
    }
}

#[test]
fn compare_orders_numerically() {
    assert!(parse("0.10.0").unwrap() > parse("0.9.9").unwrap());
    assert!(parse("1.0.0").unwrap() > parse("0.99.0").unwrap());
    assert_eq!(parse("1.2.3").unwrap(), parse("v1.2.3").unwrap());
}

#[test]
fn version_output_dev_and_release() {
    let want = Version(0, 4, 0);
    assert_eq!(
        parse_version_output("agr 0.4.0-dev+abc1234 (x, built y)").unwrap(),
        want
    );
    assert_eq!(
        parse_version_output("agr 0.4.0 (x, built y)\n").unwrap(),
        want
    );
}

#[test]
fn version_output_rejects_other_program() {
    assert!(parse_version_output("curl 8.4.0").is_err());
    assert!(parse_version_output("").is_err());
}
