//! Phase 3: Compress operators
//!
//! Removes whitespace around operators (&&, ||, redirects) while
//! preserving spaces required for shell correctness.

/// Remove unnecessary whitespace around operators.
///
/// Preserves spaces needed for:
/// - Process substitution: `< <(cmd)` must keep space
/// - Quoted content: never modified
/// - File descriptor redirects: `2>&1` not `2 >&1`
pub fn compress_operators(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();
    let mut quote_state = QuoteState::None;
    let mut prev_char = ' ';

    while let Some(c) = chars.next() {
        quote_state.update(c, prev_char);

        if quote_state.is_inside() {
            result.push(c);
            prev_char = c;
            continue;
        }

        if c == ' ' && should_skip_space(&result, &mut chars, prev_char) {
            prev_char = c;
            continue;
        }

        result.push(c);
        prev_char = c;
    }

    result
}

// ============================================================================
// Quote tracking
// ============================================================================

/// Tracks whether we're inside single or double quotes.
enum QuoteState {
    None,
    SingleQuote,
    DoubleQuote,
}

impl QuoteState {
    /// Update quote state based on current character.
    fn update(&mut self, c: char, prev: char) {
        if prev == '\\' {
            return; // Escaped character, don't change state
        }

        match (c, &self) {
            ('\'', QuoteState::None) => *self = QuoteState::SingleQuote,
            ('\'', QuoteState::SingleQuote) => *self = QuoteState::None,
            ('"', QuoteState::None) => *self = QuoteState::DoubleQuote,
            ('"', QuoteState::DoubleQuote) => *self = QuoteState::None,
            _ => {}
        }
    }

    /// Check if currently inside any quotes.
    fn is_inside(&self) -> bool {
        !matches!(self, QuoteState::None)
    }
}

// ============================================================================
// Space handling
// ============================================================================

/// Determine if a space should be skipped (compressed away).
fn should_skip_space(
    result: &str,
    chars: &mut std::iter::Peekable<std::str::Chars>,
    prev_char: char,
) -> bool {
    let Some(&next) = chars.peek() else {
        return false;
    };

    // Space after && or ||
    if (prev_char == '&' || prev_char == '|') && (result.ends_with("&&") || result.ends_with("||"))
    {
        return true;
    }

    match next {
        '&' | '|' => can_skip_space_before_operator(result),
        '<' => should_skip_space_before_input_redirect(chars),
        '>' => should_skip_space_before_output_redirect(result),
        _ => false,
    }
}

/// Check if space before && or || can be removed.
fn can_skip_space_before_operator(result: &str) -> bool {
    result.ends_with(|x: char| x.is_alphanumeric() || matches!(x, '$' | ')' | '"' | '\'' | '}'))
}

/// Check if space before < should be kept or removed.
///
/// Must preserve space for process substitution: `< <(cmd)`
fn should_skip_space_before_input_redirect(
    chars: &mut std::iter::Peekable<std::str::Chars>,
) -> bool {
    let mut peek = chars.clone();
    peek.next(); // Skip the '<'

    // Keep space if followed by another '<' or '(' (process substitution)
    !matches!(peek.peek(), Some(&'<') | Some(&'('))
}

/// Check if space before > should be removed.
///
/// Keep space after digits (file descriptor redirects like `2>`)
fn should_skip_space_before_output_redirect(result: &str) -> bool {
    !result.ends_with(|x: char| x.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_and_operator() {
        let input = "cmd1 && cmd2";
        assert_eq!(compress_operators(input), "cmd1&&cmd2");
    }

    #[test]
    fn test_compress_or_operator() {
        let input = "cmd1 || cmd2";
        assert_eq!(compress_operators(input), "cmd1||cmd2");
    }

    #[test]
    fn test_preserve_process_substitution() {
        let input = "cmd < <(other)";
        let output = compress_operators(input);
        assert!(
            output.contains("< <("),
            "process substitution space must be preserved"
        );
    }

    #[test]
    fn test_preserve_quoted_content() {
        let input = "echo \"hello   world\"";
        assert_eq!(compress_operators(input), "echo \"hello   world\"");
    }

    #[test]
    fn test_preserve_single_quoted() {
        let input = "echo 'hello   world'";
        assert_eq!(compress_operators(input), "echo 'hello   world'");
    }

    #[test]
    fn test_fd_redirect_preserved() {
        let input = "cmd 2>&1";
        // Space before 2 should be kept
        assert!(compress_operators(input).contains("2>"));
    }
}
