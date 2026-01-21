//! Branding and ASCII art logos for AGR
//!
//! Logos are embedded at compile time from the assets directory.

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

/// Truncate a string to fit within max_width, adding "…" if needed
fn truncate_str(s: &str, max_width: usize) -> String {
    let char_count: usize = s.chars().count();
    if char_count <= max_width {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_width - 1).collect();
        format!("{}…", truncated)
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
    fn truncate_str_handles_unicode() {
        // Should truncate by character count, not bytes
        assert_eq!(truncate_str("日本語テスト", 4), "日本語…");
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
