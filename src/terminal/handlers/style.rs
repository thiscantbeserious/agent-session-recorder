//! SGR (Select Graphic Rendition) handler.
//!
//! Handles CSI m sequences for text styling:
//! - Colors (foreground and background)
//! - Attributes (bold, dim, italic, underline, reverse)
//! - 256-color and RGB color support

use super::super::performer::TerminalPerformer;
use super::super::types::{CellStyle, Color};

impl TerminalPerformer<'_> {
    /// Handle SGR (Select Graphic Rendition) - CSI m.
    /// Parses parameters and updates current style.
    pub fn handle_sgr(&mut self, params: &[u16]) {
        let mut iter = params.iter().peekable();

        while let Some(&param) = iter.next() {
            match param {
                0 => *self.current_style = CellStyle::default(), // Reset
                1 => self.current_style.bold = true,
                2 => self.current_style.dim = true,
                3 => self.current_style.italic = true,
                4 => self.current_style.underline = true,
                7 => self.current_style.reverse = true,
                22 => {
                    self.current_style.bold = false;
                    self.current_style.dim = false;
                }
                23 => self.current_style.italic = false,
                24 => self.current_style.underline = false,
                27 => self.current_style.reverse = false,
                // Standard foreground colors (30-37)
                30 => self.current_style.fg = Color::Black,
                31 => self.current_style.fg = Color::Red,
                32 => self.current_style.fg = Color::Green,
                33 => self.current_style.fg = Color::Yellow,
                34 => self.current_style.fg = Color::Blue,
                35 => self.current_style.fg = Color::Magenta,
                36 => self.current_style.fg = Color::Cyan,
                37 => self.current_style.fg = Color::White,
                38 => {
                    // Extended foreground color
                    self.parse_extended_color(&mut iter, true);
                }
                39 => self.current_style.fg = Color::Default,
                // Standard background colors (40-47)
                40 => self.current_style.bg = Color::Black,
                41 => self.current_style.bg = Color::Red,
                42 => self.current_style.bg = Color::Green,
                43 => self.current_style.bg = Color::Yellow,
                44 => self.current_style.bg = Color::Blue,
                45 => self.current_style.bg = Color::Magenta,
                46 => self.current_style.bg = Color::Cyan,
                47 => self.current_style.bg = Color::White,
                48 => {
                    // Extended background color
                    self.parse_extended_color(&mut iter, false);
                }
                49 => self.current_style.bg = Color::Default,
                // Bright foreground colors (90-97)
                90 => self.current_style.fg = Color::BrightBlack,
                91 => self.current_style.fg = Color::BrightRed,
                92 => self.current_style.fg = Color::BrightGreen,
                93 => self.current_style.fg = Color::BrightYellow,
                94 => self.current_style.fg = Color::BrightBlue,
                95 => self.current_style.fg = Color::BrightMagenta,
                96 => self.current_style.fg = Color::BrightCyan,
                97 => self.current_style.fg = Color::BrightWhite,
                // Bright background colors (100-107)
                100 => self.current_style.bg = Color::BrightBlack,
                101 => self.current_style.bg = Color::BrightRed,
                102 => self.current_style.bg = Color::BrightGreen,
                103 => self.current_style.bg = Color::BrightYellow,
                104 => self.current_style.bg = Color::BrightBlue,
                105 => self.current_style.bg = Color::BrightMagenta,
                106 => self.current_style.bg = Color::BrightCyan,
                107 => self.current_style.bg = Color::BrightWhite,
                _ => {}
            }
        }
    }

    /// Parse extended color (256-color or RGB) from SGR parameters.
    fn parse_extended_color(
        &mut self,
        iter: &mut std::iter::Peekable<std::slice::Iter<'_, u16>>,
        is_foreground: bool,
    ) {
        if let Some(&&mode) = iter.peek() {
            iter.next();
            match mode {
                5 => {
                    // 256-color mode
                    if let Some(&&idx) = iter.peek() {
                        iter.next();
                        let color = Color::Indexed(idx as u8);
                        if is_foreground {
                            self.current_style.fg = color;
                        } else {
                            self.current_style.bg = color;
                        }
                    }
                }
                2 => {
                    // RGB mode
                    let r = iter.next().copied().unwrap_or(0) as u8;
                    let g = iter.next().copied().unwrap_or(0) as u8;
                    let b = iter.next().copied().unwrap_or(0) as u8;
                    let color = Color::Rgb(r, g, b);
                    if is_foreground {
                        self.current_style.fg = color;
                    } else {
                        self.current_style.bg = color;
                    }
                }
                _ => {}
            }
        }
    }
}
