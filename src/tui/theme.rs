//! Theme configuration for TUI and CLI
//!
//! Centralizes all color and style definitions for easy customization.
//! Provides both ratatui styles (for TUI) and ANSI escape codes (for CLI).

use ratatui::style::{Color, Modifier, Style};

/// Theme configuration for the TUI.
///
/// All colors and styles are defined here for easy customization.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Primary text color (used for most content)
    pub text_primary: Color,
    /// Secondary/dimmed text color
    pub text_secondary: Color,
    /// Accent color for highlights and important elements
    pub accent: Color,
    /// Error/warning color
    pub error: Color,
    /// Success color
    pub success: Color,
    /// Background color (usually default/transparent)
    pub background: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::claude_code()
    }
}

impl Theme {
    /// AGR theme - light gray text with green logo accent.
    /// Uses standard ANSI colors for consistent terminal rendering.
    pub fn claude_code() -> Self {
        Self {
            text_primary: Color::Gray,       // Light gray for help text (ANSI 37)
            text_secondary: Color::DarkGray, // Dark gray for footer hints
            accent: Color::LightGreen,       // Bright green (ANSI 92) for logo
            error: Color::Red,
            success: Color::LightGreen, // Bright green for done banner
            background: Color::Reset,
        }
    }

    /// Classic terminal theme - white text.
    pub fn classic() -> Self {
        Self {
            text_primary: Color::White,
            text_secondary: Color::DarkGray,
            accent: Color::Yellow,
            error: Color::Red,
            success: Color::Green,
            background: Color::Reset,
        }
    }

    /// Cyan/blue theme.
    pub fn ocean() -> Self {
        Self {
            text_primary: Color::Cyan,
            text_secondary: Color::DarkGray,
            accent: Color::LightCyan,
            error: Color::Red,
            success: Color::Green,
            background: Color::Reset,
        }
    }

    // Style helpers

    /// Style for primary text content.
    pub fn text_style(&self) -> Style {
        Style::default().fg(self.text_primary)
    }

    /// Style for secondary/dimmed text.
    pub fn text_secondary_style(&self) -> Style {
        Style::default().fg(self.text_secondary)
    }

    /// Style for accented/highlighted text.
    pub fn accent_style(&self) -> Style {
        Style::default().fg(self.accent)
    }

    /// Style for bold accented text (keybindings, etc).
    pub fn accent_bold_style(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// Style for error text.
    pub fn error_style(&self) -> Style {
        Style::default().fg(self.error)
    }

    /// Style for success text.
    pub fn success_style(&self) -> Style {
        Style::default().fg(self.success)
    }

    /// Style for highlighted/selected items in dialogs and menus.
    /// Uses black text on accent background for readability.
    pub fn highlight_style(&self) -> Style {
        Style::default()
            .fg(Color::Black)
            .bg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    // ANSI color helpers for CLI output

    /// Format text with the accent color (for CLI output).
    pub fn accent_text(&self, text: &str) -> String {
        format!("{}{}{}", color_to_ansi(self.accent), text, ANSI_RESET)
    }

    /// Format text with the primary color (for CLI output).
    pub fn primary_text(&self, text: &str) -> String {
        format!("{}{}{}", color_to_ansi(self.text_primary), text, ANSI_RESET)
    }

    /// Format text with the secondary color (for CLI output).
    pub fn secondary_text(&self, text: &str) -> String {
        format!(
            "{}{}{}",
            color_to_ansi(self.text_secondary),
            text,
            ANSI_RESET
        )
    }

    /// Format text with the error color (for CLI output).
    pub fn error_text(&self, text: &str) -> String {
        format!("{}{}{}", color_to_ansi(self.error), text, ANSI_RESET)
    }

    /// Format text with the success color (for CLI output).
    pub fn success_text(&self, text: &str) -> String {
        format!("{}{}{}", color_to_ansi(self.success), text, ANSI_RESET)
    }
}

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

/// Colorize CLI help text using the theme.
///
/// - Logo lines (containing Unicode block chars) are colored green
/// - REC line prefix is colored green, dashes are dark gray
/// - Description text is colored gray
///
/// This post-processes clap's output to apply consistent theming.
pub fn colorize_help(text: &str) -> String {
    let theme = current_theme();
    let green = color_to_ansi(theme.accent);
    let gray = color_to_ansi(theme.text_primary);
    let dark_gray = color_to_ansi(theme.text_secondary);
    let reset = ANSI_RESET;

    // Process each line
    text.lines()
        .map(|line| {
            // Skip lines that already have ANSI codes
            if line.contains("\x1b[") {
                return line.to_string();
            }

            // Detect logo lines (contain Unicode block characters like █ ╔ ╗ ║ ╚ ╝ ═)
            // Use bright green (ANSI 92) to match terminal's native green
            if line.contains('\u{2588}')
                || line.contains('\u{2554}')
                || line.contains('\u{2557}')
                || line.contains('\u{2551}')
                || line.contains('\u{255A}')
                || line.contains('\u{255D}')
                || line.contains('\u{2550}')
            {
                return format!("{}{}{}", green, line, reset);
            }

            // Detect REC line (starts with ⏺ REC)
            // REC prefix in bright green, dashes are dark gray
            if line.contains("\u{23FA}") && line.contains("REC") {
                if let Some(dash_start) = line.find('\u{2500}') {
                    let (prefix, dashes) = line.split_at(dash_start);
                    return format!(
                        "{}{}{}{}{}{}",
                        green, prefix, reset, dark_gray, dashes, reset
                    );
                }
                return format!("{}{}{}", green, line, reset);
            }

            // For command lines, wrap description in gray
            if line.contains("  ") {
                let trimmed = line.trim_start();
                if !trimmed.is_empty() && !trimmed.starts_with('-') && !trimmed.starts_with('<') {
                    if let Some(desc_start) = find_description_start(line) {
                        let (prefix, desc) = line.split_at(desc_start);
                        if !desc.trim().is_empty() {
                            return format!("{}{}{}{}", prefix, gray, desc, reset);
                        }
                    }
                }
            }

            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Find the start position of description text in a help line.
fn find_description_start(line: &str) -> Option<usize> {
    // Look for the pattern: word(s) followed by 2+ spaces
    // The description starts after those spaces
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
                // Found description start
                return Some(i);
            }
            in_word = true;
            space_count = 0;
        }
    }
    None
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

/// Global theme instance.
///
/// In the future, this could be loaded from config.
pub fn current_theme() -> Theme {
    Theme::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_is_claude_code() {
        let theme = Theme::default();
        // text_primary is light gray, accent is bright green
        assert_eq!(theme.text_primary, Color::Gray);
        assert_eq!(theme.accent, Color::LightGreen);
    }

    #[test]
    fn classic_theme_uses_white() {
        let theme = Theme::classic();
        assert_eq!(theme.text_primary, Color::White);
    }

    #[test]
    fn ocean_theme_uses_cyan() {
        let theme = Theme::ocean();
        assert_eq!(theme.text_primary, Color::Cyan);
    }

    #[test]
    fn style_helpers_return_correct_colors() {
        let theme = Theme::claude_code();
        assert_eq!(theme.text_style().fg, Some(Color::Gray));
        assert_eq!(theme.text_secondary_style().fg, Some(Color::DarkGray));
        assert_eq!(theme.accent_style().fg, Some(Color::LightGreen));
    }

    #[test]
    fn ansi_text_helpers_wrap_with_color_codes() {
        let theme = Theme::claude_code();

        // Accent text should wrap with bright green
        let accent = theme.accent_text("test");
        assert!(accent.starts_with("\x1b[92m")); // Bright green (ANSI 92)
        assert!(accent.ends_with("\x1b[0m")); // Reset
        assert!(accent.contains("test"));

        // Primary text should wrap with gray
        let primary = theme.primary_text("hello");
        assert!(primary.starts_with("\x1b[37m")); // Gray
        assert!(primary.ends_with("\x1b[0m"));
        assert!(primary.contains("hello"));
    }

    #[test]
    fn color_to_ansi_maps_standard_colors() {
        assert_eq!(color_to_ansi(Color::Green), "\x1b[32m");
        assert_eq!(color_to_ansi(Color::Red), "\x1b[31m");
        assert_eq!(color_to_ansi(Color::Gray), "\x1b[37m");
        assert_eq!(color_to_ansi(Color::DarkGray), "\x1b[90m");
        assert_eq!(color_to_ansi(Color::Reset), "\x1b[0m");
    }

    #[test]
    fn highlight_style_uses_black_on_accent() {
        let theme = Theme::claude_code();
        let style = theme.highlight_style();
        assert_eq!(style.fg, Some(Color::Black));
        assert_eq!(style.bg, Some(Color::LightGreen)); // accent color
        assert!(style.add_modifier.contains(ratatui::style::Modifier::BOLD));
    }
}
