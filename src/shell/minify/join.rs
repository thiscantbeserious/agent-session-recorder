//! Phase 2: Join statements with semicolons
//!
//! Combines lines into a single-line script where possible,
//! respecting shell syntax rules for control structures.

use super::comments::detect_heredoc_start;

/// Keywords that don't need a semicolon after them.
const KEYWORDS_NO_SEMICOLON_AFTER: &[&str] = &["then", "do", "else", "in", "{"];

/// Keywords that don't need a semicolon before them.
const KEYWORDS_NO_SEMICOLON_BEFORE: &[&str] = &["then", "do", "done", "fi", "esac", "}", ";;"];

/// Join processed lines into a single string with appropriate separators.
///
/// Uses semicolons between statements, spaces around control keywords,
/// and newlines only where required (heredocs, shebang).
pub fn join_statements(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }

    let mut result = String::new();
    let mut heredoc_state = HeredocState::None;

    for (i, line) in lines.iter().enumerate() {
        let separator = if i == 0 {
            String::new()
        } else {
            compute_separator(lines, i, &mut heredoc_state)
        };

        result.push_str(&separator);

        // Preserve heredoc content verbatim, trim other lines
        if matches!(heredoc_state, HeredocState::Inside { .. }) {
            result.push_str(line);
        } else {
            result.push_str(line.trim());
        }

        // Track heredoc state for future lines
        update_heredoc_state(line, &mut heredoc_state);
    }

    post_process(&result)
}

// ============================================================================
// Heredoc tracking
// ============================================================================

/// Tracks whether we're inside a heredoc block.
enum HeredocState {
    None,
    Inside { delimiter: String },
}

/// Update heredoc state after processing a line.
fn update_heredoc_state(line: &str, state: &mut HeredocState) {
    match state {
        HeredocState::None => {
            if let Some(delim) = detect_heredoc_start(line) {
                *state = HeredocState::Inside { delimiter: delim };
            }
        }
        HeredocState::Inside { delimiter } => {
            if line.trim() == delimiter {
                *state = HeredocState::None;
            }
        }
    }
}

// ============================================================================
// Separator computation
// ============================================================================

/// Compute the separator to use before the current line.
fn compute_separator(lines: &[String], index: usize, heredoc_state: &mut HeredocState) -> String {
    let prev_line = lines.get(index - 1).map(|s| s.trim()).unwrap_or("");
    let curr_line = lines[index].trim();

    // Check if entering/inside heredoc
    if let Some(delim) = detect_heredoc_start(prev_line) {
        *heredoc_state = HeredocState::Inside { delimiter: delim };
        return "\n".to_string();
    }

    if matches!(heredoc_state, HeredocState::Inside { .. }) {
        return "\n".to_string();
    }

    determine_separator(prev_line, curr_line)
}

/// Determine the separator based on previous and current line content.
fn determine_separator(prev_line: &str, curr_line: &str) -> String {
    // Shebang always needs newline after
    if prev_line.starts_with("#!") {
        return "\n".to_string();
    }

    // Control keyword at end of prev line - use space
    if ends_with_control_keyword(prev_line) {
        return " ".to_string();
    }

    // Control keyword at start of curr line
    if starts_with_control_keyword(curr_line) {
        // then/do need semicolon before (if condition;then)
        if curr_line.starts_with("then") || curr_line.starts_with("do") {
            return ";".to_string();
        }
        return " ".to_string();
    }

    // Case patterns use space
    if is_case_pattern(curr_line) {
        return " ".to_string();
    }

    // If prev line already ends with separator, no need for another
    let last_char = prev_line.chars().last().unwrap_or(' ');
    if last_char == ';' || last_char == '{' {
        return String::new();
    }

    ";".to_string()
}

// ============================================================================
// Keyword detection
// ============================================================================

/// Check if line ends with a control keyword that doesn't need semicolon after.
fn ends_with_control_keyword(line: &str) -> bool {
    let trimmed = line.trim();
    KEYWORDS_NO_SEMICOLON_AFTER.iter().any(|kw| {
        trimmed
            .strip_suffix(kw)
            .map(|before| {
                before.is_empty() || before.ends_with(char::is_whitespace) || before.ends_with(';')
            })
            .unwrap_or(false)
    })
}

/// Check if line starts with a control keyword that doesn't need semicolon before.
fn starts_with_control_keyword(line: &str) -> bool {
    let trimmed = line.trim();
    KEYWORDS_NO_SEMICOLON_BEFORE.iter().any(|kw| {
        trimmed
            .strip_prefix(kw)
            .map(|after| {
                after.is_empty()
                    || after.starts_with(char::is_whitespace)
                    || after.starts_with(';')
                    || after.starts_with(')')
            })
            .unwrap_or(false)
    })
}

/// Detect case statement patterns like `pattern)` or `*)`
fn is_case_pattern(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.ends_with(')') {
        return false;
    }
    // Case pattern has more closing parens than opening
    let open = trimmed.matches('(').count();
    let close = trimmed.matches(')').count();
    close > open
}

// ============================================================================
// Post-processing
// ============================================================================

/// Fix up edge cases in the joined output.
fn post_process(input: &str) -> String {
    let mut result = input.to_string();

    result = fix_case_endings(&result);
    result = fix_function_braces(&result);
    result = fix_control_keyword_spacing(&result);
    result = fix_semicolons_before_control(&result);
    result = fix_semicolons_before_fi(&result);
    result = fix_semicolons_after_esac(&result);

    // Remove trailing semicolon
    result.trim_end_matches(';').to_string()
}

/// Fix case statement endings.
fn fix_case_endings(input: &str) -> String {
    input
        .replace(";; esac", "; esac")
        .replace(";;esac", ";esac")
        .replace("  esac", " esac")
}

/// Fix function brace formatting.
fn fix_function_braces(input: &str) -> String {
    input
        .replace("; }", ";}")
        .replace(" }", ";}")
        .replace("{ ;", "{ ")
        .replace("{;", "{ ")
}

/// Fix control keyword spacing.
fn fix_control_keyword_spacing(input: &str) -> String {
    input
        .replace(";then;", ";then ")
        .replace(";do;", ";do ")
        .replace("in;", "in ")
}

/// Add semicolons before elif/else/case where needed.
fn fix_semicolons_before_control(input: &str) -> String {
    let mut result = input.to_string();
    let control_keywords = [" elif ", " else ", " case "];
    let no_semicolon_after = ["then", "do", "else", "{", "\n", ";"];

    for kw in control_keywords {
        result = insert_semicolons_before(&result, kw, &no_semicolon_after);
    }
    result
}

/// Insert semicolons before a keyword where needed.
fn insert_semicolons_before(input: &str, keyword: &str, skip_after: &[&str]) -> String {
    let mut result = input.to_string();
    let mut i = 0;

    while let Some(pos) = result[i..].find(keyword) {
        let abs_pos = i + pos;
        let before = &result[..abs_pos];

        if skip_after.iter().any(|s| before.ends_with(s)) {
            i = abs_pos + keyword.len();
            continue;
        }

        if needs_semicolon_before(before) {
            let kw_trimmed = keyword.trim();
            result = format!(
                "{};{} {}",
                &result[..abs_pos],
                kw_trimmed,
                &result[abs_pos + keyword.len()..]
            );
            i = abs_pos + 1 + kw_trimmed.len();
        } else {
            i = abs_pos + keyword.len();
        }
    }
    result
}

/// Check if the text before a keyword needs a semicolon.
fn needs_semicolon_before(before: &str) -> bool {
    let prev_char = before.chars().last().unwrap_or(' ');
    let needs =
        prev_char.is_alphanumeric() || matches!(prev_char, '"' | '\'' | ')' | ']' | '}' | '`');

    // Check for block-end keywords with proper word boundary
    // This prevents matching variable names like $wifi or commands like unifi
    let is_after_block_end = is_word_ending(before, "fi")
        || is_word_ending(before, "done")
        || is_word_ending(before, "esac");

    needs && !is_after_block_end
}

/// Check if text ends with a word (not part of a larger identifier).
fn is_word_ending(text: &str, word: &str) -> bool {
    text.strip_suffix(word)
        .map(|prefix| prefix.is_empty() || !prefix.chars().last().unwrap().is_alphanumeric())
        .unwrap_or(false)
}

/// Add semicolons before `fi` where needed.
fn fix_semicolons_before_fi(input: &str) -> String {
    let mut result = input.to_string();
    let no_semicolon_after = ["then", "do", "else", "{", ";", "\n"];

    let mut i = 0;
    while let Some(pos) = result[i..].find(" fi") {
        let abs_pos = i + pos;

        // Check word boundary after "fi"
        let after_fi = &result[abs_pos + 3..];
        let is_word_boundary = after_fi.is_empty() || after_fi.starts_with([' ', ';', '\n', '}']);

        if !is_word_boundary {
            i = abs_pos + 3;
            continue;
        }

        let before = &result[..abs_pos];
        if no_semicolon_after.iter().any(|s| before.ends_with(s)) {
            i = abs_pos + 3;
            continue;
        }

        let prev_char = before.chars().last().unwrap_or(' ');
        let needs =
            prev_char.is_alphanumeric() || matches!(prev_char, '"' | '\'' | ')' | ']' | '}' | '`');

        if needs {
            result = format!("{};fi{}", &result[..abs_pos], &result[abs_pos + 3..]);
        }
        i = abs_pos + 3;
    }
    result
}

/// Add semicolons after `esac` when followed by elif/else/fi.
fn fix_semicolons_after_esac(input: &str) -> String {
    input
        .replace("esac elif", "esac;elif")
        .replace("esac else", "esac;else")
        .replace("esac fi", "esac;fi")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_simple() {
        let lines = vec!["echo a".to_string(), "echo b".to_string()];
        assert_eq!(join_statements(&lines), "echo a;echo b");
    }

    #[test]
    fn test_join_empty() {
        let lines: Vec<String> = vec![];
        assert_eq!(join_statements(&lines), "");
    }

    #[test]
    fn test_shebang_gets_newline() {
        let lines = vec!["#!/bin/bash".to_string(), "echo hi".to_string()];
        assert_eq!(join_statements(&lines), "#!/bin/bash\necho hi");
    }

    #[test]
    fn test_if_then_formatting() {
        let lines = vec![
            "if [ -f file ]".to_string(),
            "then".to_string(),
            "echo found".to_string(),
            "fi".to_string(),
        ];
        let result = join_statements(&lines);
        assert!(result.contains(";then"));
        assert!(result.contains(";fi") || result.contains(" fi"));
    }

    #[test]
    fn test_is_case_pattern() {
        assert!(is_case_pattern("*)"));
        assert!(is_case_pattern("foo)"));
        assert!(!is_case_pattern("(foo)"));
        assert!(!is_case_pattern("echo hello"));
    }

    #[test]
    fn test_is_word_ending() {
        // Should match standalone keywords
        assert!(is_word_ending("fi", "fi"));
        assert!(is_word_ending("echo; fi", "fi"));
        assert!(is_word_ending("then fi", "fi"));

        // Should NOT match keywords embedded in identifiers
        assert!(!is_word_ending("wifi", "fi"));
        assert!(!is_word_ending("$wifi", "fi"));
        assert!(!is_word_ending("unifi", "fi"));

        // Same for done/esac
        assert!(is_word_ending("done", "done"));
        assert!(!is_word_ending("undone", "done"));
        assert!(is_word_ending("esac", "esac"));
        assert!(!is_word_ending("esesac", "esac"));
    }

    #[test]
    fn test_variable_names_not_treated_as_keywords() {
        // Variables ending in 'fi' should not prevent semicolon insertion
        let lines = vec![
            "wifi=test".to_string(),
            "elif true".to_string(),
            "then echo ok".to_string(),
            "fi".to_string(),
        ];
        let result = join_statements(&lines);
        // wifi=test should get a semicolon before elif (not be treated as 'fi')
        assert!(
            result.contains("wifi=test;elif") || result.contains("wifi=test; elif"),
            "Should insert semicolon after wifi=test, got: {}",
            result
        );
    }
}
