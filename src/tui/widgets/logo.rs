//! Logo widget for AGR
//!
//! Displays the AGR ASCII logo with a dynamic REC line that scales to terminal width.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Widget,
};

use crate::tui::current_theme;

/// The AGR ASCII logo
const LOGO_LINES: [&str; 6] = [
    " \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}",
    "\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2550}\u{2550}\u{255D} \u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2588}\u{2588}\u{2557}",
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2551}  \u{2588}\u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2554}\u{255D}",
    "\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2551}   \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2588}\u{2588}\u{2557}",
    "\u{2588}\u{2588}\u{2551}  \u{2588}\u{2588}\u{2551}\u{255A}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2554}\u{255D}\u{2588}\u{2588}\u{2551}  \u{2588}\u{2588}\u{2551}",
    "\u{255A}\u{2550}\u{255D}  \u{255A}\u{2550}\u{255D} \u{255A}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255D} \u{255A}\u{2550}\u{255D}  \u{255A}\u{2550}\u{255D}",
];

/// REC indicator prefix
const REC_PREFIX: &str = " \u{23FA} REC ";
/// Width of REC prefix in display columns
const REC_PREFIX_WIDTH: usize = 7;

/// Logo widget that displays the AGR ASCII logo with a dynamic REC line.
///
/// The REC line scales to fill the available terminal width.
#[derive(Debug, Default, Clone)]
pub struct Logo {
    /// Style for the logo text
    logo_style: Style,
    /// Style for the REC indicator
    rec_style: Style,
    /// Style for the dashes
    dash_style: Style,
}

impl Logo {
    /// Create a new Logo widget with styling from the current theme.
    pub fn new() -> Self {
        let theme = current_theme();
        Self {
            logo_style: theme.accent_style(), // Logo in accent color (green)
            rec_style: theme.accent_style(),  // REC in accent color (green)
            dash_style: theme.text_secondary_style(), // Dashes in muted gray
        }
    }

    /// Set the style for the logo text.
    pub fn logo_style(mut self, style: Style) -> Self {
        self.logo_style = style;
        self
    }

    /// Set the style for the REC indicator.
    pub fn rec_style(mut self, style: Style) -> Self {
        self.rec_style = style;
        self
    }

    /// Set the style for the dashes.
    pub fn dash_style(mut self, style: Style) -> Self {
        self.dash_style = style;
        self
    }

    /// Get the height required for the logo (logo lines + REC line + padding).
    pub fn height() -> u16 {
        // 2 blank lines + logo lines + 1 REC line
        2 + LOGO_LINES.len() as u16 + 1
    }

    /// Build the REC line that scales to the given width.
    fn build_rec_line(&self, width: u16) -> Line<'static> {
        let dash_count = (width as usize).saturating_sub(REC_PREFIX_WIDTH);
        let dashes = "\u{2500}".repeat(dash_count);

        Line::from(vec![
            Span::styled(REC_PREFIX.to_string(), self.rec_style),
            Span::styled(dashes, self.dash_style),
        ])
    }
}

impl Widget for Logo {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let mut y = area.y;

        // Add blank line at top
        y += 1;
        if y >= area.y + area.height {
            return;
        }

        // Another blank line
        y += 1;
        if y >= area.y + area.height {
            return;
        }

        // Render logo lines
        for line in &LOGO_LINES {
            if y >= area.y + area.height {
                return;
            }
            let line_widget = Line::styled(*line, self.logo_style);
            buf.set_line(area.x, y, &line_widget, area.width);
            y += 1;
        }

        // Render REC line
        if y < area.y + area.height {
            let rec_line = self.build_rec_line(area.width);
            buf.set_line(area.x, y, &rec_line, area.width);
        }
    }
}

/// Generate the logo as a static string for non-TUI contexts.
///
/// This is used when output is piped (not a TTY).
/// Note: Colors are applied by colorize_help() post-processing since
/// clap strips ANSI codes from before_help content.
pub fn build_static_logo(width: usize) -> String {
    let dash_count = width.saturating_sub(REC_PREFIX_WIDTH);
    let rec_line = format!("{}{}", REC_PREFIX, "\u{2500}".repeat(dash_count));

    let logo_text = LOGO_LINES.join("\n");
    format!("\n{}\n{}\n", logo_text, rec_line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn logo_height_is_correct() {
        // 2 blank + 6 logo + 1 rec = 9
        assert_eq!(Logo::height(), 9);
    }

    #[test]
    fn build_static_logo_contains_all_lines() {
        let logo = build_static_logo(80);
        // Check that logo contains the A G R characters
        assert!(logo.contains("\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}"));
        // Check REC indicator is present
        assert!(logo.contains("\u{23FA} REC"));
    }

    #[test]
    fn build_static_logo_scales_dashes() {
        let logo_40 = build_static_logo(40);
        let logo_80 = build_static_logo(80);

        // Count dashes in each
        let dashes_40 = logo_40.matches('\u{2500}').count();
        let dashes_80 = logo_80.matches('\u{2500}').count();

        // 80-width should have more dashes than 40-width
        assert!(dashes_80 > dashes_40);
        // Specifically: 80 - 7 = 73 dashes, 40 - 7 = 33 dashes
        assert_eq!(dashes_40, 33);
        assert_eq!(dashes_80, 73);
    }

    #[test]
    fn build_static_logo_handles_small_width() {
        // Width smaller than REC prefix
        let logo = build_static_logo(5);
        // Should still contain REC indicator but no dashes
        assert!(logo.contains("\u{23FA} REC"));
        assert_eq!(logo.matches('\u{2500}').count(), 0);
    }

    #[test]
    fn logo_widget_can_be_created() {
        let logo = Logo::new();
        let theme = current_theme();
        // Ensure it uses the theme's accent style for logo
        assert!(logo.logo_style == theme.accent_style());
    }

    #[test]
    fn logo_widget_style_builders_work() {
        let logo = Logo::new()
            .logo_style(Style::default().fg(Color::Blue))
            .rec_style(Style::default().fg(Color::Green))
            .dash_style(Style::default().fg(Color::Yellow));

        assert_eq!(logo.logo_style.fg, Some(Color::Blue));
        assert_eq!(logo.rec_style.fg, Some(Color::Green));
        assert_eq!(logo.dash_style.fg, Some(Color::Yellow));
    }

    #[test]
    fn logo_widget_renders_without_panic() {
        let logo = Logo::new();
        let area = Rect::new(0, 0, 80, 12);
        let mut buf = Buffer::empty(area);
        logo.render(area, &mut buf);
        // If we get here without panic, rendering works
    }

    #[test]
    fn logo_widget_handles_zero_area() {
        let logo = Logo::new();
        // Zero height
        let area = Rect::new(0, 0, 80, 0);
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 1));
        logo.clone().render(area, &mut buf);

        // Zero width
        let area = Rect::new(0, 0, 0, 12);
        let mut buf = Buffer::empty(Rect::new(0, 0, 1, 12));
        logo.render(area, &mut buf);
        // No panic means success
    }
}
