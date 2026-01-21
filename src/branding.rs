//! Branding and ASCII art logos for AGR
//!
//! Logos are embedded at compile time from the assets directory.

use unicode_width::UnicodeWidthStr;

/// Full ASCII logo for interactive CLI mode
pub const LOGO_FULL: &str = include_str!("../assets/logo.txt");

/// Small banner shown when starting a recording session
pub const LOGO_START: &str = include_str!("../assets/logo_small.txt");

/// Small banner shown when a recording session ends
pub const LOGO_DONE: &str = include_str!("../assets/logo_small_done.txt");

/// Box width (inner content width, excluding borders)
const BOX_WIDTH: usize = 39;

/// Bottom border of the box
const BOX_BOTTOM: &str = "╚═══════════════════════════════════════╝";

/// Print the start banner
pub fn print_start_banner() {
    print!("{}", LOGO_START);
}

/// Print the done banner
pub fn print_done_banner() {
    print!("{}", LOGO_DONE);
}

/// Print the full logo
pub fn print_full_logo() {
    print!("{}", LOGO_FULL);
}

/// Print a line inside the box, padded to fit
pub fn print_box_line(content: &str) {
    let truncated = truncate_str(content, BOX_WIDTH);
    println!("║{:width$}║", truncated, width = BOX_WIDTH);
}

/// Print the bottom border of the box
pub fn print_box_bottom() {
    println!("{}", BOX_BOTTOM);
}

/// Print a prompt line inside the box (no trailing border - user types after)
pub fn print_box_prompt(content: &str) {
    print!("║{:width$}", content, width = BOX_WIDTH);
}

/// Print just the closing border character (after user input on prompt line)
pub fn print_box_line_end() {
    println!("║");
}

/// Truncate a string to fit within max_width display columns, adding "…" if needed
fn truncate_str(s: &str, max_width: usize) -> String {
    let display_width = s.width();
    if display_width <= max_width {
        s.to_string()
    } else {
        // Ellipsis "…" has display width of 1
        const ELLIPSIS: &str = "…";
        const ELLIPSIS_WIDTH: usize = 1;

        let target_width = max_width.saturating_sub(ELLIPSIS_WIDTH);
        let mut truncated = String::new();
        let mut current_width = 0;

        for c in s.chars() {
            let char_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if current_width + char_width > target_width {
                break;
            }
            truncated.push(c);
            current_width += char_width;
        }

        format!("{}{}", truncated, ELLIPSIS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_full_is_not_empty() {
        assert!(!LOGO_FULL.is_empty());
        assert!(LOGO_FULL.contains("A G E N T"));
        assert!(LOGO_FULL.contains("R E C O R D E R"));
    }

    #[test]
    fn logo_start_is_not_empty() {
        assert!(!LOGO_START.is_empty());
        assert!(LOGO_START.contains("AGR"));
        assert!(LOGO_START.contains("REC"));
    }

    #[test]
    fn logo_done_is_not_empty() {
        assert!(!LOGO_DONE.is_empty());
        assert!(LOGO_DONE.contains("AGR"));
        assert!(LOGO_DONE.contains("DONE"));
    }

    #[test]
    fn logos_have_box_borders() {
        // Full logo has complete box
        assert!(LOGO_FULL.contains('╔'));
        assert!(LOGO_FULL.contains('╚'));
        // Small logos have top and separator (bottom added programmatically)
        assert!(LOGO_START.contains('╔'));
        assert!(LOGO_START.contains('╠'));
        assert!(LOGO_DONE.contains('╔'));
        assert!(LOGO_DONE.contains('╠'));
    }

    #[test]
    fn truncate_str_short_unchanged() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn truncate_str_exact_length() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn truncate_str_adds_ellipsis() {
        assert_eq!(truncate_str("hello world", 8), "hello w…");
    }

    #[test]
    fn truncate_str_handles_unicode_display_width() {
        // CJK characters have display width of 2 each
        // "日本語テスト" = 6 chars, 12 display width
        // With max_width=5, target_width=4, can fit "日本" (width 4) + "…" (width 1) = 5
        assert_eq!(truncate_str("日本語テスト", 5), "日本…");
        // With max_width=7, target_width=6, can fit "日本語" (width 6) + "…" = 7
        assert_eq!(truncate_str("日本語テスト", 7), "日本語…");
    }

    #[test]
    fn truncate_str_handles_wide_emoji() {
        // Emoji typically have display width of 2
        // "hello🎉world" - "hello" (5) + "🎉" (2) + "world" (5) = 12 display width
        assert_eq!(truncate_str("hello🎉world", 8), "hello🎉…");
    }

    #[test]
    fn box_width_matches_bottom_border() {
        // BOX_BOTTOM should be ║ + BOX_WIDTH chars + ║
        // Actually it's ╚ + BOX_WIDTH ═ chars + ╝
        let border_inner: String = BOX_BOTTOM.chars().skip(1).take(BOX_WIDTH).collect();
        assert!(border_inner.chars().all(|c| c == '═'));
    }

    #[test]
    fn print_functions_do_not_panic() {
        // Simple coverage tests - just verify they run without panicking
        print_start_banner();
        print_done_banner();
        print_full_logo();
        print_box_line("test content");
        print_box_line("a very long string that should be truncated to fit the box width");
        print_box_bottom();
        print_box_prompt("prompt: ");
        print_box_line_end();
    }
}
