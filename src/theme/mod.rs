//! Theme configuration for TUI and CLI
//!
//! Centralizes all color, style, and branding definitions.
//! Provides ratatui styles (for TUI), ANSI escape codes (for CLI),
//! and logo/banner assets (for branding).

use ratatui::style::Color;

pub mod cli;
pub mod logo;
pub mod tui;

// Re-exports from cli.rs
pub use cli::ansi;
pub use cli::{color_to_ansi, colorize_help, ANSI_RESET};

// Re-exports from logo.rs
pub use logo::{
    print_box_bottom, print_box_line, print_box_line_end, print_box_prompt, print_done_banner,
    print_full_logo, print_start_banner, truncate_str, BOX_BOTTOM, BOX_WIDTH, LOGO_DONE, LOGO_FULL,
    LOGO_START,
};

/// Theme configuration for the TUI and CLI.
///
/// All colors are defined here for easy customization.
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
            text_primary: Color::Gray,
            text_secondary: Color::DarkGray,
            accent: Color::LightGreen,
            error: Color::Red,
            success: Color::LightGreen,
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
}
