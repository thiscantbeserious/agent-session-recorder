//! Ratatui Style helpers for Theme
//!
//! All methods that return `ratatui::style::Style` live here,
//! isolating the ratatui dependency to a single file within the theme module.

use ratatui::style::{Color, Modifier, Style};

use super::Theme;

impl Theme {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_helpers_return_correct_colors() {
        let theme = Theme::claude_code();
        assert_eq!(theme.text_style().fg, Some(Color::Gray));
        assert_eq!(theme.text_secondary_style().fg, Some(Color::DarkGray));
        assert_eq!(theme.accent_style().fg, Some(Color::LightGreen));
    }

    #[test]
    fn highlight_style_uses_black_on_accent() {
        let theme = Theme::claude_code();
        let style = theme.highlight_style();
        assert_eq!(style.fg, Some(Color::Black));
        assert_eq!(style.bg, Some(Color::LightGreen));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }
}
