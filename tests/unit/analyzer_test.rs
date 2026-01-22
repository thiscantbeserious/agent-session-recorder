//! Unit tests for analyzer module

use agr::Analyzer;
use std::path::PathBuf;

#[test]
fn analyzer_new_sets_agent() {
    let analyzer = Analyzer::new("claude");
    assert_eq!(analyzer.agent, "claude");
}

#[test]
fn build_prompt_includes_filepath() {
    let analyzer = Analyzer::new("claude");
    let filepath = PathBuf::from("/home/user/sessions/test.cast");
    let prompt = analyzer.build_prompt(&filepath);

    assert!(prompt.contains("/home/user/sessions/test.cast"));
    assert!(prompt.contains("agr marker add"));
}

#[test]
fn build_prompt_replaces_all_placeholders() {
    let analyzer = Analyzer::new("claude");
    let filepath = PathBuf::from("/tmp/session.cast");
    let prompt = analyzer.build_prompt(&filepath);

    // Should not contain any unreplaced placeholders
    assert!(!prompt.contains("{filepath}"));
    // Count occurrences of the path - should be multiple
    let count = prompt.matches("/tmp/session.cast").count();
    assert!(count >= 3, "Expected at least 3 occurrences of filepath");
}

#[test]
fn is_agent_installed_returns_false_for_missing() {
    // Test with a binary that definitely doesn't exist
    let result = Analyzer::is_agent_installed("definitely-not-a-real-binary-12345");
    assert!(!result);
}

#[test]
fn gemini_agent_name_is_preserved() {
    // Verify the analyzer stores the agent name as provided
    let analyzer = Analyzer::new("gemini");
    assert_eq!(analyzer.agent, "gemini");
}

#[test]
fn build_prompt_contains_constraints() {
    let analyzer = Analyzer::new("claude");
    let filepath = PathBuf::from("/tmp/session.cast");
    let prompt = analyzer.build_prompt(&filepath);

    // Verify prompt includes marker limit constraint
    assert!(
        prompt.contains("Maximum 5-7 markers"),
        "Prompt should contain marker limit"
    );
    assert!(
        prompt.contains("CONSTRAINTS"),
        "Prompt should have CONSTRAINTS section"
    );
}

#[test]
fn build_prompt_contains_negative_examples() {
    let analyzer = Analyzer::new("claude");
    let filepath = PathBuf::from("/tmp/session.cast");
    let prompt = analyzer.build_prompt(&filepath);

    // Verify prompt includes "DO NOT MARK" section
    assert!(
        prompt.contains("DO NOT MARK"),
        "Prompt should contain DO NOT MARK section"
    );
    assert!(
        prompt.contains("Directory listings"),
        "Prompt should mention directory listings"
    );
    assert!(
        prompt.contains("Help text"),
        "Prompt should mention help text"
    );
}

#[test]
fn build_prompt_contains_priority_categories() {
    let analyzer = Analyzer::new("claude");
    let filepath = PathBuf::from("/tmp/session.cast");
    let prompt = analyzer.build_prompt(&filepath);

    // Verify priority categories are present
    assert!(prompt.contains("ERRORS"), "Prompt should mention ERRORS");
    assert!(
        prompt.contains("MILESTONES"),
        "Prompt should mention MILESTONES"
    );
    assert!(
        prompt.contains("KEY DECISIONS"),
        "Prompt should mention KEY DECISIONS"
    );
}

#[test]
fn build_prompt_contains_example_markers() {
    let analyzer = Analyzer::new("claude");
    let filepath = PathBuf::from("/tmp/session.cast");
    let prompt = analyzer.build_prompt(&filepath);

    // Verify example markers include error and milestone prefixes
    assert!(
        prompt.contains("ERROR:"),
        "Prompt should have ERROR: example"
    );
    assert!(
        prompt.contains("MILESTONE:"),
        "Prompt should have MILESTONE: example"
    );
}

#[test]
fn analyze_returns_error_for_missing_agent() {
    let analyzer = Analyzer::new("definitely-not-installed-agent-xyz");
    let filepath = PathBuf::from("/tmp/session.cast");

    let result = analyzer.analyze(&filepath);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not installed"),
        "Error should mention agent not installed"
    );
}
