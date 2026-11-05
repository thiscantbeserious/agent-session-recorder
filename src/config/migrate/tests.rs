//! Unit tests for the private section-ordering helpers of the config migration.

use super::*;

// -----------------------------------------------------------------------
// collect_section_blocks
// -----------------------------------------------------------------------

#[test]
fn collect_section_blocks_empty_headers() {
    let headers: Vec<(usize, String)> = vec![];
    let lines: Vec<&str> = vec!["key = 1"];
    let blocks = collect_section_blocks(&headers, &lines);
    assert!(blocks.is_empty());
}

#[test]
fn collect_section_blocks_single_section() {
    let lines = vec!["[shell]", "auto_wrap = true"];
    let headers = vec![(0usize, "shell".to_string())];
    let blocks = collect_section_blocks(&headers, &lines);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].0, "shell");
    assert!(blocks[0].1.contains("auto_wrap = true"));
}

#[test]
fn collect_section_blocks_merges_non_consecutive_same_group() {
    // [agents] then [shell] then [agents.claude] — the two agents blocks merge
    let lines = vec![
        "[agents]",
        "enabled = []",
        "[shell]",
        "auto_wrap = true",
        "[agents.claude]",
        "extra_args = []",
    ];
    let headers = vec![
        (0usize, "agents".to_string()),
        (2usize, "shell".to_string()),
        (4usize, "agents".to_string()),
    ];
    let blocks = collect_section_blocks(&headers, &lines);
    // shell and agents are two unique groups
    assert_eq!(blocks.len(), 2);
    let agents_block = blocks.iter().find(|(g, _)| g == "agents").unwrap();
    assert!(agents_block.1.contains("[agents]"));
    assert!(agents_block.1.contains("[agents.claude]"));
}

// -----------------------------------------------------------------------
// sort_blocks_by_order
// -----------------------------------------------------------------------

#[test]
fn sort_blocks_by_order_canonical_order() {
    let blocks = [
        ("recording".to_string(), "r".to_string()),
        ("shell".to_string(), "s".to_string()),
        ("analysis".to_string(), "a".to_string()),
    ];
    let sorted = sort_blocks_by_order(&blocks);
    assert_eq!(sorted[0].0, "shell");
    assert_eq!(sorted[1].0, "recording");
    assert_eq!(sorted[2].0, "analysis");
}

#[test]
fn sort_blocks_by_order_unknown_appended_after_known() {
    let blocks = [
        ("my_custom".to_string(), "c".to_string()),
        ("shell".to_string(), "s".to_string()),
    ];
    let sorted = sort_blocks_by_order(&blocks);
    assert_eq!(sorted[0].0, "shell");
    assert_eq!(sorted[1].0, "my_custom");
}

#[test]
fn sort_blocks_by_order_only_unknowns_preserved_in_original_order() {
    let blocks = [
        ("zzz".to_string(), "z".to_string()),
        ("aaa".to_string(), "a".to_string()),
    ];
    let sorted = sort_blocks_by_order(&blocks);
    assert_eq!(sorted[0].0, "zzz");
    assert_eq!(sorted[1].0, "aaa");
}

#[test]
fn sort_blocks_by_order_empty_input() {
    let blocks: Vec<(String, String)> = vec![];
    let sorted = sort_blocks_by_order(&blocks);
    assert!(sorted.is_empty());
}

// -----------------------------------------------------------------------
// reassemble_toml
// -----------------------------------------------------------------------

#[test]
fn reassemble_toml_no_preamble() {
    let blocks = [
        ("shell".to_string(), "[shell]\nauto_wrap = true".to_string()),
        (
            "storage".to_string(),
            "[storage]\ndirectory = \"~/r\"".to_string(),
        ),
    ];
    let refs: Vec<&(String, String)> = blocks.iter().collect();
    let result = reassemble_toml("", &refs);
    // No leading blank line
    assert!(result.starts_with("[shell]"));
    // Blank line between blocks
    assert!(result.contains("\n\n[storage]"));
    // Trailing newline
    assert!(result.ends_with('\n'));
}

#[test]
fn reassemble_toml_with_preamble() {
    let preamble = "config_version = 2";
    let blocks = [("shell".to_string(), "[shell]\nauto_wrap = true".to_string())];
    let refs: Vec<&(String, String)> = blocks.iter().collect();
    let result = reassemble_toml(preamble, &refs);
    assert!(result.starts_with("config_version = 2"));
    assert!(result.contains("\n\n[shell]"));
    assert!(result.ends_with('\n'));
}

#[test]
fn reassemble_toml_multiple_blocks_single_blank_line_between_each() {
    let blocks = [
        ("shell".to_string(), "[shell]\nauto_wrap = true".to_string()),
        (
            "storage".to_string(),
            "[storage]\ndirectory = \"~/r\"".to_string(),
        ),
        (
            "recording".to_string(),
            "[recording]\nauto_analyze = false".to_string(),
        ),
    ];
    let refs: Vec<&(String, String)> = blocks.iter().collect();
    let result = reassemble_toml("", &refs);
    // Count occurrences of triple newline (should be zero — only double)
    assert!(!result.contains("\n\n\n"));
    assert_eq!(result.matches("\n\n").count(), 2);
}

#[test]
fn reassemble_toml_always_trailing_newline() {
    let blocks = [("shell".to_string(), "[shell]\nauto_wrap = true".to_string())];
    let refs: Vec<&(String, String)> = blocks.iter().collect();
    let result = reassemble_toml("", &refs);
    assert!(result.ends_with('\n'));
}

// -----------------------------------------------------------------------
// ensure_blank_line_separator
// -----------------------------------------------------------------------

#[test]
fn ensure_blank_line_separator_already_double_newline_unchanged() {
    let mut s = "hello\n\n".to_string();
    ensure_blank_line_separator(&mut s);
    assert_eq!(s, "hello\n\n");
}

#[test]
fn ensure_blank_line_separator_single_newline_gets_extra() {
    let mut s = "hello\n".to_string();
    ensure_blank_line_separator(&mut s);
    assert_eq!(s, "hello\n\n");
}

#[test]
fn ensure_blank_line_separator_no_newline_appends_double() {
    let mut s = "hello".to_string();
    ensure_blank_line_separator(&mut s);
    assert_eq!(s, "hello\n\n");
}

#[test]
fn ensure_blank_line_separator_empty_string_appends_double() {
    let mut s = String::new();
    ensure_blank_line_separator(&mut s);
    assert_eq!(s, "\n\n");
}

#[test]
fn ensure_blank_line_separator_triple_newline_collapses_to_double() {
    let mut s = "hello\n\n\n".to_string();
    ensure_blank_line_separator(&mut s);
    assert_eq!(s, "hello\n\n");
}

#[test]
fn ensure_blank_line_separator_many_newlines_collapses_to_double() {
    let mut s = "hello\n\n\n\n".to_string();
    ensure_blank_line_separator(&mut s);
    assert_eq!(s, "hello\n\n");
}

#[test]
fn reassemble_toml_block_with_trailing_newlines_produces_single_blank_line() {
    // If block text itself ends with extra newlines, the output must not have triple newlines
    let blocks = [
        (
            "shell".to_string(),
            "[shell]\nauto_wrap = true\n\n".to_string(),
        ),
        (
            "storage".to_string(),
            "[storage]\ndirectory = \"~/r\"".to_string(),
        ),
    ];
    let refs: Vec<&(String, String)> = blocks.iter().collect();
    let result = reassemble_toml("", &refs);
    assert!(
        !result.contains("\n\n\n"),
        "must not contain triple newline"
    );
    assert!(
        result.contains("\n\n[storage]"),
        "must have single blank line between blocks"
    );
}

// -----------------------------------------------------------------------
// parse_section_header
// -----------------------------------------------------------------------

#[test]
fn parse_section_header_valid_section() {
    assert_eq!(
        parse_section_header("[recording]"),
        Some("recording".to_string())
    );
}

#[test]
fn parse_section_header_dotted_section() {
    assert_eq!(
        parse_section_header("[agents.claude]"),
        Some("agents.claude".to_string())
    );
}

#[test]
fn parse_section_header_array_of_tables_returns_none() {
    assert_eq!(parse_section_header("[[array]]"), None);
}

#[test]
fn parse_section_header_not_a_section() {
    assert_eq!(parse_section_header("key = value"), None);
}

#[test]
fn parse_section_header_empty_brackets() {
    assert_eq!(parse_section_header("[]"), None);
}

#[test]
fn parse_section_header_with_whitespace() {
    assert_eq!(
        parse_section_header("[ recording ]"),
        Some("recording".to_string())
    );
}

#[test]
fn parse_section_header_empty_string() {
    assert_eq!(parse_section_header(""), None);
}
