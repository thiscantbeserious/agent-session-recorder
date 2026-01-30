//! Shell script minification
//!
//! Provides aggressive compression for shell scripts to minimize line count
//! while preserving functionality.
//!
//! # Architecture
//!
//! The minification pipeline has three phases:
//! 1. **Comment/blank removal** - Strip non-functional content
//! 2. **Statement joining** - Combine lines with semicolons
//! 3. **Operator compression** - Remove whitespace around operators
//!
//! # Safety Rules (from shfmt analysis)
//!
//! - `< <(cmd)` must preserve space (process substitution)
//! - Heredoc content never modified
//! - Word boundaries preserved: `echo $var` not `echo$var`
//! - Quoted strings preserved verbatim
//! - Don't join across control structure boundaries

mod comments;
mod compress;
mod join;

/// Minify a shell script for embedding in RC files.
///
/// Applies aggressive compression:
/// - Removes all comments except shebang
/// - Removes blank lines and indentation
/// - Joins statements with semicolons
/// - Collapses function bodies to single lines
/// - Inlines if/then/fi and case/esac structures
/// - Removes spaces around operators (&&, ||, |, redirects)
///
/// # Safety
/// The following are preserved to maintain shell correctness:
/// - Content inside quotes (single and double)
/// - Space before process substitution `< <(cmd)`
/// - Heredoc content
/// - Word boundaries (prevents `echo$var`)
/// - Zsh parameter expansion flags `${(f)...}`
///
/// # Example
/// ```
/// use agr::shell::minify;
/// let input = "echo a\necho b";
/// let output = minify::exec(input);
/// assert_eq!(output, "echo a;echo b");
/// ```
pub fn exec(script: &str) -> String {
    let processed = comments::remove_comments_and_blanks(script);
    let joined = join::join_statements(&processed);
    compress::compress_operators(&joined)
}

/// Return script unchanged for debugging/readability.
///
/// Use this instead of [`exec`] when you need readable output.
pub fn debug(script: &str) -> String {
    script.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_joins_statements() {
        let input = "echo a\necho b";
        assert_eq!(exec(input), "echo a;echo b");
    }

    #[test]
    fn test_removes_comments() {
        let input = "# comment\necho hello\n# another";
        assert_eq!(exec(input), "echo hello");
    }

    #[test]
    fn test_preserves_shebang() {
        let input = "#!/bin/bash\n# comment\necho hi";
        assert_eq!(exec(input), "#!/bin/bash\necho hi");
    }

    #[test]
    fn test_debug_mode_preserves_input() {
        let input = "echo a\n# comment\necho b";
        assert_eq!(debug(input), input);
    }

    #[test]
    fn test_preserves_quoted_content() {
        let input = "echo \"hello world\"\n# comment";
        assert_eq!(exec(input), "echo \"hello world\"");
    }

    #[test]
    fn test_here_string_not_treated_as_heredoc() {
        let input = "while read -r line; do echo $line; done <<< \"$input\"";
        let output = exec(input);
        assert!(
            !output.contains('\n'),
            "here-string should not create newlines"
        );
    }
}
