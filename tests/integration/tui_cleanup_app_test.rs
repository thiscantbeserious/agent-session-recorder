//! Unit tests for the cleanup app.

use agr::tui::cleanup_app::*;
use agr::tui::testing::cleanup_app::*;

#[test]
fn mode_default_is_normal() {
    assert_eq!(Mode::default(), Mode::Normal);
}

#[test]
fn mode_equality() {
    assert_eq!(Mode::Search, Mode::Search);
    assert_ne!(Mode::Search, Mode::Normal);
    assert_ne!(Mode::GlobSelect, Mode::Search);
}

#[test]
#[allow(clippy::clone_on_copy)]
fn mode_clone_and_copy() {
    let mode = Mode::Help;
    let cloned = mode.clone();
    let copied = mode;
    assert_eq!(cloned, copied);
}

#[test]
fn mode_debug_format() {
    let mode = Mode::ConfirmDelete;
    let debug = format!("{:?}", mode);
    assert!(debug.contains("ConfirmDelete"));
}

#[test]
fn glob_mode_exists() {
    let mode = Mode::GlobSelect;
    let debug = format!("{:?}", mode);
    assert!(debug.contains("GlobSelect"));
}

// Glob matching tests

#[test]
fn glob_match_exact() {
    assert!(glob_match("test.cast", "test.cast"));
    assert!(!glob_match("test.cast", "other.cast"));
}

#[test]
fn glob_match_star_any() {
    assert!(glob_match("test.cast", "*"));
    assert!(glob_match("test.cast", "*.cast"));
    assert!(glob_match("test.cast", "test.*"));
    assert!(glob_match("test.cast", "*test*"));
    assert!(glob_match("session_2024_01.cast", "*2024*"));
}

#[test]
fn glob_match_question_single() {
    assert!(glob_match("test.cast", "tes?.cast"));
    assert!(glob_match("test.cast", "????.cast"));
    assert!(!glob_match("test.cast", "???.cast"));
}

#[test]
fn glob_match_case_insensitive() {
    assert!(glob_match("TEST.CAST", "test.cast"));
    assert!(glob_match("Test.Cast", "TEST.CAST"));
    assert!(glob_match("MyFile.cast", "*myfile*"));
}

#[test]
fn glob_match_complex_patterns() {
    assert!(glob_match(
        "session_2024_01_15.cast",
        "session_????_??_??.cast"
    ));
    assert!(glob_match("claude_session.cast", "*_session.cast"));
    assert!(!glob_match("test.txt", "*.cast"));
}

// parse_glob_pattern tests

#[test]
fn parse_glob_bare_pattern() {
    let (agent, name) = parse_glob_pattern("foo");
    assert_eq!(agent, None);
    assert_eq!(name, "foo");
}

#[test]
fn parse_glob_slash_split() {
    let (agent, name) = parse_glob_pattern("agent/name");
    assert_eq!(agent, Some("agent"));
    assert_eq!(name, "name");
}

#[test]
fn parse_glob_multiple_slashes() {
    // Only the first slash is used as the separator
    let (agent, name) = parse_glob_pattern("a/b/c");
    assert_eq!(agent, Some("a"));
    assert_eq!(name, "b/c");
}

#[test]
fn parse_glob_empty() {
    let (agent, name) = parse_glob_pattern("");
    assert_eq!(agent, None);
    assert_eq!(name, "");
}

#[test]
fn parse_glob_leading_slash() {
    let (agent, name) = parse_glob_pattern("/name");
    assert_eq!(agent, Some(""));
    assert_eq!(name, "name");
}

#[test]
fn parse_glob_trailing_slash() {
    let (agent, name) = parse_glob_pattern("agent/");
    assert_eq!(agent, Some("agent"));
    assert_eq!(name, "");
}

// matches_glob_pattern tests

#[test]
fn matches_no_agent_filter() {
    // None agent_filter: any agent is accepted, only name is matched
    assert!(matches_glob_pattern("any-agent", "session.cast", None, "*"));
    assert!(matches_glob_pattern("claude", "hello.cast", None, "*.cast"));
    assert!(!matches_glob_pattern("claude", "hello.txt", None, "*.cast"));
}

#[test]
fn matches_both_agent_and_name() {
    assert!(matches_glob_pattern(
        "claude",
        "session.cast",
        Some("claude"),
        "*.cast"
    ));
}

#[test]
fn matches_agent_only() {
    // Agent matches but name does not → false
    assert!(!matches_glob_pattern(
        "claude",
        "session.txt",
        Some("claude"),
        "*.cast"
    ));
}

#[test]
fn matches_name_only() {
    // Name matches but agent does not → false
    assert!(!matches_glob_pattern(
        "gpt",
        "session.cast",
        Some("claude"),
        "*.cast"
    ));
}

#[test]
fn matches_agent_glob_with_star() {
    // "*" agent pattern must match any agent string
    assert!(matches_glob_pattern(
        "claude",
        "session.cast",
        Some("*"),
        "*.cast"
    ));
    assert!(matches_glob_pattern(
        "gpt-4",
        "notes.cast",
        Some("*"),
        "notes.cast"
    ));
}

// glob_match edge-case tests

#[test]
fn glob_match_empty_empty() {
    // Both empty: pattern is satisfied
    assert!(glob_match("", ""));
}

#[test]
fn glob_match_empty_pattern() {
    // Non-empty text against empty pattern: no match
    assert!(!glob_match("text", ""));
}

#[test]
fn glob_match_empty_text() {
    // Non-wildcard pattern against empty text: no match
    assert!(!glob_match("", "pattern"));
}

#[test]
fn glob_match_star_matches_empty() {
    // "*" alone must match the empty string
    assert!(glob_match("", "*"));
}

#[test]
fn glob_match_multibyte_utf8() {
    assert!(glob_match("café.cast", "*.cast"));
    assert!(glob_match("über.cast", "?ber.cast"));
    assert!(glob_match("日本語.cast", "*.cast"));
}

// Deleting compressed sessions

#[test]
fn delete_paths_removes_gzip_session_with_hidden_backup() {
    let dir = tempfile::tempdir().unwrap();
    let session = dir.path().join("x.cast.gz");
    let backup = dir.path().join(".x.cast.gz.bak");
    std::fs::write(&session, [0u8; 40]).unwrap();
    std::fs::write(&backup, b"bak").unwrap();

    let (deleted, freed) = delete_paths(&[session.to_string_lossy().to_string()]);

    assert_eq!(deleted, 1);
    assert_eq!(freed, 40);
    assert!(!session.exists());
    assert!(!backup.exists());
}

#[test]
fn deletion_status_reports_on_disk_size_freed() {
    assert_eq!(
        deletion_status(2, 2, 2048),
        "Deleted 2 sessions (freed 2 KiB)"
    );
    assert!(deletion_status(1, 2, 0).contains("1/2"));
}
