//! UI rendering helpers for TUI
//!
//! Common UI utilities and layout helpers.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

use super::current_theme;
use super::widgets::Logo;

/// Render the logo centered at the top of the frame.
pub fn render_logo(frame: &mut Frame) {
    let area = frame.area();
    let logo = Logo::new();
    frame.render_widget(logo, area);
}

/// Render the full help screen with logo at top and scrollable help content.
pub fn render_help(frame: &mut Frame, help_text: &str, scroll_offset: u16) {
    let theme = current_theme();
    let area = frame.area();

    // Split into logo area (top) and help content (bottom)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(Logo::height()), Constraint::Min(1)])
        .split(area);

    // Render logo at top
    let logo = Logo::new();
    frame.render_widget(logo, chunks[0]);

    // Render help content with scroll
    let help_paragraph = Paragraph::new(help_text)
        .style(theme.text_style())
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset, 0));

    frame.render_widget(help_paragraph, chunks[1]);

    // Render footer with instructions
    let footer_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(" q", theme.accent_bold_style()),
        Span::styled(" quit  ", theme.text_secondary_style()),
        Span::styled("↑/↓", theme.accent_bold_style()),
        Span::styled(" scroll  ", theme.text_secondary_style()),
        Span::styled("PgUp/PgDn", theme.accent_bold_style()),
        Span::styled(" page", theme.text_secondary_style()),
    ]));
    frame.render_widget(footer, footer_area);
}

/// Create a centered layout with the given constraints.
///
/// Returns the center area that can be used for content.
/// Percentages are clamped to 0-100 to prevent underflow.
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    // Clamp percentages to valid range to prevent underflow
    let percent_x = percent_x.min(100);
    let percent_y = percent_y.min(100);

    let vertical_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_rect_creates_smaller_area() {
        let area = Rect::new(0, 0, 100, 100);
        let centered = centered_rect(50, 50, area);

        // Centered area should be roughly 50% of original
        assert!(centered.width <= 55); // Allow some rounding
        assert!(centered.height <= 55);
    }

    #[test]
    fn centered_rect_is_centered() {
        let area = Rect::new(0, 0, 100, 100);
        let centered = centered_rect(50, 50, area);

        // Should be roughly centered
        assert!(centered.x >= 20 && centered.x <= 30);
        assert!(centered.y >= 20 && centered.y <= 30);
    }

    #[test]
    fn centered_rect_clamps_percent_over_100() {
        let area = Rect::new(0, 0, 100, 100);
        // Should not panic with percent > 100, and should behave like 100%
        let centered = centered_rect(150, 200, area);

        // With 100% for both, the centered rect should fill the area
        assert_eq!(centered.width, area.width);
        assert_eq!(centered.height, area.height);
    }
}
