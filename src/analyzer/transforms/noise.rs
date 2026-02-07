//! Content-based noise classification for terminal output.
//!
//! [`NoiseClassifier`] provides structural heuristics that detect common TUI
//! noise patterns by shape rather than by hardcoded strings. It acts as a
//! fallback for one-shot noise that the behavioral row-rewrite detector in
//! [`super::TerminalTransform`] cannot catch (lines that appear exactly once
//! before scrolling off).

/// Minimum number of key-binding patterns required to classify a line as a
/// keybinding hint bar.
const MIN_KEYBINDING_HITS: usize = 2;

/// Structural noise classifier.
///
/// Detects noise by the *shape* of a line rather than by matching specific
/// strings. This generalises across different agent TUIs (Claude Code, Cursor,
/// Codex CLI, etc.) because the structural patterns are universal.
pub struct NoiseClassifier;

impl NoiseClassifier {
    /// Returns `true` if the line looks like one-shot TUI noise.
    ///
    /// Three structural checks, in order:
    /// 1. Spinner / ellipsis line (short, few words, ends with `…` or `...`)
    /// 2. Key-binding hint bar (2+ modifier-key patterns)
    /// 3. Metadata prefix line (`Tip:`, `Hint:`, `Update available`, etc.)
    pub fn is_noise(line: &str) -> bool {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return false;
        }
        Self::is_spinner_line(trimmed)
            || Self::is_keybinding_bar(trimmed)
            || Self::is_metadata_prefix(trimmed)
            || Self::is_status_summary(trimmed)
    }

    /// Heuristic 1: Spinner / ellipsis status line.
    ///
    /// A single natural-language word ending in an ellipsis character (`…`)
    /// or three dots (`...`), under 80 characters. This catches animated
    /// spinner text like "Shimmying…", "Razzle-dazzling…", "Loading...", etc.
    ///
    /// Restricted to exactly one word to avoid false positives on code
    /// fragments ("impl Foo..."). Multi-word spinners are reliably caught
    /// by the behavioral row-rewrite detector instead.
    fn is_spinner_line(s: &str) -> bool {
        if s.len() >= 80 {
            return false;
        }
        let ends_ellipsis = s.ends_with('…') || s.ends_with("...");
        if !ends_ellipsis {
            return false;
        }
        // Strip the ellipsis and check what remains
        let stem = s.trim_end_matches('…').trim_end_matches("...");
        let words: Vec<&str> = stem.split_whitespace().collect();
        if words.is_empty() {
            // Just "…" or "..." alone
            return true;
        }
        if words.len() != 1 {
            return false;
        }
        // The single word must be a natural-language word (only letters/hyphens)
        words[0]
            .chars()
            .all(|c| c.is_alphabetic() || c == '-' || c == '\'')
    }

    /// Heuristic 2: Key-binding hint bar.
    ///
    /// Lines containing 2+ key-binding patterns like `Ctrl+X`, `shift+Tab`,
    /// `Esc`, `(Tab to cycle)`, etc. These are toolbar / hint bars that every
    /// TUI renders at the bottom or top of the screen.
    ///
    /// Counts total *occurrences* (not unique patterns) so "Ctrl+C … Ctrl+D"
    /// scores 2 even though both match the same "ctrl+" pattern.
    fn is_keybinding_bar(s: &str) -> bool {
        let mut hits = 0usize;
        let lower = s.to_ascii_lowercase();

        // Modifier+key combos: count every occurrence
        for pattern in &["ctrl+", "alt+", "shift+", "cmd+", "meta+", "super+"] {
            hits += lower.matches(pattern).count();
        }

        // Standalone key names in parentheses: "(Tab", "(Esc", "(Enter"
        for pattern in &["(tab", "(esc", "(enter"] {
            hits += lower.matches(pattern).count();
        }

        // "key to action" phrases: "Esc to cancel", "Tab to cycle"
        for pattern in &["esc to ", "tab to ", "enter to "] {
            hits += lower.matches(pattern).count();
        }

        hits >= MIN_KEYBINDING_HITS
    }

    /// Heuristic 3: Metadata prefix line.
    ///
    /// Lines starting with a known short prefix that agents use for tips,
    /// update notices, and context indicators. This is a small structural
    /// allowlist — we match the *prefix shape*, not the full content.
    fn is_metadata_prefix(s: &str) -> bool {
        s.starts_with("Tip:")
            || s.starts_with("Hint:")
            || s.starts_with("Note:")
            || s.starts_with("Update available")
            || s.starts_with("Context left until")
    }

    /// Heuristic 4: Status summary / thinking indicator.
    ///
    /// Short lines (< 60 chars) that are agent status indicators:
    /// - Thinking indicators: short line that is *only* the word "thinking"
    ///   or a very short phrase containing it (not prose like "thinking about X")
    /// - Tool/task summary counters: "Done (in Xs | N tool uses)" pattern
    fn is_status_summary(s: &str) -> bool {
        // Thinking indicator: very short, standalone "thinking" line
        // (must be < 40 chars to avoid matching prose sentences)
        if s.len() < 40 {
            let lower = s.to_ascii_lowercase();
            if lower == "thinking"
                || lower.ends_with("thinking")
                || lower.ends_with("thinking…")
                || lower.ends_with("thinking...")
                || lower.ends_with("(thinking)")
                || lower.ends_with("(thinking…)")
                || lower.ends_with("(thinking...)")
            {
                return true;
            }
        }

        // Tool summary counter: "Done" + duration/count pattern
        if s.contains("Done") && (s.contains("tool use") || s.contains("tool call")) {
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Spinner / Ellipsis ──────────────────────────────────────────

    #[test]
    fn spinner_single_word_ellipsis() {
        assert!(NoiseClassifier::is_noise("Shimmying…"));
        assert!(NoiseClassifier::is_noise("  Orbiting…  "));
        assert!(NoiseClassifier::is_noise("Loading..."));
        assert!(NoiseClassifier::is_noise("Razzle-dazzling…"));
    }

    #[test]
    fn spinner_rejects_multi_word() {
        // Multi-word spinners are caught by behavioral detection, not content.
        // "Claude is thinking…" is caught by is_status_summary instead.
        assert!(!NoiseClassifier::is_noise("Please wait..."));
    }

    #[test]
    fn spinner_rejects_long_lines() {
        // Real code line that happens to end with "..."
        let long = "a".repeat(80) + "...";
        assert!(!NoiseClassifier::is_noise(&long));
    }

    #[test]
    fn spinner_rejects_prose() {
        // Sentence that ends with "..." but has many words
        assert!(!NoiseClassifier::is_noise(
            "The quick brown fox jumps over the lazy dog..."
        ));
    }

    #[test]
    fn spinner_rejects_code_ellipsis() {
        // Code identifiers with punctuation are not natural-language words
        assert!(!NoiseClassifier::is_noise("impl Foo..."));
        assert!(!NoiseClassifier::is_noise("foo_bar..."));
        assert!(!NoiseClassifier::is_noise("std::io..."));
    }

    #[test]
    fn spinner_accepts_natural_language_ellipsis() {
        // Single natural-language word + ellipsis is a spinner
        assert!(NoiseClassifier::is_noise("Error…"));
        assert!(NoiseClassifier::is_noise("Waiting..."));
    }

    // ── Key-Binding Bar ─────────────────────────────────────────────

    #[test]
    fn keybinding_bar_detected() {
        assert!(NoiseClassifier::is_noise(
            "accept edits on (shift+Tab to cycle)"
        ));
        assert!(NoiseClassifier::is_noise(
            "Ctrl+C to cancel  Ctrl+D to exit"
        ));
        assert!(NoiseClassifier::is_noise(
            "Press Esc to cancel, Tab to next"
        ));
    }

    #[test]
    fn keybinding_bar_rejects_prose() {
        // Single mention of a key in normal text
        assert!(!NoiseClassifier::is_noise(
            "Press Ctrl+C to interrupt the process"
        ));
    }

    #[test]
    fn keybinding_bar_rejects_code() {
        assert!(!NoiseClassifier::is_noise(
            "if event.key == Key::Tab { handle_tab() }"
        ));
    }

    // ── Metadata Prefix ─────────────────────────────────────────────

    #[test]
    fn metadata_prefix_detected() {
        assert!(NoiseClassifier::is_noise("Tip: use /help for assistance"));
        assert!(NoiseClassifier::is_noise("Hint: try the new feature"));
        assert!(NoiseClassifier::is_noise("Update available! v2.0.0"));
        assert!(NoiseClassifier::is_noise(
            "Context left until auto-compact: 50%"
        ));
    }

    #[test]
    fn metadata_prefix_rejects_similar() {
        // "Tip" inside a word or after other text
        assert!(!NoiseClassifier::is_noise("Tips and tricks for Rust"));
        assert!(!NoiseClassifier::is_noise("tooltip.show()"));
        // "Note" in the middle of a sentence
        assert!(!NoiseClassifier::is_noise(
            "Please note that this is important"
        ));
    }

    // ── Status Summary / Thinking ──────────────────────────────────

    #[test]
    fn thinking_indicator_detected() {
        assert!(NoiseClassifier::is_noise("thinking"));
        assert!(NoiseClassifier::is_noise("  thinking  "));
        assert!(NoiseClassifier::is_noise("Claude thinking"));
        assert!(NoiseClassifier::is_noise("thinking…"));
        assert!(NoiseClassifier::is_noise("thinking..."));
    }

    #[test]
    fn thinking_rejects_prose() {
        // Real sentences containing "thinking" — too long or not at end
        assert!(!NoiseClassifier::is_noise(
            "thinking about the architecture of the system"
        ));
        assert!(!NoiseClassifier::is_noise(
            "I was thinking we should refactor this module"
        ));
    }

    #[test]
    fn tool_summary_detected() {
        assert!(NoiseClassifier::is_noise("Done (in 3.2s | 5 tool uses)"));
        assert!(NoiseClassifier::is_noise("Done (in 1s | 1 tool use)"));
        assert!(NoiseClassifier::is_noise("Done (12 tool calls)"));
    }

    #[test]
    fn tool_summary_rejects_unrelated() {
        assert!(!NoiseClassifier::is_noise("Done with the refactoring"));
        assert!(!NoiseClassifier::is_noise("The tool uses a config file"));
    }

    // ── Regression: patterns that leaked in real recordings ────────

    #[test]
    fn regression_concatenated_status_bar() {
        // Full-width TUI status bar with multiple elements concatenated
        assert!(NoiseClassifier::is_noise(
            "? for shortcuts  esc to interrupt                                                    Update available! Run: brew upgrade claude-code  esc to interrupt  content?"
        ));
    }

    #[test]
    fn regression_spinner_with_parenthesized_thinking() {
        assert!(NoiseClassifier::is_noise("Razzle-dazzling… (thinking)"));
        assert!(NoiseClassifier::is_noise("Clauding… (thinking)"));
    }

    // ── Negative cases: real content must NOT be noise ──────────────

    #[test]
    fn real_content_not_noise() {
        assert!(!NoiseClassifier::is_noise("fn main() {"));
        assert!(!NoiseClassifier::is_noise("    let x = 42;"));
        assert!(!NoiseClassifier::is_noise("$ cargo build"));
        assert!(!NoiseClassifier::is_noise("error[E0308]: mismatched types"));
        assert!(!NoiseClassifier::is_noise("Hello, world!"));
        assert!(!NoiseClassifier::is_noise(""));
        assert!(!NoiseClassifier::is_noise("   "));
    }
}
