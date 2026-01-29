//! Tests for filename sanitization and generation.
//!
//! These tests are written BEFORE implementation (TDD approach).

use agr::files::filename::{self, Config, FilenameError, Template, TemplateError};

// ============================================================================
// Space Replacement Tests
// ============================================================================

#[test]
fn sanitize_replaces_spaces_with_hyphens() {
    let config = Config::default();
    assert_eq!(filename::sanitize("my project", &config), "my-project");
}

#[test]
fn sanitize_replaces_multiple_spaces() {
    let config = Config::default();
    assert_eq!(filename::sanitize("my   project", &config), "my-project");
}

#[test]
fn sanitize_replaces_tabs_with_hyphens() {
    let config = Config::default();
    assert_eq!(filename::sanitize("my\tproject", &config), "my-project");
}

#[test]
fn sanitize_replaces_mixed_whitespace() {
    let config = Config::default();
    assert_eq!(filename::sanitize("my \t\n project", &config), "my-project");
}

// ============================================================================
// Invalid Character Removal Tests
// ============================================================================

#[test]
fn sanitize_removes_forward_slash() {
    let config = Config::default();
    assert_eq!(filename::sanitize("path/to/file", &config), "pathtofile");
}

#[test]
fn sanitize_removes_backslash() {
    let config = Config::default();
    assert_eq!(filename::sanitize("path\\to\\file", &config), "pathtofile");
}

#[test]
fn sanitize_removes_colon() {
    let config = Config::default();
    assert_eq!(filename::sanitize("file:name", &config), "filename");
}

#[test]
fn sanitize_removes_asterisk() {
    let config = Config::default();
    assert_eq!(filename::sanitize("file*name", &config), "filename");
}

#[test]
fn sanitize_removes_question_mark() {
    let config = Config::default();
    assert_eq!(filename::sanitize("file?name", &config), "filename");
}

#[test]
fn sanitize_removes_quotes() {
    let config = Config::default();
    assert_eq!(filename::sanitize("file\"name", &config), "filename");
}

#[test]
fn sanitize_removes_angle_brackets() {
    let config = Config::default();
    assert_eq!(filename::sanitize("file<name>", &config), "filename");
}

#[test]
fn sanitize_removes_pipe() {
    let config = Config::default();
    assert_eq!(filename::sanitize("file|name", &config), "filename");
}

#[test]
fn sanitize_removes_all_invalid_chars() {
    let config = Config::default();
    assert_eq!(
        filename::sanitize("a/b\\c:d*e?f\"g<h>i|j", &config),
        "abcdefghij"
    );
}

// ============================================================================
// Unicode Handling Tests
// ============================================================================

#[test]
fn sanitize_transliterates_accented_chars() {
    let config = Config::default();
    assert_eq!(filename::sanitize("café", &config), "cafe");
}

#[test]
fn sanitize_transliterates_umlauts() {
    let config = Config::default();
    assert_eq!(filename::sanitize("über", &config), "uber");
}

#[test]
fn sanitize_removes_non_transliteratable_unicode() {
    let config = Config::default();
    // Japanese characters that can't be transliterated to ASCII
    let result = filename::sanitize("日本語", &config);
    // Should either be empty (then fallback) or removed
    assert!(!result.contains('日'));
}

#[test]
fn sanitize_handles_emoji() {
    let config = Config::default();
    let result = filename::sanitize("project🚀name", &config);
    assert!(!result.contains('🚀'));
    // Should preserve the ASCII parts
    assert!(result.contains("project"));
    assert!(result.contains("name"));
}

#[test]
fn sanitize_handles_mixed_unicode_and_ascii() {
    let config = Config::default();
    let result = filename::sanitize("my-projeçt_v2", &config);
    assert_eq!(result, "my-project_v2");
}

// ============================================================================
// Leading/Trailing Trimming Tests
// ============================================================================

#[test]
fn sanitize_trims_leading_spaces() {
    let config = Config::default();
    assert_eq!(filename::sanitize("  project", &config), "project");
}

#[test]
fn sanitize_trims_trailing_spaces() {
    let config = Config::default();
    assert_eq!(filename::sanitize("project  ", &config), "project");
}

#[test]
fn sanitize_trims_leading_dots() {
    let config = Config::default();
    assert_eq!(filename::sanitize("..project", &config), "project");
}

#[test]
fn sanitize_trims_trailing_dots() {
    let config = Config::default();
    assert_eq!(filename::sanitize("project..", &config), "project");
}

#[test]
fn sanitize_trims_mixed_leading_chars() {
    let config = Config::default();
    assert_eq!(filename::sanitize(". . .project", &config), "project");
}

#[test]
fn sanitize_trims_leading_hyphens() {
    let config = Config::default();
    assert_eq!(filename::sanitize("---project", &config), "project");
}

#[test]
fn sanitize_trims_trailing_hyphens() {
    let config = Config::default();
    assert_eq!(filename::sanitize("project---", &config), "project");
}

// ============================================================================
// Windows Reserved Names Tests
// ============================================================================

#[test]
fn sanitize_handles_con() {
    let config = Config::default();
    assert_eq!(filename::sanitize("CON", &config), "_CON");
}

#[test]
fn sanitize_handles_prn() {
    let config = Config::default();
    assert_eq!(filename::sanitize("PRN", &config), "_PRN");
}

#[test]
fn sanitize_handles_aux() {
    let config = Config::default();
    assert_eq!(filename::sanitize("AUX", &config), "_AUX");
}

#[test]
fn sanitize_handles_nul() {
    let config = Config::default();
    assert_eq!(filename::sanitize("NUL", &config), "_NUL");
}

#[test]
fn sanitize_handles_com_ports() {
    let config = Config::default();
    assert_eq!(filename::sanitize("COM1", &config), "_COM1");
    assert_eq!(filename::sanitize("COM9", &config), "_COM9");
}

#[test]
fn sanitize_handles_lpt_ports() {
    let config = Config::default();
    assert_eq!(filename::sanitize("LPT1", &config), "_LPT1");
    assert_eq!(filename::sanitize("LPT9", &config), "_LPT9");
}

#[test]
fn sanitize_handles_reserved_names_case_insensitive() {
    let config = Config::default();
    assert_eq!(filename::sanitize("con", &config), "_con");
    assert_eq!(filename::sanitize("Con", &config), "_Con");
}

#[test]
fn sanitize_allows_reserved_names_as_substrings() {
    let config = Config::default();
    // "CONTROLLER" contains "CON" but should not be treated as reserved
    assert_eq!(filename::sanitize("CONTROLLER", &config), "CONTROLLER");
}

#[test]
fn sanitize_handles_reserved_names_with_extensions() {
    let config = Config::default();
    // Reserved names with extensions should also be prefixed
    assert_eq!(filename::sanitize("CON.txt", &config), "_CON.txt");
    assert_eq!(filename::sanitize("NUL.cast", &config), "_NUL.cast");
    assert_eq!(filename::sanitize("PRN.doc", &config), "_PRN.doc");
}

// ============================================================================
// Empty Result Fallback Tests
// ============================================================================

#[test]
fn sanitize_empty_string_returns_fallback() {
    let config = Config::default();
    assert_eq!(filename::sanitize("", &config), "recording");
}

#[test]
fn sanitize_only_spaces_returns_fallback() {
    let config = Config::default();
    assert_eq!(filename::sanitize("   ", &config), "recording");
}

#[test]
fn sanitize_only_invalid_chars_returns_fallback() {
    let config = Config::default();
    assert_eq!(filename::sanitize("/\\:*?\"<>|", &config), "recording");
}

#[test]
fn sanitize_only_dots_returns_fallback() {
    let config = Config::default();
    assert_eq!(filename::sanitize("...", &config), "recording");
}

#[test]
fn sanitize_transliterates_cjk_characters() {
    let config = Config::default();
    // CJK characters get romanized by deunicode
    let result = filename::sanitize("日本語", &config);
    // deunicode romanizes Japanese characters
    assert!(!result.contains('日'));
    assert!(!result.is_empty());
}

// ============================================================================
// Directory Truncation Tests
// ============================================================================

#[test]
fn sanitize_directory_truncates_to_max_length() {
    // Test smart abbreviation with very tight limit
    // "this-is-a-very-long-directory-name" = 34 chars, limit 10
    // After first syllable: this-is-a-very-long-dir-nam = 27 chars
    // Still too long, so proportional truncation: 1 char per word
    // Result: "t-i-a-v-l-d-n" = 13 chars
    // Final hard truncation to 10 chars: "t-i-a-v-l-"
    let config = Config {
        directory_max_length: 10,
    };
    let long_name = "this-is-a-very-long-directory-name";
    let result = filename::sanitize_directory(long_name, &config);

    // Verify hard truncation respects limit
    assert!(
        result.chars().count() <= 10,
        "Expected <= 10 chars, got {} ('{}')",
        result.chars().count(),
        result
    );
}

#[test]
fn sanitize_directory_preserves_short_names() {
    let config = Config {
        directory_max_length: 50,
    };
    let result = filename::sanitize_directory("short", &config);
    assert_eq!(result, "short");
}

#[test]
fn sanitize_directory_truncates_after_sanitization() {
    let config = Config {
        directory_max_length: 10,
    };
    // Spaces become hyphens, then truncate
    let result = filename::sanitize_directory("my long project name", &config);
    assert!(result.len() <= 10);
}

#[test]
fn sanitize_directory_default_max_is_50() {
    let config = Config::default();
    assert_eq!(config.directory_max_length, 50);
}

#[test]
fn config_new_enforces_minimum_directory_length() {
    // Config::new should enforce minimum of 1
    let config = Config::new(0);
    assert_eq!(config.directory_max_length, 1);

    let config = Config::new(5);
    assert_eq!(config.directory_max_length, 5);
}

// ============================================================================
// Final Length Validation Tests
// ============================================================================

#[test]
fn validate_length_accepts_short_filename() {
    assert!(filename::validate_length("short.cast").is_ok());
}

#[test]
fn validate_length_accepts_255_chars() {
    let name = "a".repeat(255);
    assert!(filename::validate_length(&name).is_ok());
}

#[test]
fn validate_length_rejects_256_chars() {
    let name = "a".repeat(256);
    let result = filename::validate_length(&name);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        FilenameError::TooLong {
            length: 256,
            max: 255
        }
    );
}

#[test]
fn validate_length_rejects_very_long_filename() {
    let name = "a".repeat(1000);
    let result = filename::validate_length(&name);
    assert!(result.is_err());
}

// ============================================================================
// Preservation Tests (things that should NOT change)
// ============================================================================

#[test]
fn sanitize_preserves_alphanumeric() {
    let config = Config::default();
    assert_eq!(filename::sanitize("abc123XYZ", &config), "abc123XYZ");
}

#[test]
fn sanitize_preserves_hyphens() {
    let config = Config::default();
    assert_eq!(filename::sanitize("my-project", &config), "my-project");
}

#[test]
fn sanitize_preserves_underscores() {
    let config = Config::default();
    assert_eq!(filename::sanitize("my_project", &config), "my_project");
}

#[test]
fn sanitize_preserves_dots_in_middle() {
    let config = Config::default();
    assert_eq!(filename::sanitize("file.v2.0", &config), "file.v2.0");
}

// ============================================================================
// Combined/Integration Tests
// ============================================================================

#[test]
fn sanitize_handles_realistic_directory_name() {
    let config = Config::default();
    // Realistic example: "My Project (v2)"
    assert_eq!(
        filename::sanitize("My Project (v2)", &config),
        "My-Project-v2"
    );
}

#[test]
fn sanitize_handles_path_like_input() {
    let config = Config::default();
    // User might accidentally pass a path - slashes removed, space becomes hyphen
    assert_eq!(
        filename::sanitize("/home/user/my project", &config),
        "homeusermy-project"
    );
}

#[test]
fn sanitize_collapses_multiple_hyphens() {
    let config = Config::default();
    // Multiple spaces or hyphens should collapse to single hyphen
    assert_eq!(filename::sanitize("my---project", &config), "my-project");
    assert_eq!(filename::sanitize("my   project", &config), "my-project");
}

// ============================================================================
// Template Parsing Tests
// ============================================================================

#[test]
fn template_parse_literal_only() {
    let template = Template::parse("my-recording").unwrap();
    assert_eq!(template.segments().len(), 1);
}

#[test]
fn template_parse_directory_tag() {
    let template = Template::parse("{directory}").unwrap();
    assert_eq!(template.segments().len(), 1);
}

#[test]
fn template_parse_date_tag_default_format() {
    let template = Template::parse("{date}").unwrap();
    assert_eq!(template.segments().len(), 1);
}

#[test]
fn template_parse_date_tag_custom_format() {
    let template = Template::parse("{date:%Y-%m-%d}").unwrap();
    assert_eq!(template.segments().len(), 1);
}

#[test]
fn template_parse_time_tag_default_format() {
    let template = Template::parse("{time}").unwrap();
    assert_eq!(template.segments().len(), 1);
}

#[test]
fn template_parse_time_tag_custom_format() {
    let template = Template::parse("{time:%H%M%S}").unwrap();
    assert_eq!(template.segments().len(), 1);
}

#[test]
fn template_parse_mixed_tags_and_literals() {
    // Default template: {directory}_{date:%y%m%d}_{time:%H%M}
    let template = Template::parse("{directory}_{date:%y%m%d}_{time:%H%M}").unwrap();
    // Should have: directory, literal "_", date, literal "_", time
    assert_eq!(template.segments().len(), 5);
}

#[test]
fn template_parse_literal_at_start() {
    let template = Template::parse("prefix-{directory}").unwrap();
    assert_eq!(template.segments().len(), 2);
}

#[test]
fn template_parse_literal_at_end() {
    let template = Template::parse("{directory}-suffix").unwrap();
    assert_eq!(template.segments().len(), 2);
}

#[test]
fn template_parse_empty_returns_error() {
    let result = Template::parse("");
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), TemplateError::Empty));
}

#[test]
fn template_parse_unclosed_brace_returns_error() {
    let result = Template::parse("{directory");
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), TemplateError::UnclosedBrace));
}

#[test]
fn template_parse_unknown_tag_returns_error() {
    let result = Template::parse("{unknown}");
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), TemplateError::UnknownTag(_)));
}

#[test]
fn template_parse_invalid_format_string_returns_error() {
    // Invalid strftime format (empty after colon)
    let result = Template::parse("{date:}");
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        TemplateError::InvalidFormat(_)
    ));
}

#[test]
fn template_parse_format_without_specifiers_returns_error() {
    // Format with no valid strftime specifiers
    let result = Template::parse("{date:invalid}");
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        TemplateError::InvalidFormat(_)
    ));
}

#[test]
fn template_parse_unmatched_close_brace_returns_error() {
    let result = Template::parse("test}bar");
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        TemplateError::UnmatchedCloseBrace
    ));
}

#[test]
fn template_default_constant_exists() {
    let template = Template::default();
    // Should parse the default template successfully
    assert!(!template.segments().is_empty());
}

#[test]
fn template_parse_nested_braces_returns_error() {
    let result = Template::parse("{date:{%Y}}");
    assert!(result.is_err());
}

#[test]
fn template_parse_only_literal_underscore() {
    let template = Template::parse("_").unwrap();
    assert_eq!(template.segments().len(), 1);
}

// ============================================================================
// Template Rendering Tests
// ============================================================================

#[test]
fn template_render_literal_only() {
    let template = Template::parse("my-recording").unwrap();
    let config = Config::default();
    let result = template.render("test-dir", &config);
    assert_eq!(result, "my-recording");
}

#[test]
fn template_render_directory_tag() {
    let template = Template::parse("{directory}").unwrap();
    let config = Config::default();
    let result = template.render("my-project", &config);
    assert_eq!(result, "my-project");
}

#[test]
fn template_render_directory_sanitized() {
    let template = Template::parse("{directory}").unwrap();
    let config = Config::default();
    // Directory with spaces should be sanitized
    let result = template.render("My Project", &config);
    assert_eq!(result, "My-Project");
}

#[test]
fn template_render_directory_truncated() {
    let template = Template::parse("{directory}").unwrap();
    let config = Config {
        directory_max_length: 10,
    };
    // "very-long-directory-name" = 24 chars, limit 10
    // After first syllable: "very-long-dir-nam" = 17 chars
    // Proportional truncation: 4 words, 3 separators = 3 chars
    // Available: 10-3 = 7 chars, 7/4 = 1 char per word
    // Result: "v-l-d-n" = 7 chars
    let result = template.render("very-long-directory-name", &config);
    assert_eq!(result.len(), 7);
    assert_eq!(result, "v-l-d-n");
}

#[test]
fn template_render_date_default_format() {
    let template = Template::parse("{date}").unwrap();
    let config = Config::default();
    let result = template.render("dir", &config);
    // Default format is %y%m%d (6 digits)
    assert_eq!(result.len(), 6);
    assert!(result.chars().all(|c| c.is_ascii_digit()));
}

#[test]
fn template_render_time_default_format() {
    let template = Template::parse("{time}").unwrap();
    let config = Config::default();
    let result = template.render("dir", &config);
    // Default format is %H%M (4 digits)
    assert_eq!(result.len(), 4);
    assert!(result.chars().all(|c| c.is_ascii_digit()));
}

#[test]
fn template_render_date_custom_format() {
    let template = Template::parse("{date:%Y}").unwrap();
    let config = Config::default();
    let result = template.render("dir", &config);
    // Should be 4-digit year
    assert_eq!(result.len(), 4);
    assert!(result.starts_with("20")); // 21st century
}

#[test]
fn template_render_full_default_template() {
    let template = Template::default();
    let config = Config::default();
    let result = template.render("my-project", &config);
    // Should contain directory, underscore separators, date, time
    assert!(result.contains("my-project"));
    assert!(result.contains('_'));
}

#[test]
fn template_render_preserves_literal_separators() {
    let template = Template::parse("{directory}--{date}").unwrap();
    let config = Config::default();
    let result = template.render("test", &config);
    assert!(result.contains("--"));
}

// ============================================================================
// Generate Function Tests
// ============================================================================

#[test]
fn generate_returns_filename_with_cast_extension() {
    let config = Config::default();
    let result = filename::generate("my-project", "{directory}", &config).unwrap();
    assert!(result.ends_with(".cast"));
}

#[test]
fn generate_uses_template() {
    let config = Config::default();
    let result = filename::generate("test-dir", "{directory}", &config).unwrap();
    assert_eq!(result, "test-dir.cast");
}

#[test]
fn generate_sanitizes_directory() {
    let config = Config::default();
    let result = filename::generate("My Project", "{directory}", &config).unwrap();
    assert_eq!(result, "My-Project.cast");
}

#[test]
fn generate_with_default_template() {
    let config = Config::default();
    let result = filename::generate(
        "my-project",
        "{directory}_{date:%y%m%d}_{time:%H%M}",
        &config,
    )
    .unwrap();
    assert!(result.starts_with("my-project_"));
    assert!(result.ends_with(".cast"));
}

#[test]
fn generate_validates_final_length() {
    let config = Config {
        directory_max_length: 300, // Allow long directory
    };
    // Create a template that would produce a very long filename
    let long_dir = "a".repeat(260);
    let result = filename::generate(&long_dir, "{directory}", &config);
    // Should fail because final filename > 255 chars
    assert!(result.is_err());
}

#[test]
fn generate_with_invalid_template_returns_error() {
    let config = Config::default();
    let result = filename::generate("dir", "{unknown}", &config);
    assert!(result.is_err());
}

// ============================================================================
// Smart Abbreviation Tests (via sanitize_directory)
// ============================================================================
// These tests verify the FIRST SYLLABLE HEURISTIC abbreviation strategy.
//
// ALGORITHM:
// 1. If input fits within limit, return unchanged
// 2. Split on word separators (-, _, ., whitespace)
// 3. For each word: extract first syllable
//    - Find first vowel (a, e, i, o, u)
//    - Include consonants after first vowel until next vowel or end
//    - If only one vowel in word, keep whole word
// 4. If first-syllable result still too long, truncate proportionally
// 5. Join with hyphens
//
// FIRST SYLLABLE EXAMPLES:
// - "agent" -> "ag" (a + g, stop before 'e')
// - "session" -> "ses" (s + e + s, stop before 'i')
// - "recorder" -> "rec" (r + e + c, stop before 'o')
// - "project" -> "proj" (p + r + o + j, stop before 'e')
// - "hello" -> "hel" (h + e + l, stop before 'o')
// - "world" -> "world" (only one vowel, keep all)
// - "awesome" -> "aw" (a + w, stop before 'e')
// - "testing" -> "test" (t + e + s + t, stop before 'i')
// - "example" -> "ex" (e + x, stop before 'a')
// - "cool" -> "co" (c + o, stop at second 'o' which is a vowel)
// - "my" -> "my" (no vowel or short, keep as-is)
// - "three" -> "three" (thr + ee at end, only one vowel area)
// - "one" -> "one" (short, keep as-is)
// - "two" -> "two" (short, keep as-is)
// - "four" -> "four" (f + ou + r, short word)
// - "five" -> "fiv" (f + i + v, stop before 'e')
// - "six" -> "six" (short, keep as-is)
// - "seven" -> "sev" (s + e + v, stop before 'e')
// - "eight" -> "eight" (only 'ei' vowels together, keep all)
// - "rust" -> "rust" (only one vowel, keep all)
// - "cli" -> "cli" (only one vowel, keep all)

// --- First Syllable Verification Tests ---

#[test]
fn first_syllable_agent_session_recorder_at_12() {
    // "agent" -> "ag", "session" -> "ses", "recorder" -> "rec"
    // First syllable: "ag-ses-rec" = 10 chars
    // At limit 12, should fit
    let config = Config::new(12);
    let result = filename::sanitize_directory("agent-session-recorder", &config);

    assert_eq!(result, "ag-ses-rec");
}

#[test]
fn first_syllable_agent_session_recorder_at_10() {
    // First syllable: "ag-ses-rec" = 10 chars
    // At limit 10, fits exactly!
    let config = Config::new(10);
    let result = filename::sanitize_directory("agent-session-recorder", &config);

    assert_eq!(result, "ag-ses-rec");
}

#[test]
fn first_syllable_agent_session_recorder_at_8() {
    // First syllable: "ag-ses-rec" = 10 chars, too long
    // Need to truncate further
    let config = Config::new(8);
    let result = filename::sanitize_directory("agent-session-recorder", &config);

    assert!(
        result.len() <= 8,
        "Expected <= 8, got {} ('{}')",
        result.len(),
        result
    );
    let hyphen_count = result.chars().filter(|&c| c == '-').count();
    assert_eq!(
        hyphen_count, 2,
        "Expected 2 hyphens in '{}', got {}",
        result, hyphen_count
    );
    assert!(
        !result.ends_with('-'),
        "Should not end with hyphen: '{}'",
        result
    );
}

#[test]
fn first_syllable_hello_world_at_10() {
    // "hello" -> "hel", "world" -> "world"
    // First syllable: "hel-world" = 9 chars
    // At limit 10, should fit
    let config = Config::new(10);
    let result = filename::sanitize_directory("hello-world", &config);

    assert_eq!(result, "hel-world");
}

#[test]
fn first_syllable_hello_world_at_9() {
    // First syllable: "hel-world" = 9 chars, fits exactly!
    let config = Config::new(9);
    let result = filename::sanitize_directory("hello-world", &config);

    assert_eq!(result, "hel-world");
}

#[test]
fn first_syllable_hello_world_at_7() {
    // First syllable: "hel-world" = 9 chars, too long
    // Need to truncate
    let config = Config::new(7);
    let result = filename::sanitize_directory("hello-world", &config);

    assert!(
        result.len() <= 7,
        "Expected <= 7, got {} ('{}')",
        result.len(),
        result
    );
    let hyphen_count = result.chars().filter(|&c| c == '-').count();
    assert_eq!(
        hyphen_count, 1,
        "Expected 1 hyphen in '{}', got {}",
        result, hyphen_count
    );
    assert!(
        !result.ends_with('-'),
        "Should not end with hyphen: '{}'",
        result
    );
}

#[test]
fn first_syllable_my_cool_project_at_12() {
    // "my" -> "my", "cool" -> "co" (c+o, stop at second 'o'), "project" -> "proj"
    // First syllable: "my-co-proj" = 10 chars
    // At limit 12, should fit
    let config = Config::new(12);
    let result = filename::sanitize_directory("my-cool-project", &config);

    assert_eq!(result, "my-co-proj");
}

#[test]
fn first_syllable_my_cool_project_at_10() {
    // "my" -> "my", "cool" -> "co" (c+o, stop at second 'o'), "project" -> "proj"
    // First syllable: "my-co-proj" = 10 chars, fits exactly!
    let config = Config::new(10);
    let result = filename::sanitize_directory("my-cool-project", &config);

    assert_eq!(result, "my-co-proj");
}

#[test]
fn first_syllable_my_cool_project_at_9() {
    // "my" -> "my", "cool" -> "co", "project" -> "proj"
    // First syllable: "my-co-proj" = 10 chars, too long for 9
    // Need to truncate
    let config = Config::new(9);
    let result = filename::sanitize_directory("my-cool-project", &config);

    assert!(
        result.len() <= 9,
        "Expected <= 9, got {} ('{}')",
        result.len(),
        result
    );
    let hyphen_count = result.chars().filter(|&c| c == '-').count();
    assert_eq!(
        hyphen_count, 2,
        "Expected 2 hyphens in '{}', got {}",
        result, hyphen_count
    );
}

#[test]
fn first_syllable_my_awesome_project_at_12() {
    // "my" -> "my", "awesome" -> "aw", "project" -> "proj"
    // First syllable: "my-aw-proj" = 10 chars
    // At limit 12, should fit
    let config = Config::new(12);
    let result = filename::sanitize_directory("my-awesome-project", &config);

    assert_eq!(result, "my-aw-proj");
}

#[test]
fn first_syllable_my_awesome_project_at_10() {
    // First syllable: "my-aw-proj" = 10 chars, fits exactly!
    let config = Config::new(10);
    let result = filename::sanitize_directory("my-awesome-project", &config);

    assert_eq!(result, "my-aw-proj");
}

#[test]
fn first_syllable_testing_example_at_10() {
    // "testing" -> "test", "example" -> "ex"
    // First syllable: "test-ex" = 7 chars
    // At limit 10, should fit
    let config = Config::new(10);
    let result = filename::sanitize_directory("testing-example", &config);

    assert_eq!(result, "test-ex");
}

#[test]
fn first_syllable_testing_example_at_7() {
    // First syllable: "test-ex" = 7 chars, fits exactly!
    let config = Config::new(7);
    let result = filename::sanitize_directory("testing-example", &config);

    assert_eq!(result, "test-ex");
}

#[test]
fn first_syllable_one_two_three_four_unchanged() {
    // All short words: "one", "two", "three", "four"
    // First syllable keeps short words: "one-two-three-four" = 18 chars
    // At limit 20, should be unchanged
    let config = Config::new(20);
    let result = filename::sanitize_directory("one-two-three-four", &config);

    assert_eq!(result, "one-two-three-four");
}

#[test]
fn first_syllable_one_two_three_four_at_18() {
    // Full name is 18 chars, fits exactly unchanged
    let config = Config::new(18);
    let result = filename::sanitize_directory("one-two-three-four", &config);

    assert_eq!(result, "one-two-three-four");
}

#[test]
fn first_syllable_one_two_three_four_at_15() {
    // 18 chars > 15, need abbreviation
    // But these are all short single-syllable words, so truncate proportionally
    let config = Config::new(15);
    let result = filename::sanitize_directory("one-two-three-four", &config);

    assert!(
        result.len() <= 15,
        "Expected <= 15, got {} ('{}')",
        result.len(),
        result
    );
    let hyphen_count = result.chars().filter(|&c| c == '-').count();
    assert_eq!(
        hyphen_count, 3,
        "Expected 3 hyphens in '{}', got {}",
        result, hyphen_count
    );
}

// --- Unchanged when fits tests ---

#[test]
fn unchanged_when_fits_agent_session_recorder() {
    // "agent-session-recorder" = 22 chars
    // At limit 50, should be unchanged
    let config = Config::new(50);
    let result = filename::sanitize_directory("agent-session-recorder", &config);
    assert_eq!(result, "agent-session-recorder");
}

#[test]
fn unchanged_when_fits_hello_world() {
    // "hello-world" = 11 chars
    // At limit 11 or more, should be unchanged
    let config = Config::new(11);
    let result = filename::sanitize_directory("hello-world", &config);
    assert_eq!(result, "hello-world");
}

#[test]
fn unchanged_when_fits_my_project() {
    // "my-project" = 10 chars
    let config = Config::new(10);
    let result = filename::sanitize_directory("my-project", &config);
    assert_eq!(result, "my-project");
}

// --- Single word tests (no vowel removal, just truncate) ---

#[test]
fn single_word_fits() {
    let config = Config::new(10);
    let result = filename::sanitize_directory("hello", &config);
    assert_eq!(result, "hello");
}

#[test]
fn single_word_truncated() {
    // Single word just gets hard truncated (no word structure to preserve)
    let config = Config::new(5);
    let result = filename::sanitize_directory("supercalifragilistic", &config);
    assert_eq!(result, "super");
}

// --- Edge cases ---

#[test]
fn first_syllable_very_tight_two_words() {
    // "hello-world" -> "hel-world" = 9 chars
    // At limit 5, need aggressive truncation
    let config = Config::new(5);
    let result = filename::sanitize_directory("hello-world", &config);

    assert!(
        result.len() <= 5,
        "Expected <= 5, got {} ('{}')",
        result.len(),
        result
    );
    // Should try to preserve word structure
    assert!(
        !result.ends_with('-'),
        "Should not end with hyphen: '{}'",
        result
    );
}

#[test]
fn first_syllable_three_words_unchanged() {
    // "one-two-three" = 13 chars, all short words
    // At limit 13, should fit unchanged
    let config = Config::new(13);
    let result = filename::sanitize_directory("one-two-three", &config);

    assert_eq!(result, "one-two-three");
}

#[test]
fn first_syllable_three_words_at_10() {
    // "one-two-three" = 13 chars, too long
    // Need to truncate
    let config = Config::new(10);
    let result = filename::sanitize_directory("one-two-three", &config);

    assert!(
        result.len() <= 10,
        "Expected <= 10, got {} ('{}')",
        result.len(),
        result
    );
    let hyphen_count = result.chars().filter(|&c| c == '-').count();
    assert_eq!(
        hyphen_count, 2,
        "Expected 2 hyphens in '{}', got {}",
        result, hyphen_count
    );
}

#[test]
fn first_syllable_preserves_short_words() {
    // "my" has no vowels, stays "my"
    let config = Config::new(10);
    let result = filename::sanitize_directory("my-project", &config);
    // "my-project" = 10 chars, fits exactly unchanged
    assert_eq!(result, "my-project");
}

#[test]
fn first_syllable_with_underscores() {
    // Underscores are word separators
    // "my_cool_project" -> first syllable -> "my_col_proj" or similar
    let config = Config::new(12);
    let result = filename::sanitize_directory("my_cool_project", &config);

    assert!(
        result.len() <= 12,
        "Expected <= 12, got {} ('{}')",
        result.len(),
        result
    );
}

#[test]
fn first_syllable_with_dots() {
    // Dots are word separators
    let config = Config::new(12);
    let result = filename::sanitize_directory("my.cool.project", &config);

    assert!(
        result.len() <= 12,
        "Expected <= 12, got {} ('{}')",
        result.len(),
        result
    );
}

#[test]
fn first_syllable_with_spaces() {
    // Spaces become hyphens after sanitize
    // "hello world test" -> "hello-world-test" (16 chars)
    // first syllable: "hel-world-test" = 14 chars (world and test are single-syllable)
    let config = Config::new(14);
    let result = filename::sanitize_directory("hello world test", &config);

    assert_eq!(result, "hel-world-test");
}

#[test]
fn first_syllable_mixed_separators() {
    // Mix of hyphen, underscore, dot, space
    // All short words, should stay mostly unchanged
    let config = Config::new(18);
    let result = filename::sanitize_directory("one-two_three.four", &config);

    assert!(
        result.len() <= 18,
        "Expected <= 18, got {} ('{}')",
        result.len(),
        result
    );
}

// --- Negative/Edge Cases (same as before but with smarter behavior) ---

#[test]
fn abbrev_empty_string_returns_fallback() {
    let config = Config::new(10);
    let result = filename::sanitize_directory("", &config);
    assert_eq!(result, "recording");
}

#[test]
fn abbrev_whitespace_only_returns_fallback() {
    let config = Config::new(10);
    let result = filename::sanitize_directory("   ", &config);
    assert_eq!(result, "recording");
}

#[test]
fn abbrev_single_char() {
    let config = Config::new(10);
    let result = filename::sanitize_directory("a", &config);
    assert_eq!(result, "a");
}

#[test]
fn abbrev_all_separators_returns_fallback() {
    let config = Config::new(10);
    let result = filename::sanitize_directory("---", &config);
    assert_eq!(result, "recording");
}

#[test]
fn abbrev_limit_zero_uses_minimum() {
    let config = Config::new(0);
    assert_eq!(config.directory_max_length, 1);
    let result = filename::sanitize_directory("test", &config);
    assert_eq!(result.len(), 1);
}

#[test]
fn abbrev_limit_one() {
    let config = Config::new(1);
    let result = filename::sanitize_directory("test", &config);
    assert_eq!(result.len(), 1);
    assert_eq!(result, "t");
}

#[test]
fn abbrev_leading_separators_trimmed() {
    let config = Config::new(10);
    let result = filename::sanitize_directory("-test-", &config);
    assert_eq!(result, "test");
}

#[test]
fn abbrev_multiple_consecutive_separators() {
    let config = Config::new(10);
    let result = filename::sanitize_directory("a--b", &config);
    assert_eq!(result, "a-b");
}

#[test]
fn abbrev_unicode_umlaut() {
    let config = Config::new(8);
    let result = filename::sanitize_directory("über-project", &config);
    // "über" -> "uber", then abbreviate "uber-project" to fit in 8
    assert!(result.len() <= 8);
}

#[test]
fn abbrev_with_numbers() {
    let config = Config::new(10);
    let result = filename::sanitize_directory("project-123-test", &config);
    assert!(result.len() <= 10);
}

#[test]
fn abbrev_mixed_case_preserved() {
    let config = Config::new(8);
    let result = filename::sanitize_directory("Hello-World", &config);
    assert!(result.len() <= 8);
    assert!(
        result.starts_with('H'),
        "Expected uppercase H at start of '{}'",
        result
    );
}

#[test]
fn abbrev_result_no_double_hyphens() {
    let config = Config::new(10);
    let result = filename::sanitize_directory("one-two-three", &config);
    assert!(
        !result.contains("--"),
        "Result '{}' should not contain double hyphens",
        result
    );
}

#[test]
fn abbrev_result_no_leading_hyphen() {
    let config = Config::new(10);
    let result = filename::sanitize_directory("--test-name", &config);
    assert!(
        !result.starts_with('-'),
        "Result '{}' should not start with hyphen",
        result
    );
}

#[test]
fn abbrev_result_no_trailing_hyphen() {
    let config = Config::new(10);
    let result = filename::sanitize_directory("test-name--", &config);
    assert!(
        !result.ends_with('-'),
        "Result '{}' should not end with hyphen",
        result
    );
}

#[test]
fn abbrev_windows_reserved_name() {
    let config = Config::new(10);
    let result = filename::sanitize_directory("CON", &config);
    assert_eq!(result, "_CON");
}

#[test]
fn abbrev_exact_fit_unchanged() {
    let config = Config::new(5);
    let result = filename::sanitize_directory("a-b-c", &config);
    assert_eq!(result, "a-b-c");
}

#[test]
fn abbrev_empty_words_filtered() {
    let config = Config::new(10);
    let result = filename::sanitize_directory("a---b---c", &config);
    assert_eq!(result, "a-b-c");
}

// --- First syllable specific value assertions ---

#[test]
fn syllable_specific_agent_session_recorder_exact() {
    // "agent" -> "ag", "session" -> "ses", "recorder" -> "rec"
    // First syllable: "ag-ses-rec" = 10 chars
    let config = Config::new(10);
    let result = filename::sanitize_directory("agent-session-recorder", &config);
    assert_eq!(result, "ag-ses-rec");
}

#[test]
fn syllable_specific_testing_example() {
    // "testing" -> "test", "example" -> "ex"
    // First syllable: "test-ex" = 7 chars
    let config = Config::new(7);
    let result = filename::sanitize_directory("testing-example", &config);
    assert_eq!(result, "test-ex");
}

#[test]
fn syllable_specific_testing_example_at_6() {
    // First syllable: "test-ex" = 7 chars, need to truncate
    let config = Config::new(6);
    let result = filename::sanitize_directory("testing-example", &config);

    assert!(
        result.len() <= 6,
        "Expected <= 6, got {} ('{}')",
        result.len(),
        result
    );
    let hyphen_count = result.chars().filter(|&c| c == '-').count();
    assert_eq!(
        hyphen_count, 1,
        "Expected 1 hyphen in '{}', got {}",
        result, hyphen_count
    );
}

#[test]
fn syllable_specific_one_two_three_unchanged() {
    // All short single-syllable words, stay unchanged
    // "one-two-three" = 13 chars
    let config = Config::new(13);
    let result = filename::sanitize_directory("one-two-three", &config);
    assert_eq!(result, "one-two-three");
}

#[test]
fn syllable_specific_five_words() {
    // "one" -> "one", "two" -> "two", "three" -> "three", "four" -> "four", "five" -> "fiv"
    // First syllable: "one-two-three-four-fiv" = 22 chars (five -> fiv because f+i+v before 'e')
    // Actually all these are short, mostly unchanged
    let config = Config::new(25);
    let result = filename::sanitize_directory("one-two-three-four-five", &config);
    // Should be unchanged at this limit (23 chars original)
    assert_eq!(result, "one-two-three-four-five");
}

#[test]
fn syllable_specific_five_words_at_20() {
    // "one-two-three-four-five" = 23 chars, need abbreviation
    let config = Config::new(20);
    let result = filename::sanitize_directory("one-two-three-four-five", &config);

    assert!(
        result.len() <= 20,
        "Expected <= 20, got {} ('{}')",
        result.len(),
        result
    );
    let hyphen_count = result.chars().filter(|&c| c == '-').count();
    assert_eq!(
        hyphen_count, 4,
        "Expected 4 hyphens (5 words) in '{}', got {}",
        result, hyphen_count
    );
}

// --- Tests that verify no trailing/leading hyphens ---

#[test]
fn syllable_no_trailing_hyphen_at_tight_limit() {
    // "hello-world" -> "hel-world" = 9 chars
    // At limit 6, need to truncate further
    let config = Config::new(6);
    let result = filename::sanitize_directory("hello-world", &config);

    assert!(
        !result.ends_with('-'),
        "Result '{}' should not end with hyphen",
        result
    );
    assert!(result.len() <= 6);
}

#[test]
fn syllable_never_produces_trailing_hyphen() {
    // Various limits that would produce trailing hyphens with hard truncation
    for limit in [5, 6, 7, 8, 9, 10, 11, 12] {
        let config = Config::new(limit);
        let result = filename::sanitize_directory("agent-session-recorder", &config);
        assert!(
            !result.ends_with('-'),
            "Limit {} produced '{}' which ends with hyphen",
            limit,
            result
        );
    }
}

#[test]
fn syllable_never_produces_leading_hyphen() {
    for limit in [5, 6, 7, 8, 9, 10, 11, 12] {
        let config = Config::new(limit);
        let result = filename::sanitize_directory("agent-session-recorder", &config);
        assert!(
            !result.starts_with('-'),
            "Limit {} produced '{}' which starts with hyphen",
            limit,
            result
        );
    }
}

#[test]
fn syllable_consistent_word_count_at_various_limits() {
    // Word count should stay constant (3 words = 2 hyphens) at all limits
    // "ag-ses-rec" = 10 chars
    for limit in [8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21] {
        let config = Config::new(limit);
        let result = filename::sanitize_directory("agent-session-recorder", &config);
        let hyphen_count = result.chars().filter(|&c| c == '-').count();
        assert_eq!(
            hyphen_count, 2,
            "Limit {} should preserve 3 words (2 hyphens), got '{}' with {} hyphens",
            limit, result, hyphen_count
        );
    }
}

#[test]
fn syllable_my_cool_project_structure() {
    // "my" -> "my", "cool" -> "co", "project" -> "proj"
    // First syllable: "my-co-proj" = 10 chars
    // At limit 9, need to truncate
    let config = Config::new(9);
    let result = filename::sanitize_directory("my-cool-project", &config);

    assert!(
        result.len() <= 9,
        "Expected <= 9, got {} ('{}')",
        result.len(),
        result
    );
    let hyphen_count = result.chars().filter(|&c| c == '-').count();
    assert_eq!(
        hyphen_count, 2,
        "Expected 2 hyphens in '{}', got {}",
        result, hyphen_count
    );

    // Each word should have at least some chars
    let parts: Vec<&str> = result.split('-').collect();
    assert_eq!(parts.len(), 3);
    for (i, part) in parts.iter().enumerate() {
        assert!(
            !part.is_empty(),
            "Word {} should not be empty in '{}'",
            i,
            result
        );
    }
}

#[test]
fn syllable_realistic_long_directory() {
    // "my" -> "my", "really" -> "real" (r+e+a+l, stop before second vowel? wait...)
    // Actually "really": r-e-a-l-l-y. First vowel is 'e', next vowel is 'a'.
    // So: r + e + (nothing before 'a') = "re"? No wait, we include consonants AFTER first vowel.
    // "really": first vowel 'e' at index 1, next vowel 'a' at index 2. So just "re".
    // Hmm, that seems too short. Let me re-read the algorithm.
    // "Find first vowel, then include consonants until next vowel"
    // "really" = r(cons) + e(vowel) + a(vowel) - stop! Result: "re"
    // "awesome" = a(vowel) + w(cons) + e(vowel) - stop! Result: "aw"
    // "cool" = c(cons) + o(vowel) + o(vowel) - stop! Result: "co"
    // "project" = p(cons) + r(cons) + o(vowel) + j(cons) + e(vowel) - stop! Result: "proj"
    // First syllable: "my-re-aw-co-proj" = 16 chars
    let config = Config::new(20);
    let result = filename::sanitize_directory("my-really-awesome-cool-project", &config);

    assert_eq!(result, "my-re-aw-co-proj");
}

#[test]
fn syllable_realistic_long_directory_at_16() {
    // First syllable: "my-re-aw-co-proj" = 16 chars, fits exactly
    let config = Config::new(16);
    let result = filename::sanitize_directory("my-really-awesome-cool-project", &config);

    assert_eq!(result, "my-re-aw-co-proj");
}

#[test]
fn syllable_realistic_long_directory_at_14() {
    // First syllable: "my-re-aw-co-proj" = 16 chars, need to truncate
    let config = Config::new(14);
    let result = filename::sanitize_directory("my-really-awesome-cool-project", &config);

    assert!(
        result.len() <= 14,
        "Expected <= 14, got {} ('{}')",
        result.len(),
        result
    );
    let hyphen_count = result.chars().filter(|&c| c == '-').count();
    assert_eq!(
        hyphen_count, 4,
        "Expected 4 hyphens (5 words) in '{}', got {}",
        result, hyphen_count
    );
}

// ============================================================================
// 1 Word Cases - Single words just get hard truncated
// ============================================================================

#[test]
fn one_word_project_fits_at_10() {
    let config = Config::new(10);
    let result = filename::sanitize_directory("project", &config);
    assert_eq!(result, "project");
}

#[test]
fn one_word_project_truncated_at_4() {
    let config = Config::new(4);
    let result = filename::sanitize_directory("project", &config);
    assert_eq!(result, "proj");
}

#[test]
fn one_word_superlongprojectname_at_8() {
    // Single word, just hard truncate
    let config = Config::new(8);
    let result = filename::sanitize_directory("superlongprojectname", &config);
    assert_eq!(result, "superlon");
}

#[test]
fn one_word_single_char_a_at_5() {
    let config = Config::new(5);
    let result = filename::sanitize_directory("a", &config);
    assert_eq!(result, "a");
}

// ============================================================================
// 4 Word Cases
// ============================================================================

#[test]
fn four_words_my_cool_rust_project_at_20() {
    // Full name is 20 chars, should fit exactly unchanged
    let config = Config::new(20);
    let result = filename::sanitize_directory("my-cool-rust-project", &config);
    assert_eq!(result, "my-cool-rust-project");
}

#[test]
fn four_words_my_cool_rust_project_at_17() {
    // First syllable: "my" -> "my", "cool" -> "co", "rust" -> "rust", "project" -> "proj"
    // First syllable: "my-co-rust-proj" = 15 chars, fits at 17
    let config = Config::new(17);
    let result = filename::sanitize_directory("my-cool-rust-project", &config);
    assert_eq!(result, "my-co-rust-proj");
}

#[test]
fn four_words_my_cool_rust_project_at_15() {
    // First syllable: "my" -> "my", "cool" -> "co", "rust" -> "rust", "project" -> "proj"
    // First syllable: "my-co-rust-proj" = 15 chars, fits exactly!
    let config = Config::new(15);
    let result = filename::sanitize_directory("my-cool-rust-project", &config);
    assert_eq!(result, "my-co-rust-proj");
}

#[test]
fn four_words_my_cool_rust_project_at_14() {
    // First syllable: "my-co-rust-proj" = 15 chars, need slight truncation
    let config = Config::new(14);
    let result = filename::sanitize_directory("my-cool-rust-project", &config);

    assert!(
        result.len() <= 14,
        "Expected <= 14, got {} ('{}')",
        result.len(),
        result
    );
    let hyphen_count = result.chars().filter(|&c| c == '-').count();
    assert_eq!(
        hyphen_count, 3,
        "Expected 3 hyphens (4 words) in '{}', got {}",
        result, hyphen_count
    );
    assert!(
        !result.ends_with('-'),
        "Should not end with hyphen: '{}'",
        result
    );
}

#[test]
fn four_words_my_cool_rust_project_at_10() {
    // First syllable: "my-co-rust-proj" = 15 chars, need aggressive truncation
    // 4 words, 3 separators = 3 chars for separators, 7 chars for words
    // 7 / 4 = 1 char per word on average
    let config = Config::new(10);
    let result = filename::sanitize_directory("my-cool-rust-project", &config);

    assert!(
        result.len() <= 10,
        "Expected <= 10, got {} ('{}')",
        result.len(),
        result
    );
    let hyphen_count = result.chars().filter(|&c| c == '-').count();
    assert_eq!(
        hyphen_count, 3,
        "Expected 3 hyphens (4 words) in '{}', got {}",
        result, hyphen_count
    );
    assert!(
        !result.ends_with('-'),
        "Should not end with hyphen: '{}'",
        result
    );
}

#[test]
fn four_words_one_two_three_four_at_19() {
    // First syllable: "one" -> "one", "two" -> "two", "three" -> "three", "four" -> "four"
    // (all short words or single-vowel words, keep unchanged)
    // Full form is "one-two-three-four" = 18 chars, fits at 19
    let config = Config::new(19);
    let result = filename::sanitize_directory("one-two-three-four", &config);
    assert_eq!(result, "one-two-three-four");
}

#[test]
fn four_words_one_two_three_four_at_14() {
    // First syllable: all short/single-vowel words, can't abbreviate further
    // "one-two-three-four" = 18 chars, need proportional truncation
    let config = Config::new(14);
    let result = filename::sanitize_directory("one-two-three-four", &config);

    assert!(
        result.len() <= 14,
        "Expected <= 14, got {} ('{}')",
        result.len(),
        result
    );
    let hyphen_count = result.chars().filter(|&c| c == '-').count();
    assert_eq!(
        hyphen_count, 3,
        "Expected 3 hyphens (4 words) in '{}', got {}",
        result, hyphen_count
    );
    assert!(
        !result.ends_with('-'),
        "Should not end with hyphen: '{}'",
        result
    );
}

#[test]
fn four_words_one_two_three_four_at_10() {
    // First syllable: all short/single-vowel words, can't abbreviate further
    // Very tight: 4 words in 10 chars needs proportional truncation
    let config = Config::new(10);
    let result = filename::sanitize_directory("one-two-three-four", &config);

    assert!(
        result.len() <= 10,
        "Expected <= 10, got {} ('{}')",
        result.len(),
        result
    );
    let hyphen_count = result.chars().filter(|&c| c == '-').count();
    assert_eq!(
        hyphen_count, 3,
        "Expected 3 hyphens (4 words) in '{}', got {}",
        result, hyphen_count
    );
    assert!(
        !result.ends_with('-'),
        "Should not end with hyphen: '{}'",
        result
    );
}

// ============================================================================
// 5 Word Cases
// ============================================================================

#[test]
fn five_words_agent_session_recorder_for_cli_at_25() {
    // First syllable: "agent" -> "ag", "session" -> "ses", "recorder" -> "rec", "for" -> "for", "cli" -> "cli"
    // First syllable: "ag-ses-rec-for-cli" = 18 chars, fits at 25
    let config = Config::new(25);
    let result = filename::sanitize_directory("agent-session-recorder-for-cli", &config);
    assert_eq!(result, "ag-ses-rec-for-cli");
}

#[test]
fn five_words_agent_session_recorder_for_cli_at_18() {
    // First syllable: "ag-ses-rec-for-cli" = 18 chars, fits exactly!
    let config = Config::new(18);
    let result = filename::sanitize_directory("agent-session-recorder-for-cli", &config);
    assert_eq!(result, "ag-ses-rec-for-cli");
}

#[test]
fn five_words_agent_session_recorder_for_cli_at_20() {
    // First syllable: "ag-ses-rec-for-cli" = 18 chars, fits at 20
    let config = Config::new(20);
    let result = filename::sanitize_directory("agent-session-recorder-for-cli", &config);
    assert_eq!(result, "ag-ses-rec-for-cli");
}

#[test]
fn five_words_agent_session_recorder_for_cli_at_15() {
    // First syllable: "ag-ses-rec-for-cli" = 18 chars, need truncation
    let config = Config::new(15);
    let result = filename::sanitize_directory("agent-session-recorder-for-cli", &config);

    assert!(
        result.len() <= 15,
        "Expected <= 15, got {} ('{}')",
        result.len(),
        result
    );
    let hyphen_count = result.chars().filter(|&c| c == '-').count();
    assert_eq!(
        hyphen_count, 4,
        "Expected 4 hyphens (5 words) in '{}', got {}",
        result, hyphen_count
    );
    assert!(
        !result.ends_with('-'),
        "Should not end with hyphen: '{}'",
        result
    );
}

#[test]
fn five_words_this_is_a_long_name() {
    // First syllable: "this" -> "this" (only one vowel), "is" -> "is", "a" -> "a",
    // "long" -> "long" (only one vowel), "name" -> "nam" (n+a+m, stop at 'e')
    // First syllable: "this-is-a-long-nam" = 18 chars
    let config = Config::new(18);
    let result = filename::sanitize_directory("this-is-a-long-name", &config);
    assert_eq!(result, "this-is-a-long-nam");
}

#[test]
fn five_words_this_is_a_long_name_at_15() {
    // First syllable: "this-is-a-long-nam" = 18 chars, need truncation
    let config = Config::new(15);
    let result = filename::sanitize_directory("this-is-a-long-name", &config);

    assert!(
        result.len() <= 15,
        "Expected <= 15, got {} ('{}')",
        result.len(),
        result
    );
    let hyphen_count = result.chars().filter(|&c| c == '-').count();
    assert_eq!(
        hyphen_count, 4,
        "Expected 4 hyphens (5 words) in '{}', got {}",
        result, hyphen_count
    );
    assert!(
        !result.ends_with('-'),
        "Should not end with hyphen: '{}'",
        result
    );
}

#[test]
fn five_words_single_char_words_a_b_c_d_e_at_9() {
    // 5 single-char words: "a", "b", "c", "d", "e"
    // Full form is "a-b-c-d-e" = 9 chars
    // Single consonants can't have vowels removed, stays unchanged
    let config = Config::new(9);
    let result = filename::sanitize_directory("a-b-c-d-e", &config);
    assert_eq!(result, "a-b-c-d-e");
}

#[test]
fn five_words_single_char_words_a_b_c_d_e_at_7() {
    // 5 single-char words at tight limit
    // "a-b-c-d-e" = 9 chars, limit 7
    // After proportional truncation (1 char per word), still 9 chars
    // Final hard truncation kicks in: "a-b-c-d-e" -> "a-b-c-d" (7 chars)
    let config = Config::new(7);
    let result = filename::sanitize_directory("a-b-c-d-e", &config);

    // Verify hard truncation respects limit
    assert!(
        result.chars().count() <= 7,
        "Expected <= 7 chars, got {} ('{}')",
        result.chars().count(),
        result
    );
}

// ============================================================================
// 6+ Word Cases
// ============================================================================

#[test]
fn six_words_one_two_three_four_five_six_at_27() {
    // Original "one-two-three-four-five-six" = 27 chars, fits exactly at 27
    // No abbreviation needed when it fits
    let config = Config::new(27);
    let result = filename::sanitize_directory("one-two-three-four-five-six", &config);
    assert_eq!(result, "one-two-three-four-five-six");
}

#[test]
fn six_words_one_two_three_four_five_six_at_26() {
    // Original "one-two-three-four-five-six" = 27 chars, need to abbreviate to fit 26
    // First syllable: "three" -> "thre", "four" -> "fo", "five" -> "fiv"
    // Result: "one-two-thre-fo-fiv-six" = 23 chars, fits at 26
    let config = Config::new(26);
    let result = filename::sanitize_directory("one-two-three-four-five-six", &config);
    assert_eq!(result, "one-two-thre-fo-fiv-six");
}

#[test]
fn six_words_one_two_three_four_five_six_at_20() {
    // First syllable: "one-two-three-four-fiv-six" = 26 chars, need truncation
    let config = Config::new(20);
    let result = filename::sanitize_directory("one-two-three-four-five-six", &config);

    assert!(
        result.len() <= 20,
        "Expected <= 20, got {} ('{}')",
        result.len(),
        result
    );
    let hyphen_count = result.chars().filter(|&c| c == '-').count();
    assert_eq!(
        hyphen_count, 5,
        "Expected 5 hyphens (6 words) in '{}', got {}",
        result, hyphen_count
    );
    assert!(
        !result.ends_with('-'),
        "Should not end with hyphen: '{}'",
        result
    );
}

#[test]
fn seven_words_single_char_a_b_c_d_e_f_g_at_13() {
    // 7 single-char words: "a-b-c-d-e-f-g" = 13 chars
    // Single chars can't have vowels removed, stays unchanged
    let config = Config::new(13);
    let result = filename::sanitize_directory("a-b-c-d-e-f-g", &config);
    assert_eq!(result, "a-b-c-d-e-f-g");
}

#[test]
fn seven_words_single_char_a_b_c_d_e_f_g_at_10() {
    // 7 single-char words at tight limit
    // "a-b-c-d-e-f-g" = 13 chars, limit 10
    // After proportional truncation (1 char per word), still 13 chars
    // Final hard truncation kicks in: "a-b-c-d-e-f-g" -> "a-b-c-d-e-" (10 chars)
    let config = Config::new(10);
    let result = filename::sanitize_directory("a-b-c-d-e-f-g", &config);

    // Verify hard truncation respects limit
    assert!(
        result.chars().count() <= 10,
        "Expected <= 10 chars, got {} ('{}')",
        result.chars().count(),
        result
    );
}

#[test]
fn eight_words_first_syllable() {
    // Original "one-two-three-four-five-six-seven-eight" = 39 chars
    // First syllable abbreviation:
    // "one" -> "one", "two" -> "two", "three" -> "thre", "four" -> "fo",
    // "five" -> "fiv", "six" -> "six", "seven" -> "sev", "eight" -> "e"
    // Abbreviated: "one-two-thre-fo-fiv-six-sev-e" = 29 chars
    // At limit 36, abbreviated result fits
    let config = Config::new(36);
    let result = filename::sanitize_directory("one-two-three-four-five-six-seven-eight", &config);
    assert_eq!(result, "one-two-thre-fo-fiv-six-sev-e");
}

#[test]
fn eight_words_at_30() {
    // First syllable: "one-two-three-four-fiv-six-sev-eight" = 36 chars, need truncation
    let config = Config::new(30);
    let result = filename::sanitize_directory("one-two-three-four-five-six-seven-eight", &config);

    assert!(
        result.len() <= 30,
        "Expected <= 30, got {} ('{}')",
        result.len(),
        result
    );
    let hyphen_count = result.chars().filter(|&c| c == '-').count();
    assert_eq!(
        hyphen_count, 7,
        "Expected 7 hyphens (8 words) in '{}', got {}",
        result, hyphen_count
    );
    assert!(
        !result.ends_with('-'),
        "Should not end with hyphen: '{}'",
        result
    );
}

#[test]
fn ten_words_at_30() {
    // 10 single-char words: "a-b-c-d-e-f-g-h-i-j" = 19 chars
    // Should fit unchanged
    let config = Config::new(30);
    let result = filename::sanitize_directory("a-b-c-d-e-f-g-h-i-j", &config);
    assert_eq!(result, "a-b-c-d-e-f-g-h-i-j");
}

#[test]
fn ten_words_at_19() {
    // "a-b-c-d-e-f-g-h-i-j" = 19 chars, fits exactly
    let config = Config::new(19);
    let result = filename::sanitize_directory("a-b-c-d-e-f-g-h-i-j", &config);
    assert_eq!(result, "a-b-c-d-e-f-g-h-i-j");
}
