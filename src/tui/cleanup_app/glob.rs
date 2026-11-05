//! Glob matching for the cleanup app glob-select mode.

/// Parse an optional `agent/file_pattern` glob syntax into its two components.
///
/// Returns `(Some(agent_pattern), file_pattern)` when a `/` separator is present,
/// or `(None, pattern)` for a bare file pattern.
pub(super) fn parse_glob_pattern(pattern: &str) -> (Option<&str>, &str) {
    if let Some(slash_pos) = pattern.find('/') {
        (Some(&pattern[..slash_pos]), &pattern[slash_pos + 1..])
    } else {
        (None, pattern)
    }
}

/// Return whether an item's agent/name matches the parsed glob filter.
///
/// When `agent_filter` is `Some`, both the agent and name must match their
/// respective patterns. When `None`, only the name is matched against
/// `file_pattern`.
pub(super) fn matches_glob_pattern(
    agent: &str,
    name: &str,
    agent_filter: Option<&str>,
    file_pattern: &str,
) -> bool {
    if let Some(agent_pat) = agent_filter {
        glob_match(agent, agent_pat) && glob_match(name, file_pattern)
    } else {
        glob_match(name, file_pattern)
    }
}

/// Simple glob pattern matching.
/// Supports * (match any) and ? (match single char).
pub(super) fn glob_match(text: &str, pattern: &str) -> bool {
    let text = text.to_lowercase();
    let pattern = pattern.to_lowercase();

    glob_match_recursive(&text, &pattern)
}

fn glob_match_recursive(text: &str, pattern: &str) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }

    let mut pattern_chars = pattern.chars().peekable();
    let mut text_chars = text.chars().peekable();

    while let Some(p) = pattern_chars.next() {
        match p {
            '*' => {
                // Collect remaining pattern after *
                let rest_pattern: String = pattern_chars.collect();

                // If * is at the end, match everything
                if rest_pattern.is_empty() {
                    return true;
                }

                // Try matching rest of pattern at each position
                let rest_text: String = text_chars.collect();
                for (i, _) in rest_text.char_indices() {
                    if glob_match_recursive(&rest_text[i..], &rest_pattern) {
                        return true;
                    }
                }
                return glob_match_recursive("", &rest_pattern);
            }
            '?' => {
                // Match any single character
                if text_chars.next().is_none() {
                    return false;
                }
            }
            c => {
                // Match literal character
                match text_chars.next() {
                    Some(t) if t == c => {}
                    _ => return false,
                }
            }
        }
    }

    // Pattern exhausted, text should be exhausted too
    text_chars.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_cast_pattern_matches_gzip_sessions_too() {
        assert!(matches_glob_pattern(
            "claude",
            "x.cast.gz",
            None,
            "*.cast.gz"
        ));
        assert!(matches_glob_pattern(
            "claude",
            "x.cast.gz",
            Some("claude"),
            "*.cast*"
        ));
        assert!(!matches_glob_pattern("claude", "x.cast.gz", None, "*.cast"));
    }
}
