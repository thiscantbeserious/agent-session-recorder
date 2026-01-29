//! Terminal data types.
//!
//! Contains the core data structures for representing terminal state:
//! - Color: ANSI color codes (16 colors, 256-color palette, RGB)
//! - CellStyle: Text attributes (bold, italic, underline, etc.)
//! - Cell: A single character with its style
//! - StyledLine: A line of styled cells for rendering

/// ANSI color codes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    /// 256-color palette index
    Indexed(u8),
    /// RGB color
    Rgb(u8, u8, u8),
}

/// Style attributes for a terminal cell
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellStyle {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
}

/// A single cell in the terminal buffer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub char: char,
    pub style: CellStyle,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            char: ' ',
            style: CellStyle::default(),
        }
    }
}

/// A styled line for rendering
#[derive(Debug, Clone)]
pub struct StyledLine {
    pub cells: Vec<Cell>,
}
