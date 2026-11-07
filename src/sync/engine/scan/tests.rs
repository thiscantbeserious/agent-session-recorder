use super::*;

#[test]
fn valid_path_rejects_colon() {
    assert!(valid_path("claude/a.cast.gz"));
    assert!(!valid_path("claude/C:x.cast.gz"));
    assert!(!valid_path("C:/x.cast.gz"));
}
