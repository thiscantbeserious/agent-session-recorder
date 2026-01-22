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
pub const BOX_WIDTH: usize = 39;

/// Bottom border of the box
pub const BOX_BOTTOM: &str = "╚═══════════════════════════════════════╝";

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

/// Truncate a string to fit within max_width display columns, adding "..." if needed
pub fn truncate_str(s: &str, max_width: usize) -> String {
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
