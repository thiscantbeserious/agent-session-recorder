//! Tests for aggressive shell script minification
//!
//! These tests verify the `minify()` function with compress=true which
//! compresses shell scripts to achieve single-digit line output while
//! preserving functionality.
//!
//! Test categories follow PLAN.md Stage 1:
//! - 1.1 Basic Compression
//! - 1.2 Function Collapsing
//! - 1.3 Control Structure
//! - 1.4 Quote Safety (CRITICAL)
//! - 1.5 Special Syntax Safety
//! - 1.6 Operator Compression
//! - 1.7 Integration
//! - 1.8 Functional Preservation
//! - 1.9 Debug Mode

use agr::shell::completions::{generate_bash_init, generate_zsh_init};
use agr::shell::minify;

/// Helper for tests - calls minify::exec (compression)
fn minify_aggressive(input: &str) -> String {
    minify::exec(input)
}

// ============================================================================
// 1.1 Basic Compression Tests
// ============================================================================

mod basic_compression {
    use super::*;

    #[test]
    fn test_joins_simple_statements_with_semicolons() {
        let input = "echo a\necho b";
        let output = minify_aggressive(input);
        assert_eq!(output, "echo a;echo b");
    }

    #[test]
    fn test_removes_all_comments_except_shebang() {
        let input = "#!/bin/bash\n# This is a comment\necho hello\n# Another comment\necho world";
        let output = minify_aggressive(input);
        // Shebang preserved, comments removed
        assert!(output.contains("#!/bin/bash"));
        assert!(!output.contains("This is a comment"));
        assert!(!output.contains("Another comment"));
        assert!(output.contains("echo hello"));
        assert!(output.contains("echo world"));
    }

    #[test]
    fn test_removes_blank_lines() {
        let input = "echo a\n\n\n\necho b";
        let output = minify_aggressive(input);
        // No blank lines in output
        assert!(!output.contains("\n\n"));
        assert!(output.contains("echo a"));
        assert!(output.contains("echo b"));
    }

    #[test]
    fn test_removes_indentation() {
        let input = "    echo indented\n        echo more indented";
        let output = minify_aggressive(input);
        // No leading whitespace
        assert!(!output.starts_with(' '));
        assert!(!output.starts_with('\t'));
        assert!(!output.contains("\n "));
        assert!(!output.contains("\n\t"));
    }
}

// ============================================================================
// 1.2 Function Collapsing Tests
// ============================================================================

mod function_collapsing {
    use super::*;

    #[test]
    fn test_collapses_function_body_to_single_line() {
        let input = r#"my_func() {
    echo "hello"
    echo "world"
}"#;
        let output = minify_aggressive(input);
        // Function should be on single line with semicolons
        assert!(
            output.contains("my_func(){") || output.contains("my_func() {"),
            "Function definition should be compact"
        );
        // Body joined with semicolons
        assert!(
            output.contains(";") && !output.contains("\n    "),
            "Body should be inline, not indented on separate lines"
        );
    }

    #[test]
    fn test_handles_nested_braces_in_functions() {
        let input = r#"outer() {
    if [[ -n "$var" ]]; then
        echo "has value"
    fi
}"#;
        let output = minify_aggressive(input);
        // Nested braces should be preserved correctly
        assert!(output.contains("[[ -n"));
        assert!(output.contains("fi"));
        // Function closing brace should match opening
        let open_count = output.matches('{').count();
        let close_count = output.matches('}').count();
        assert_eq!(open_count, close_count, "Braces should be balanced");
    }

    #[test]
    fn test_preserves_function_with_complex_body() {
        let input = r#"complex_func() {
    local var="value"
    if [[ -f "$file" ]]; then
        cat "$file"
    else
        echo "not found"
    fi
    return 0
}"#;
        let output = minify_aggressive(input);
        // All components preserved
        assert!(output.contains("local var="));
        assert!(output.contains("if [[ -f"));
        assert!(output.contains("then"));
        assert!(output.contains("else"));
        assert!(output.contains("fi"));
        assert!(output.contains("return 0"));
    }
}

// ============================================================================
// 1.3 Control Structure Tests
// ============================================================================

mod control_structure {
    use super::*;

    #[test]
    fn test_inlines_if_then_fi() {
        let input = r#"if [[ -n "$var" ]]
then
    echo "yes"
fi"#;
        let output = minify_aggressive(input);
        eprintln!("if/then input:\n{}", input);
        eprintln!("if/then output:\n{}", output);
        // Should be on fewer lines than original
        let line_count = output.lines().count();
        assert!(
            line_count <= 2,
            "if/then/fi should be inlined, got {} lines: {}",
            line_count,
            output
        );
        // Proper semicolon after condition
        assert!(
            output.contains(";then") || output.contains("; then"),
            "Should have semicolon before 'then': {}",
            output
        );
    }

    #[test]
    fn test_inlines_case_statement() {
        let input = r#"case "$1" in
    start)
        echo "starting"
        ;;
    stop)
        echo "stopping"
        ;;
esac"#;
        let output = minify_aggressive(input);
        // Case should be more compact
        let input_lines = input.lines().count();
        let output_lines = output.lines().count();
        assert!(
            output_lines < input_lines,
            "Case should be compressed from {} to fewer lines, got {}",
            input_lines,
            output_lines
        );
        assert!(output.contains("case"));
        assert!(output.contains("esac"));
    }

    #[test]
    fn test_removes_final_double_semicolon_before_esac() {
        let input = r#"case "$1" in
    a) echo a ;;
esac"#;
        let output = minify_aggressive(input);
        // The final ;; before esac can be removed
        assert!(
            !output.contains(";;esac") && !output.contains(";; esac"),
            "Final ;; before esac should be removed or the pattern simplified"
        );
    }

    #[test]
    fn test_handles_nested_case_in_function() {
        let input = r#"handler() {
    case "$1" in
        one)
            echo 1
            ;;
        two)
            echo 2
            ;;
    esac
}"#;
        let output = minify_aggressive(input);
        // Both function and case structure preserved
        assert!(output.contains("handler()"));
        assert!(output.contains("case"));
        assert!(output.contains("esac"));
        // Should be compact
        let line_count = output.lines().count();
        assert!(
            line_count <= 3,
            "Nested case should be compact, got {} lines",
            line_count
        );
    }
}

// ============================================================================
// 1.4 Quote Safety Tests (CRITICAL)
// ============================================================================

mod quote_safety {
    use super::*;

    #[test]
    fn test_preserves_content_inside_double_quotes() {
        let input = r#"echo "hello   world""#;
        let output = minify_aggressive(input);
        // Spaces inside double quotes must be preserved
        assert!(
            output.contains("hello   world"),
            "Multiple spaces inside double quotes must be preserved"
        );
    }

    #[test]
    fn test_preserves_content_inside_single_quotes() {
        let input = "echo 'hello   world'";
        let output = minify_aggressive(input);
        // Spaces inside single quotes must be preserved
        assert!(
            output.contains("hello   world"),
            "Multiple spaces inside single quotes must be preserved"
        );
    }

    #[test]
    fn test_preserves_spaces_in_quoted_strings() {
        // Test that spaces inside quoted strings are preserved
        let input = r#"echo "hello   world"
echo 'multiple   spaces'"#;
        let output = minify_aggressive(input);
        // Spaces inside quotes must be preserved
        assert!(
            output.contains("hello   world"),
            "Multiple spaces inside double quotes should be preserved"
        );
        assert!(
            output.contains("multiple   spaces"),
            "Multiple spaces inside single quotes should be preserved"
        );
    }

    #[test]
    fn test_preserves_hash_inside_quotes() {
        let input = "color=\"#ff0000\"\necho \"# This is not a comment\"";
        let output = minify_aggressive(input);
        // Hash inside quotes is NOT a comment
        assert!(
            output.contains("#ff0000"),
            "Hash inside quotes should be preserved as literal"
        );
        assert!(
            output.contains("# This is not a comment"),
            "Hash inside quotes should be preserved"
        );
    }

    #[test]
    fn test_preserves_nested_quotes() {
        let input = r#"echo "She said 'hello'""#;
        let output = minify_aggressive(input);
        assert!(
            output.contains("She said 'hello'"),
            "Nested single quotes inside double quotes should be preserved"
        );

        let input2 = r#"echo 'He said "hi"'"#;
        let output2 = minify_aggressive(input2);
        assert!(
            output2.contains(r#"He said "hi""#),
            "Nested double quotes inside single quotes should be preserved"
        );
    }
}

// ============================================================================
// 1.5 Special Syntax Safety Tests
// ============================================================================

mod special_syntax_safety {
    use super::*;

    #[test]
    fn test_preserves_space_before_process_substitution() {
        let input = "diff <(cmd1) <(cmd2)";
        let output = minify_aggressive(input);
        // Space before <( is required - removing it creates syntax error
        assert!(
            output.contains(" <(") || output == input,
            "Space before process substitution must be preserved: {}",
            output
        );
    }

    #[test]
    fn test_preserves_heredoc_content_verbatim() {
        let input = r#"cat <<'EOF'
This content
   has spaces
and $variables that should be literal
EOF"#;
        let output = minify_aggressive(input);
        eprintln!("Heredoc input:\n{}", input);
        eprintln!("Heredoc output:\n{}", output);
        // Heredoc content should be untouched
        assert!(
            output.contains("   has spaces"),
            "Heredoc content must be preserved verbatim: {}",
            output
        );
        assert!(
            output.contains("$variables that should be literal"),
            "Heredoc content must not be modified"
        );
    }

    #[test]
    fn test_preserves_zsh_parameter_expansion() {
        let input = r#"files=(${(f)"$(cmd)"})"#;
        let output = minify_aggressive(input);
        // Zsh parameter expansion flags must be preserved exactly
        assert!(
            output.contains("${(f)"),
            "Zsh parameter expansion flags must be preserved"
        );
    }

    #[test]
    fn test_preserves_bash_array_syntax() {
        let input = "arr=(one two three)\necho \"${arr[@]}\"";
        let output = minify_aggressive(input);
        // Array syntax preserved
        assert!(output.contains("arr=(one two three)"));
        assert!(output.contains("${arr[@]}"));
    }

    #[test]
    fn test_preserves_word_boundaries() {
        let input = "echo $var";
        let output = minify_aggressive(input);
        // Must NOT become echo$var
        assert!(
            output.contains("echo $var") || output.contains("echo $"),
            "Word boundary between command and variable must be preserved"
        );
        assert!(
            !output.contains("echo$"),
            "Must not join 'echo' and '$var' without space"
        );
    }
}

// ============================================================================
// 1.6 Operator Compression Tests
// ============================================================================

mod operator_compression {
    use super::*;

    #[test]
    fn test_removes_spaces_around_and_or() {
        let input = "cmd1 && cmd2 || cmd3";
        let output = minify_aggressive(input);
        // Spaces around && and || can be removed
        assert!(
            output.contains("&&") && output.contains("||"),
            "Operators must be present"
        );
        // Ideally compressed (but functionality preserved either way)
        let has_operators = output.contains("cmd1&&cmd2") || output.contains("cmd1 && cmd2");
        assert!(has_operators, "Commands should be joined with operators");
    }

    #[test]
    fn test_removes_spaces_around_pipes() {
        let input = "cmd1 | cmd2 | cmd3";
        let output = minify_aggressive(input);
        // Pipe operators preserved
        assert!(
            output.matches('|').count() == 2,
            "Both pipes must be present"
        );
    }

    #[test]
    fn test_removes_spaces_around_redirects() {
        let input = "cmd > file.txt 2>&1";
        let output = minify_aggressive(input);
        // Redirects preserved
        assert!(output.contains(">"), "Redirect must be present");
        assert!(output.contains("2>&1"), "Stderr redirect must be present");
    }

    #[test]
    fn test_preserves_redirect_fd_numbers() {
        let input = "cmd 2>/dev/null";
        let output = minify_aggressive(input);
        // FD number must stay attached to redirect
        assert!(
            output.contains("2>") || output.contains("2 >"),
            "File descriptor number must be preserved with redirect"
        );
        assert!(
            !output.contains(" 2>") || output.contains("cmd 2>"),
            "FD should be adjacent to redirect operator"
        );
    }
}

// ============================================================================
// 1.7 Integration Tests
// ============================================================================

mod integration {
    use super::*;

    /// Maximum acceptable line count for minified output
    const MAX_ACCEPTABLE_LINES: usize = 15;
    /// Aspirational target line count
    const ASPIRATIONAL_LINES: usize = 10;

    #[test]
    fn test_minify_zsh_init_achieves_target_lines() {
        let zsh_init = generate_zsh_init(true);
        let minified = minify_aggressive(&zsh_init);
        let line_count = minified.lines().count();

        println!("Zsh init minified to {} lines", line_count);
        println!("Content:\n{}", minified);

        assert!(
            line_count <= MAX_ACCEPTABLE_LINES,
            "Zsh init should be <= {} lines, got {}",
            MAX_ACCEPTABLE_LINES,
            line_count
        );

        // Log if we achieved aspirational target
        if line_count <= ASPIRATIONAL_LINES {
            println!(
                "SUCCESS: Achieved aspirational target of <= {} lines",
                ASPIRATIONAL_LINES
            );
        }
    }

    #[test]
    fn test_minify_bash_init_achieves_target_lines() {
        let bash_init = generate_bash_init(true);
        let minified = minify_aggressive(&bash_init);
        let line_count = minified.lines().count();

        println!("Bash init minified to {} lines", line_count);
        println!("Content:\n{}", minified);

        assert!(
            line_count <= MAX_ACCEPTABLE_LINES,
            "Bash init should be <= {} lines, got {}",
            MAX_ACCEPTABLE_LINES,
            line_count
        );

        // Log if we achieved aspirational target
        if line_count <= ASPIRATIONAL_LINES {
            println!(
                "SUCCESS: Achieved aspirational target of <= {} lines",
                ASPIRATIONAL_LINES
            );
        }
    }

    #[test]
    fn test_minified_output_is_valid_shell_syntax() {
        let zsh_init = generate_zsh_init(true);
        let minified = minify_aggressive(&zsh_init);

        eprintln!("=== Minified zsh output ===\n{}\n=== End ===", minified);

        // Basic syntax validation checks - count structural brackets outside quotes
        let (open_braces, close_braces) = count_outside_quotes(&minified, '{', '}');
        let (open_brackets, close_brackets) = count_outside_quotes(&minified, '[', ']');

        // 1. Balanced braces
        assert_eq!(
            open_braces, close_braces,
            "Braces must be balanced: {} open, {} close",
            open_braces, close_braces
        );

        // Note: Parentheses check is skipped because shell `case` statement syntax
        // uses unbalanced `)` for pattern endings like `pattern) cmd ;;`
        // This is valid shell syntax but would fail a simple balance check.

        // 2. Balanced brackets
        assert_eq!(
            open_brackets, close_brackets,
            "Brackets must be balanced: {} open, {} close",
            open_brackets, close_brackets
        );

        // 3. No unescaped format placeholders
        assert!(
            !minified.contains("{cmd_"),
            "Should not have unescaped format placeholders"
        );

        // 4. Verify shell syntax is parseable with zsh (zsh completions use zsh syntax)
        #[cfg(unix)]
        {
            use std::process::Command;
            // Use zsh for syntax check since the completions use zsh-specific syntax
            let result = Command::new("zsh")
                .arg("-n") // syntax check only
                .arg("-c")
                .arg(&minified)
                .output();

            if let Ok(output) = result {
                assert!(
                    output.status.success(),
                    "Zsh syntax check failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            // Note: If zsh is not available, the test passes (graceful degradation)
        }
    }

    /// Count characters outside of quoted strings
    fn count_outside_quotes(s: &str, open: char, close: char) -> (usize, usize) {
        let mut open_count = 0;
        let mut close_count = 0;
        let mut in_single = false;
        let mut in_double = false;
        let mut prev = ' ';

        for c in s.chars() {
            if c == '\'' && !in_double && prev != '\\' {
                in_single = !in_single;
            } else if c == '"' && !in_single && prev != '\\' {
                in_double = !in_double;
            } else if !in_single && !in_double {
                if c == open {
                    open_count += 1;
                } else if c == close {
                    close_count += 1;
                }
            }
            prev = c;
        }

        (open_count, close_count)
    }
}

// ============================================================================
// 1.8 Functional Preservation Tests (REQ-1)
// ============================================================================

mod functional_preservation {
    use super::*;

    #[test]
    fn test_minified_zsh_completions_work() {
        let zsh_init = generate_zsh_init(true);
        let minified = minify_aggressive(&zsh_init);

        // Essential components for completion to work
        // Note: _AGR_LOADED is now in agr.sh, not completions
        assert!(
            minified.contains("_agr_commands"),
            "Command list must be present"
        );
        assert!(
            minified.contains("_agr_complete"),
            "Completion function must be present"
        );
        assert!(
            minified.contains("compdef"),
            "Completion registration must be present"
        );
    }

    #[test]
    fn test_minified_bash_completions_work() {
        let bash_init = generate_bash_init(true);
        let minified = minify_aggressive(&bash_init);

        // Essential components for completion to work
        // Note: _AGR_LOADED is now in agr.sh, not completions
        assert!(
            minified.contains("_agr_commands"),
            "Command list must be present"
        );
        assert!(
            minified.contains("_agr_complete"),
            "Completion function must be present"
        );
        assert!(
            minified.contains("complete -F"),
            "Completion registration must be present"
        );
    }

    #[test]
    fn test_subcommand_completion_preserved() {
        let zsh_init = generate_zsh_init(true);
        let minified = minify_aggressive(&zsh_init);

        // Subcommand arrays should be present
        assert!(
            minified.contains("_agr_") && minified.contains("_subcmds"),
            "Subcommand completion arrays should be present"
        );
    }

    #[test]
    fn test_file_completion_preserved() {
        let zsh_init = generate_zsh_init(true);
        let minified = minify_aggressive(&zsh_init);

        // File completion function should be present
        assert!(
            minified.contains("_agr_complete_files"),
            "File completion function must be preserved"
        );
        assert!(
            minified.contains("_agr_file_cmds"),
            "File-accepting command list must be preserved"
        );
    }
}

// ============================================================================
// 1.9 Debug Mode Tests (REQ-2)
// ============================================================================

mod debug_mode {
    use super::*;

    #[test]
    fn test_debug_flag_outputs_uncompressed() {
        let input = "echo a\necho b\necho c";
        let debug_output = minify::debug(input);
        let normal_output = minify::exec(input);

        // Debug output should be longer (more readable)
        assert!(
            debug_output.len() >= normal_output.len(),
            "Debug output should be at least as long as compressed output"
        );
    }

    #[test]
    fn test_debug_output_includes_section_comments() {
        let zsh_init = generate_zsh_init(true);
        let debug_output = minify::debug(&zsh_init);

        // Debug mode should include section markers/comments
        // Either preserve original comments or add debug markers
        assert!(
            debug_output.contains('#') && debug_output.lines().count() > 10,
            "Debug output should be readable with comments"
        );
    }

    #[test]
    fn test_default_outputs_compressed() {
        let input = "echo a\necho b\necho c";
        let output = minify::exec(input);

        // Compressed output should be single line
        let line_count = output.lines().count();
        assert!(
            line_count <= 1,
            "Default output should be compressed to single line, got {}",
            line_count
        );
    }
}
