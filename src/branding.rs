//! Branding and ASCII art logos for AGR
//!
//! Logos are embedded at compile time from the assets directory.
//! Uses the theme system for consistent colors across TUI and CLI.

use crate::tui::theme::current_theme;
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

/// Print the start banner with theme colors
/// The logo is green except for ◉ and REC which are red
pub fn print_start_banner() {
    let theme = current_theme();
    // Apply colors: accent for most, red for ◉ and REC
    let colored = colorize_recording_banner(LOGO_START, &theme);
    print!("{}", colored);
}

/// Print the done banner with theme colors
/// The done banner is green except for ◉ and DONE which are also green (success)
pub fn print_done_banner() {
    let theme = current_theme();
    // For done banner, we use success color for the whole thing (including ◉/DONE)
    print!("{}", theme.success_text(LOGO_DONE));
}

/// Colorize the recording banner with bold REC
fn colorize_recording_banner(text: &str, theme: &crate::tui::theme::Theme) -> String {
    use crate::tui::theme::{color_to_ansi, ANSI_RESET};

    let accent = color_to_ansi(theme.accent);
    const BOLD: &str = "\x1b[1m";

    let mut result = String::new();

    for line in text.lines() {
        // Check if this line contains REC
        if line.contains("REC") {
            // Process character by character for this line
            let mut in_rec = false;
            let mut chars = line.chars().peekable();

            result.push_str(accent);

            while let Some(c) = chars.next() {
                // Check for "REC" - make it bold
                if c == 'R' && !in_rec {
                    // Peek ahead to check for "EC"
                    let rest: String = chars.clone().take(2).collect();
                    if rest == "EC" {
                        result.push_str(BOLD);
                        result.push_str("REC");
                        result.push_str(ANSI_RESET);
                        result.push_str(accent);
                        chars.next(); // skip E
                        chars.next(); // skip C
                        in_rec = true;
                    } else {
                        result.push(c);
                    }
                } else {
                    result.push(c);
                }
            }
            result.push_str(ANSI_RESET);
            result.push('\n');
        } else {
            // No special coloring needed, use accent for the whole line
            result.push_str(accent);
            result.push_str(line);
            result.push_str(ANSI_RESET);
            result.push('\n');
        }
    }

    result
}

/// Print the full logo with theme colors
pub fn print_full_logo() {
    let theme = current_theme();
    print!("{}", theme.accent_text(LOGO_FULL));
}

/// Print a line inside the box, padded to fit (with accent color)
pub fn print_box_line(content: &str) {
    let theme = current_theme();
    let truncated = truncate_str(content, BOX_WIDTH);
    println!(
        "{}",
        theme.accent_text(&format!("║{:width$}║", truncated, width = BOX_WIDTH))
    );
}

/// Print the bottom border of the box (with accent color)
pub fn print_box_bottom() {
    let theme = current_theme();
    println!("{}", theme.accent_text(BOX_BOTTOM));
}

/// Print a prompt line inside the box (no trailing border - user types after)
pub fn print_box_prompt(content: &str) {
    use crate::tui::theme::{color_to_ansi, ANSI_RESET};
    let theme = current_theme();
    // Don't reset color at end so user input follows in same color
    print!(
        "{}║{:width$}",
        color_to_ansi(theme.accent),
        content,
        width = BOX_WIDTH
    );
    print!("{}", ANSI_RESET);
}

/// Print just the closing border character (after user input on prompt line)
pub fn print_box_line_end() {
    let theme = current_theme();
    println!("{}", theme.accent_text("║"));
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
