//! Clap styling for the AGR CLI.

use clap::builder::styling::{AnsiColor, Effects, Styles};

/// Build clap styles using our theme colors.
///
/// Maps theme colors to clap's styling system for consistent CLI appearance.
/// - Green: headers, usage, command names (accent color)
/// - White: descriptions, placeholders (renders as light gray on dark terminals)
pub fn build_cli_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default() | Effects::BOLD)
        .usage(AnsiColor::Green.on_default() | Effects::BOLD)
        .literal(AnsiColor::Green.on_default())
        .placeholder(AnsiColor::White.on_default()) // Light gray for descriptions
        .valid(AnsiColor::White.on_default()) // Light gray for valid values
        .invalid(AnsiColor::Red.on_default())
        .error(AnsiColor::Red.on_default() | Effects::BOLD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_use_green_headers_and_red_errors() {
        let styles = build_cli_styles();
        assert_eq!(
            styles.get_header().get_fg_color(),
            Some(AnsiColor::Green.into())
        );
        assert_eq!(
            styles.get_error().get_fg_color(),
            Some(AnsiColor::Red.into())
        );
    }
}
