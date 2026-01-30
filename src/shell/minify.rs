//! Shell script minification
//!
//! Removes comments and blank lines from shell scripts while preserving functionality.

/// Minify a shell script by removing comments and blank lines.
///
/// Preserves:
/// - Shebang lines (#!/...)
/// - Lines containing # inside strings (approximation: # not at line start after trim)
/// - All functional code
///
/// Removes:
/// - Comment-only lines (starting with #, except shebang)
/// - Blank/whitespace-only lines
pub fn minify(script: &str) -> String {
    script
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return false;
            }
            // Keep shebang
            if trimmed.starts_with("#!") {
                return true;
            }
            // Remove comment-only lines
            if trimmed.starts_with('#') {
                return false;
            }
            true
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_removes_comments() {
        let input = "# comment\necho hello\n# another";
        assert_eq!(minify(input), "echo hello");
    }

    #[test]
    fn test_removes_blank_lines() {
        let input = "echo a\n\n\necho b";
        assert_eq!(minify(input), "echo a\necho b");
    }

    #[test]
    fn test_preserves_shebang() {
        let input = "#!/bin/bash\n# comment\necho hi";
        assert_eq!(minify(input), "#!/bin/bash\necho hi");
    }

    #[test]
    fn test_preserves_inline_hash() {
        let input = "echo \"#hashtag\"\n# comment";
        assert_eq!(minify(input), "echo \"#hashtag\"");
    }

    #[test]
    fn test_preserves_variable_with_hash() {
        let input = "color=\"#ff0000\"\n# this is a comment";
        assert_eq!(minify(input), "color=\"#ff0000\"");
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(minify(""), "");
    }

    #[test]
    fn test_only_comments() {
        let input = "# comment 1\n# comment 2";
        assert_eq!(minify(input), "");
    }
}
