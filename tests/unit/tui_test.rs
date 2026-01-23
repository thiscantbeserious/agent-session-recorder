//! Visual snapshot tests for TUI components
//!
//! Uses insta for snapshot testing to ensure visual output is correct.

use agr::tui::theme::current_theme;
use agr::tui::widgets::Logo;
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

/// Render a widget to a string for snapshot testing.
fn render_to_string<W: Widget>(widget: W, width: u16, height: u16) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    widget.render(area, &mut buf);

    // Convert buffer to string representation with ANSI colors
    let mut output = String::new();
    for y in 0..height {
        for x in 0..width {
            let cell = &buf[(x, y)];
            // Include fg color info in a readable format
            let symbol = cell.symbol();
            output.push_str(symbol);
        }
        output.push('\n');
    }
    output
}

/// Render a widget to a detailed string with color information.
fn render_with_colors<W: Widget>(widget: W, width: u16, height: u16) -> String {
    use ratatui::style::Color;

    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    widget.render(area, &mut buf);

    let mut output = String::new();

    // Header with dimensions
    output.push_str(&format!("=== {}x{} ===\n", width, height));

    // Track unique colors used
    let mut colors_used: Vec<(String, Color)> = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let cell = &buf[(x, y)];
            if let Some(fg) = cell.style().fg {
                if !colors_used.iter().any(|(_, c)| *c == fg) {
                    let label = match fg {
                        Color::Rgb(r, g, b) => format!("RGB({},{},{})", r, g, b),
                        other => format!("{:?}", other),
                    };
                    colors_used.push((label, fg));
                }
            }
            output.push_str(cell.symbol());
        }
        output.push('\n');
    }

    // Color summary
    if !colors_used.is_empty() {
        output.push_str("\n--- Colors Used ---\n");
        for (label, _) in colors_used {
            output.push_str(&format!("  {}\n", label));
        }
    }

    output
}

#[test]
fn snapshot_logo_visual() {
    let logo = Logo::new();
    let output = render_with_colors(logo, 80, 12);
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_theme_colors() {
    let theme = current_theme();
    let snapshot = format!(
        "Theme: claude_code\n\
         text_primary: {:?}\n\
         text_secondary: {:?}\n\
         accent: {:?}\n\
         error: {:?}\n\
         success: {:?}\n\
         background: {:?}",
        theme.text_primary,
        theme.text_secondary,
        theme.accent,
        theme.error,
        theme.success,
        theme.background
    );
    insta::assert_snapshot!(snapshot);
}

#[test]
fn snapshot_logo_rec_line_scales() {
    // Test that the REC line scales to different widths
    let logo_40 = Logo::new();
    let logo_80 = Logo::new();
    let logo_120 = Logo::new();

    let out_40 = render_to_string(logo_40, 40, 12);
    let out_80 = render_to_string(logo_80, 80, 12);
    let out_120 = render_to_string(logo_120, 120, 12);

    let snapshot = format!(
        "=== Width 40 ===\n{}\n\
         === Width 80 ===\n{}\n\
         === Width 120 ===\n{}",
        out_40, out_80, out_120
    );
    insta::assert_snapshot!(snapshot);
}

#[test]
fn logo_uses_accent_color_for_logo_text() {
    use ratatui::style::Color;

    let logo = Logo::new();

    let area = Rect::new(0, 0, 80, 12);
    let mut buf = Buffer::empty(area);
    logo.render(area, &mut buf);

    // Check a character from the logo (row 2, should be part of 'A')
    // The logo starts after 2 blank lines, so row 2 is the first logo line
    let logo_cell = &buf[(0, 2)]; // First char of first logo line

    // Logo should use accent color (bright green)
    assert_eq!(
        logo_cell.style().fg,
        Some(Color::LightGreen),
        "Logo should use accent color (bright green)"
    );
}

#[test]
fn logo_uses_secondary_color_for_dashes() {
    use ratatui::style::Color;

    let logo = Logo::new();

    let area = Rect::new(0, 0, 80, 12);
    let mut buf = Buffer::empty(area);
    logo.render(area, &mut buf);

    // REC line is at row 8 (2 blank + 6 logo lines)
    // The dashes start after "⏺ REC " (7 chars)
    let dash_cell = &buf[(10, 8)]; // A dash character

    // Dashes should use text_secondary color (dark gray)
    assert_eq!(
        dash_cell.style().fg,
        Some(Color::DarkGray),
        "Dashes should use text_secondary color (dark gray)"
    );
}

#[test]
fn snapshot_cli_ansi_theme_colors() {
    // Snapshot the ANSI color codes used by the theme for CLI output
    let theme = current_theme();

    let snapshot = format!(
        "CLI ANSI Theme Colors\n\
         =====================\n\
         accent_text(\"test\"): {:?}\n\
         primary_text(\"test\"): {:?}\n\
         secondary_text(\"test\"): {:?}\n\
         error_text(\"test\"): {:?}\n\
         success_text(\"test\"): {:?}",
        theme.accent_text("test"),
        theme.primary_text("test"),
        theme.secondary_text("test"),
        theme.error_text("test"),
        theme.success_text("test"),
    );
    insta::assert_snapshot!(snapshot);
}

// ============================================================================
// Branding Assets Snapshots
// ============================================================================

#[test]
fn snapshot_branding_logo_start() {
    // Small logo shown when starting a recording session
    let logo = agr::branding::LOGO_START;
    insta::assert_snapshot!("branding_logo_start", logo);
}

#[test]
fn snapshot_branding_logo_done() {
    // Small logo shown when a recording session ends
    let logo = agr::branding::LOGO_DONE;
    insta::assert_snapshot!("branding_logo_done", logo);
}

#[test]
fn snapshot_branding_logo_full() {
    // Full ASCII logo for interactive CLI mode
    let logo = agr::branding::LOGO_FULL;
    insta::assert_snapshot!("branding_logo_full", logo);
}

#[test]
fn snapshot_branding_box_bottom() {
    // Bottom border of the box
    let border = agr::branding::BOX_BOTTOM;
    insta::assert_snapshot!("branding_box_bottom", border);
}

// ============================================================================
// Colorize Help Function Snapshots
// ============================================================================

#[test]
fn snapshot_colorize_help_commands() {
    use agr::tui::colorize_help;

    // Test colorize_help with typical command list output
    let input = r#"
Commands:
  record   Start recording a session
  status   Show storage statistics
  cleanup  Interactive cleanup of old sessions
  list     List recorded sessions [aliases: ls]
  analyze  Analyze a recording with AI
"#;

    let output = colorize_help(input);
    insta::assert_snapshot!("colorize_help_commands", output);
}

#[test]
fn snapshot_colorize_help_logo() {
    use agr::tui::colorize_help;

    // Test colorize_help with logo (should apply green to logo lines)
    let input = r#" █████╗  ██████╗ ██████╗
██╔══██╗██╔════╝ ██╔══██╗
███████║██║  ███╗██████╔╝
██╔══██║██║   ██║██╔══██╗
██║  ██║╚██████╔╝██║  ██║
╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝
 ⏺ REC ─────────────────────────────────────────────────────────────────────────
"#;

    let output = colorize_help(input);
    insta::assert_snapshot!("colorize_help_logo", output);
}

#[test]
fn snapshot_colorize_help_mixed() {
    use agr::tui::colorize_help;

    // Test colorize_help with mixed content (logo + commands)
    let input = r#" █████╗  ██████╗ ██████╗
██╔══██╗██╔════╝ ██╔══██╗
███████║██║  ███╗██████╔╝
██╔══██║██║   ██║██╔══██╗
██║  ██║╚██████╔╝██║  ██║
╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝
 ⏺ REC ─────────────────────────────────────────────────────────────────────────


[ Agent Session Recorder ] - auto-record agent sessions!

Usage: agr <COMMAND>

Commands:
  record   Start recording a session
  status   Show storage statistics
"#;

    let output = colorize_help(input);
    insta::assert_snapshot!("colorize_help_mixed", output);
}

// ============================================================================
// Static Logo Builder Snapshots (Fixed Width)
// ============================================================================

#[test]
fn snapshot_static_logo_width_40() {
    use agr::tui::widgets::logo::build_static_logo;
    let output = build_static_logo(40);
    insta::assert_snapshot!("static_logo_width_40", output);
}

#[test]
fn snapshot_static_logo_width_80() {
    use agr::tui::widgets::logo::build_static_logo;
    let output = build_static_logo(80);
    insta::assert_snapshot!("static_logo_width_80", output);
}

#[test]
fn snapshot_static_logo_width_120() {
    use agr::tui::widgets::logo::build_static_logo;
    let output = build_static_logo(120);
    insta::assert_snapshot!("static_logo_width_120", output);
}

// ============================================================================
// Theme Text Wrapper Snapshots
// ============================================================================

#[test]
fn snapshot_theme_text_wrappers() {
    let theme = current_theme();

    let snapshot = format!(
        "Theme Text Wrappers (Raw ANSI codes)\n\
         ====================================\n\
         \n\
         accent_text(\"Hello World\"):\n{}\n\
         \n\
         primary_text(\"Hello World\"):\n{}\n\
         \n\
         secondary_text(\"Hello World\"):\n{}\n\
         \n\
         error_text(\"Hello World\"):\n{}\n\
         \n\
         success_text(\"Hello World\"):\n{}",
        theme.accent_text("Hello World"),
        theme.primary_text("Hello World"),
        theme.secondary_text("Hello World"),
        theme.error_text("Hello World"),
        theme.success_text("Hello World"),
    );
    insta::assert_snapshot!(snapshot);
}
