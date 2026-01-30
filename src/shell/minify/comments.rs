//! Phase 1: Remove comments and blank lines
//!
//! Strips non-functional content while preserving:
//! - Shebang lines (#!/bin/bash)
//! - Heredoc content (verbatim)
//! - Indentation is removed

/// Remove comments and blank lines from a shell script.
///
/// Returns processed lines with:
/// - Comments removed (except shebang)
/// - Blank lines removed
/// - Leading indentation stripped
/// - Heredoc content preserved verbatim
pub fn remove_comments_and_blanks(script: &str) -> Vec<String> {
    let lines: Vec<&str> = script.lines().collect();
    let mut processed: Vec<String> = Vec::new();
    let mut heredoc_state = HeredocState::None;

    for line in &lines {
        match process_line(line, &mut heredoc_state) {
            LineAction::Include(s) => processed.push(s),
            LineAction::Skip => continue,
        }
    }

    processed
}

/// Detects if a line starts a heredoc and extracts the delimiter.
///
/// Handles variants: `<<EOF`, `<<'EOF'`, `<<"EOF"`, `<<-EOF` (tab-stripped).
/// Excludes here-strings (`<<<`) which are single-line constructs.
pub fn detect_heredoc_start(line: &str) -> Option<String> {
    // Here-strings (<<<) are NOT heredocs
    if line.contains("<<<") {
        return None;
    }

    // Try patterns in order of specificity (quoted first)
    const PATTERNS: &[&str] = &["<<'", "<<\"", "<<-'", "<<-\"", "<<-", "<<"];

    for pattern in PATTERNS {
        if let Some(delim) = try_extract_delimiter(line, pattern) {
            return Some(delim);
        }
    }
    None
}

// ============================================================================
// Internal types and helpers
// ============================================================================

/// Tracks whether we're inside a heredoc block.
enum HeredocState {
    None,
    Inside { delimiter: String },
}

/// What to do with a processed line.
enum LineAction {
    Include(String),
    Skip,
}

/// Process a single line based on current heredoc state.
fn process_line(line: &str, heredoc_state: &mut HeredocState) -> LineAction {
    match heredoc_state {
        HeredocState::Inside { delimiter } => {
            let is_end = line.trim() == delimiter;
            if is_end {
                *heredoc_state = HeredocState::None;
            }
            LineAction::Include(line.to_string())
        }
        HeredocState::None => process_normal_line(line, heredoc_state),
    }
}

/// Handle a normal line - check for heredoc start, filter comments/blanks.
fn process_normal_line(line: &str, heredoc_state: &mut HeredocState) -> LineAction {
    // Check for heredoc start first
    if let Some(delim) = detect_heredoc_start(line) {
        *heredoc_state = HeredocState::Inside { delimiter: delim };
        return LineAction::Include(line.trim_start().to_string());
    }

    let trimmed = line.trim();

    // Skip blank lines
    if trimmed.is_empty() {
        return LineAction::Skip;
    }

    // Preserve shebang
    if trimmed.starts_with("#!") {
        return LineAction::Include(trimmed.to_string());
    }

    // Remove comment-only lines
    if trimmed.starts_with('#') {
        return LineAction::Skip;
    }

    // Normal line - strip indentation
    LineAction::Include(trimmed.to_string())
}

/// Try to extract a heredoc delimiter using the given pattern.
fn try_extract_delimiter(line: &str, pattern: &str) -> Option<String> {
    let pos = line.find(pattern)?;
    let rest = &line[pos + pattern.len()..];

    let delim = if pattern.ends_with('\'') || pattern.ends_with('"') {
        // Quoted delimiter: extract until closing quote
        let quote = pattern.chars().last().unwrap();
        rest.split(quote).next().map(|s| s.to_string())
    } else {
        // Unquoted delimiter: first word, strip any surrounding quotes
        rest.split_whitespace()
            .next()
            .map(|s| s.trim_matches(|c| c == '\'' || c == '"').to_string())
    };

    delim.filter(|d| !d.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_heredoc_basic() {
        assert_eq!(detect_heredoc_start("cat <<EOF"), Some("EOF".to_string()));
    }

    #[test]
    fn test_detect_heredoc_quoted() {
        assert_eq!(detect_heredoc_start("cat <<'EOF'"), Some("EOF".to_string()));
        assert_eq!(
            detect_heredoc_start("cat <<\"EOF\""),
            Some("EOF".to_string())
        );
    }

    #[test]
    fn test_detect_heredoc_tab_stripped() {
        assert_eq!(detect_heredoc_start("cat <<-EOF"), Some("EOF".to_string()));
    }

    #[test]
    fn test_here_string_not_heredoc() {
        assert_eq!(detect_heredoc_start("cat <<< \"hello\""), None);
    }

    #[test]
    fn test_remove_comments() {
        let input = "# comment\necho hello\n# another";
        let result = remove_comments_and_blanks(input);
        assert_eq!(result, vec!["echo hello"]);
    }

    #[test]
    fn test_preserve_shebang() {
        let input = "#!/bin/bash\n# comment\necho hi";
        let result = remove_comments_and_blanks(input);
        assert_eq!(result, vec!["#!/bin/bash", "echo hi"]);
    }

    #[test]
    fn test_strip_indentation() {
        let input = "  echo a\n    echo b";
        let result = remove_comments_and_blanks(input);
        assert_eq!(result, vec!["echo a", "echo b"]);
    }
}
