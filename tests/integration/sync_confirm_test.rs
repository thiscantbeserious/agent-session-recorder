//! The prompts of `agr sync delete`, library level with a fake reader and an explicit tty flag.

use agr::sync::confirm::{confirm_delete, Confirm, Flags};

fn run(count: usize, yes: bool, force: bool, tty: bool, input: &str) -> (Confirm, String) {
    let mut out = Vec::new();
    let flags = Flags { yes, force, tty };
    let got = confirm_delete(count, &flags, &mut input.as_bytes(), &mut out).unwrap();
    (got, String::from_utf8(out).unwrap())
}

#[test]
fn one_prompt_up_to_ten_matches() {
    let (got, out) = run(10, false, false, true, "n\n");
    assert_eq!(got, Confirm::Declined);
    assert!(!out.contains("Type"), "{out}");
    assert_eq!(run(10, false, false, true, "y\n").0, Confirm::Go);
}

#[test]
fn over_ten_matches_asks_to_type_the_count() {
    let (got, out) = run(11, false, false, true, "y\n7\n");
    assert!(out.contains("Type 11 to confirm"), "{out}");
    assert_eq!(got, Confirm::Declined);
    assert_eq!(run(11, false, false, true, "y\n11\n").0, Confirm::Go);
}

#[test]
fn a_no_to_the_first_prompt_never_asks_for_the_count() {
    let (got, out) = run(11, false, false, true, "n\n11\n");
    assert_eq!(got, Confirm::Declined);
    assert!(!out.contains("Type"), "{out}");
}

#[test]
fn yes_skips_only_the_first_prompt() {
    let (got, out) = run(11, true, false, true, "3\n");
    assert!(out.contains("Type 11 to confirm"), "{out}");
    assert!(!out.contains("[y/N]"), "{out}");
    assert_eq!(got, Confirm::Declined);
}

#[test]
fn over_ten_without_a_terminal_needs_force() {
    assert_eq!(run(11, true, false, false, "").0, Confirm::NeedForce);
    assert_eq!(run(11, true, true, false, "").0, Confirm::Go);
}

#[test]
fn without_a_terminal_one_match_still_needs_yes() {
    let (got, out) = run(1, false, false, false, "y\n");
    assert_eq!(got, Confirm::Declined);
    assert!(out.contains("--yes"), "{out}");
}
