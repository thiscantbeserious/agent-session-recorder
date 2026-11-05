//! ANSI color helpers and CLI text formatting
//!
//! Provides ANSI escape codes for CLI output, color conversion from
//! ratatui colors, themed text wrappers, and help text colorization.

use std::io::IsTerminal;

use ratatui::style::Color;

use super::Theme;

/// ANSI reset sequence
pub const ANSI_RESET: &str = "\x1b[0m";

/// ANSI color codes for CLI output - exposed for clap styling
pub mod ansi {
    /// Gray color (ANSI 37) - used for descriptions
    pub const GRAY: &str = "\x1b[37m";
    /// Green color (ANSI 32) - used for accent/headers
    pub const GREEN: &str = "\x1b[32m";
    /// Red color (ANSI 31) - used for errors
    pub const RED: &str = "\x1b[31m";
    /// Dark gray (ANSI 90) - used for secondary text
    pub const DARK_GRAY: &str = "\x1b[90m";
    /// Reset color
    pub const RESET: &str = "\x1b[0m";
}

/// Pure colour decision, see <https://no-color.org>.
///
/// Colour only when stdout is a terminal and `NO_COLOR` is unset or empty.
pub fn colors_enabled(is_tty: bool, no_color: Option<&str>) -> bool {
    is_tty && no_color.is_none_or(|v| v.is_empty())
}

/// Runtime decision for CLI colour on stdout.
///
/// `AGR_FORCE_COLOR` set to a non-empty value forces colour on regardless of tty and
/// `NO_COLOR`. It exists only so snapshot tests can capture colour without a tty.
/// Not cached on purpose, the env read is cheap and tests can set the override late.
pub fn cli_colors() -> bool {
    let forced = std::env::var("AGR_FORCE_COLOR").is_ok_and(|v| !v.is_empty());
    let no_color = std::env::var("NO_COLOR").ok();
    forced || colors_enabled(std::io::stdout().is_terminal(), no_color.as_deref())
}

/// An ANSI code, or an empty string when CLI colour is off.
pub fn gate(code: &'static str) -> &'static str {
    if cli_colors() {
        code
    } else {
        ""
    }
}

/// Wrap text in a colour code and reset, or return it unchanged when colour is off.
fn paint(color: Color, text: &str) -> String {
    paint_if(cli_colors(), color, text)
}

fn paint_if(on: bool, color: Color, text: &str) -> String {
    if on {
        format!("{}{}{}", color_to_ansi(color), text, ANSI_RESET)
    } else {
        text.to_string()
    }
}

/// Convert a ratatui Color to an ANSI escape code.
pub fn color_to_ansi(color: Color) -> &'static str {
    match color {
        Color::Black => "\x1b[30m",
        Color::Red => "\x1b[31m",
        Color::Green => "\x1b[32m",
        Color::Yellow => "\x1b[33m",
        Color::Blue => "\x1b[34m",
        Color::Magenta => "\x1b[35m",
        Color::Cyan => "\x1b[36m",
        Color::Gray => "\x1b[37m",
        Color::DarkGray => "\x1b[90m",
        Color::LightRed => "\x1b[91m",
        Color::LightGreen => "\x1b[92m",
        Color::LightYellow => "\x1b[93m",
        Color::LightBlue => "\x1b[94m",
        Color::LightMagenta => "\x1b[95m",
        Color::LightCyan => "\x1b[96m",
        Color::White => "\x1b[97m",
        Color::Reset => "\x1b[0m",
        // For RGB and indexed colors, fall back to reset (no color)
        _ => "",
    }
}

impl Theme {
    /// Format text with the accent color (for CLI output).
    pub fn accent_text(&self, text: &str) -> String {
        paint(self.accent, text)
    }

    /// Format text with the primary color (for CLI output).
    pub fn primary_text(&self, text: &str) -> String {
        paint(self.text_primary, text)
    }

    /// Format text with the secondary color (for CLI output).
    pub fn secondary_text(&self, text: &str) -> String {
        paint(self.text_secondary, text)
    }

    /// Format text with the error color (for CLI output).
    pub fn error_text(&self, text: &str) -> String {
        paint(self.error, text)
    }

    /// Format text with the success color (for CLI output).
    pub fn success_text(&self, text: &str) -> String {
        paint(self.success, text)
    }
}

/// Colorize CLI help text using the theme.
///
/// - Logo lines (containing Unicode block chars) are colored green
/// - REC line prefix is colored green, dashes are dark gray
/// - Description text is colored gray
///
/// This post-processes clap's output to apply consistent theming.
pub fn colorize_help(text: &str) -> String {
    if !cli_colors() {
        return text.to_string();
    }
    let theme = super::current_theme();
    let green = color_to_ansi(theme.accent);
    let gray = color_to_ansi(theme.text_primary);
    let dark_gray = color_to_ansi(theme.text_secondary);
    let reset = ANSI_RESET;

    text.lines()
        .map(|line| colorize_help_line(line, green, gray, dark_gray, reset))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Colorize a single help line.
fn colorize_help_line(line: &str, green: &str, gray: &str, dark_gray: &str, reset: &str) -> String {
    if line.contains("\x1b[") {
        return line.to_string();
    }

    if is_logo_line(line) {
        return format!("{}{}{}", green, line, reset);
    }

    if let Some(colored) = colorize_rec_line(line, green, dark_gray, reset) {
        return colored;
    }

    if let Some(colored) = colorize_command_line(line, gray, reset) {
        return colored;
    }

    line.to_string()
}

/// Check if a line contains Unicode block characters used in the logo.
fn is_logo_line(line: &str) -> bool {
    line.contains('\u{2588}')
        || line.contains('\u{2554}')
        || line.contains('\u{2557}')
        || line.contains('\u{2551}')
        || line.contains('\u{255A}')
        || line.contains('\u{255D}')
        || line.contains('\u{2550}')
}

/// Colorize a REC indicator line, returning None if not a REC line.
fn colorize_rec_line(line: &str, green: &str, dark_gray: &str, reset: &str) -> Option<String> {
    if !line.contains("\u{23FA}") || !line.contains("REC") {
        return None;
    }
    if let Some(dash_start) = line.find('\u{2500}') {
        let (prefix, dashes) = line.split_at(dash_start);
        return Some(format!(
            "{}{}{}{}{}{}",
            green, prefix, reset, dark_gray, dashes, reset
        ));
    }
    Some(format!("{}{}{}", green, line, reset))
}

/// Colorize a command description line, returning None if not applicable.
fn colorize_command_line(line: &str, gray: &str, reset: &str) -> Option<String> {
    if !line.contains("  ") {
        return None;
    }
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('-') || trimmed.starts_with('<') {
        return None;
    }
    if let Some(desc_start) = find_description_start(line) {
        let (prefix, desc) = line.split_at(desc_start);
        if !desc.trim().is_empty() {
            return Some(format!("{}{}{}{}", prefix, gray, desc, reset));
        }
    }
    None
}

/// Find the start position of description text in a help line.
fn find_description_start(line: &str) -> Option<usize> {
    let mut in_word = false;
    let mut space_count = 0;
    let mut last_word_end = 0;

    for (i, c) in line.char_indices() {
        if c.is_whitespace() {
            if in_word {
                in_word = false;
                last_word_end = i;
                space_count = 1;
            } else {
                space_count += 1;
            }
        } else {
            if !in_word && space_count >= 2 && last_word_end > 0 {
                return Some(i);
            }
            in_word = true;
            space_count = 0;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_wraps_only_when_on() {
        let theme = Theme::claude_code();
        let on = paint_if(true, theme.accent, "test");
        assert_eq!(on, "\x1b[92mtest\x1b[0m");
        assert_eq!(paint_if(false, theme.accent, "test"), "test");
    }

    #[test]
    fn colors_enabled_needs_tty() {
        assert!(!colors_enabled(false, None));
        assert!(colors_enabled(true, None));
    }

    #[test]
    fn colors_enabled_honours_no_color() {
        assert!(!colors_enabled(true, Some("1")));
        assert!(colors_enabled(true, Some("")));
    }

    #[test]
    fn color_to_ansi_maps_standard_colors() {
        assert_eq!(color_to_ansi(Color::Green), "\x1b[32m");
        assert_eq!(color_to_ansi(Color::Red), "\x1b[31m");
        assert_eq!(color_to_ansi(Color::Gray), "\x1b[37m");
        assert_eq!(color_to_ansi(Color::DarkGray), "\x1b[90m");
        assert_eq!(color_to_ansi(Color::Reset), "\x1b[0m");
    }
}
