//! Template parsing and rendering for filename generation.
//!
//! Supports tags like `{directory}`, `{date}`, `{time}` with optional format specifiers.

use super::filename::{sanitize_branch, sanitize_directory, Config};

/// Errors that can occur during template parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// Template string is empty.
    Empty,
    /// Unclosed brace in template.
    UnclosedBrace,
    /// Unmatched closing brace in template.
    UnmatchedCloseBrace,
    /// Unknown tag name.
    UnknownTag(String),
    /// Invalid format string.
    InvalidFormat(String),
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateError::Empty => write!(f, "Template cannot be empty"),
            TemplateError::UnclosedBrace => write!(f, "Unclosed brace in template"),
            TemplateError::UnmatchedCloseBrace => {
                write!(f, "Unmatched closing brace in template")
            }
            TemplateError::UnknownTag(tag) => write!(f, "Unknown template tag: {}", tag),
            TemplateError::InvalidFormat(fmt) => {
                write!(f, "Invalid format string: {}", fmt)
            }
        }
    }
}

impl std::error::Error for TemplateError {}

/// A segment of a parsed template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// Literal text to include as-is.
    Literal(String),
    /// Directory name tag.
    Directory,
    /// Date tag with format string.
    Date(String),
    /// Time tag with format string.
    Time(String),
    /// Git branch name tag.
    Branch,
    /// Base36 epoch timestamp tag.
    Id,
}

/// Default date format for {date} tag.
const DEFAULT_DATE_FORMAT: &str = "%y%m%d";

/// Default time format for {time} tag.
const DEFAULT_TIME_FORMAT: &str = "%H%M%S";

/// Default template string.
pub const DEFAULT_TEMPLATE: &str = "{directory}_{date}_{time}";

/// Context passed to `Template::render()` with all dynamic values.
#[derive(Debug, Clone)]
pub struct RenderContext<'a> {
    /// The directory name to use for `{directory}` tags.
    pub directory: &'a str,
    /// The git branch name, or None if not in a git repo / detached HEAD.
    pub branch: Option<&'a str>,
}

/// A parsed filename template.
#[derive(Debug, Clone)]
pub struct Template {
    segments: Vec<Segment>,
}

impl Default for Template {
    fn default() -> Self {
        Self::parse(DEFAULT_TEMPLATE).expect("Default template should be valid")
    }
}

impl Template {
    /// Parses a template string into segments.
    pub fn parse(template: &str) -> Result<Self, TemplateError> {
        if template.is_empty() {
            return Err(TemplateError::Empty);
        }

        let mut segments = Vec::new();
        let mut chars = template.chars().peekable();
        let mut literal = String::new();

        while let Some(c) = chars.next() {
            if c == '{' {
                push_literal(&mut segments, &mut literal);
                let segment = parse_brace_content(&mut chars)?;
                segments.push(segment);
            } else if c == '}' {
                return Err(TemplateError::UnmatchedCloseBrace);
            } else {
                literal.push(c);
            }
        }

        if !literal.is_empty() {
            segments.push(Segment::Literal(literal));
        }

        Ok(Self { segments })
    }

    /// Returns the parsed segments.
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// Renders the template with the given context and config.
    pub fn render(&self, ctx: &RenderContext<'_>, config: &Config) -> String {
        use chrono::Local;

        let now = Local::now();
        let mut result = String::new();

        for segment in &self.segments {
            match segment {
                Segment::Literal(s) => result.push_str(s),
                Segment::Directory => {
                    let sanitized = sanitize_directory(ctx.directory, config);
                    result.push_str(&sanitized);
                }
                Segment::Date(fmt) => {
                    result.push_str(&now.format(fmt).to_string());
                }
                Segment::Time(fmt) => {
                    result.push_str(&now.format(fmt).to_string());
                }
                Segment::Branch => {
                    if let Some(branch) = ctx.branch {
                        result.push_str(&sanitize_branch(branch));
                    }
                }
                Segment::Id => {
                    result.push_str(&epoch_base36());
                }
            }
        }

        result
    }
}

/// Saves accumulated literal text as a segment and clears the buffer.
fn push_literal(segments: &mut Vec<Segment>, literal: &mut String) {
    if !literal.is_empty() {
        segments.push(Segment::Literal(literal.clone()));
        literal.clear();
    }
}

/// Parses content between `{` and `}` into a Segment.
fn parse_brace_content(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
) -> Result<Segment, TemplateError> {
    let mut tag_content = String::new();
    let mut found_close = false;

    for tc in chars.by_ref() {
        if tc == '}' {
            found_close = true;
            break;
        }
        if tc == '{' {
            return Err(TemplateError::UnclosedBrace);
        }
        tag_content.push(tc);
    }

    if !found_close {
        return Err(TemplateError::UnclosedBrace);
    }

    parse_tag(&tag_content)
}

/// Parses a tag content string (without braces) into a Segment.
fn parse_tag(content: &str) -> Result<Segment, TemplateError> {
    let (tag_name, format) = split_tag_format(content);

    match tag_name {
        "directory" => {
            reject_format(format, "directory")?;
            Ok(Segment::Directory)
        }
        "date" => parse_datetime_tag(format, DEFAULT_DATE_FORMAT, "date"),
        "time" => parse_datetime_tag(format, DEFAULT_TIME_FORMAT, "time"),
        "branch" => {
            reject_format(format, "branch")?;
            Ok(Segment::Branch)
        }
        "id" => {
            reject_format(format, "id")?;
            Ok(Segment::Id)
        }
        _ => Err(TemplateError::UnknownTag(tag_name.to_string())),
    }
}

/// Rejects a format specifier for tags that don't accept one.
fn reject_format(format: Option<&str>, tag_name: &str) -> Result<(), TemplateError> {
    if format.is_some() {
        return Err(TemplateError::InvalidFormat(format!(
            "{} tag does not accept format",
            tag_name
        )));
    }
    Ok(())
}

/// Splits tag content on the first colon into (name, optional_format).
fn split_tag_format(content: &str) -> (&str, Option<&str>) {
    match content.find(':') {
        Some(pos) => {
            let (name, fmt) = content.split_at(pos);
            (name, Some(&fmt[1..]))
        }
        None => (content, None),
    }
}

/// Parses a date or time tag with an optional format specifier.
fn parse_datetime_tag(
    format: Option<&str>,
    default_fmt: &str,
    tag_name: &str,
) -> Result<Segment, TemplateError> {
    let fmt = format.unwrap_or(default_fmt);
    if fmt.is_empty() {
        return Err(TemplateError::InvalidFormat(format!(
            "{} format cannot be empty",
            tag_name
        )));
    }
    validate_strftime_format(fmt)?;
    match tag_name {
        "date" => Ok(Segment::Date(fmt.to_string())),
        "time" => Ok(Segment::Time(fmt.to_string())),
        _ => unreachable!(),
    }
}

/// Validates a strftime format string contains at least one valid specifier.
fn validate_strftime_format(fmt: &str) -> Result<(), TemplateError> {
    const VALID_SPECIFIERS: &[char] = &[
        'Y', 'y', 'm', 'd', 'H', 'M', 'S', 'f', 'j', 'U', 'W', 'w', 'a', 'A', 'b', 'B', 'C', 'e',
        'G', 'g', 'I', 'k', 'l', 'n', 'P', 'p', 'r', 'R', 'T', 's', 't', 'u', 'V', 'z', 'Z', '+',
        '%',
    ];

    let mut found_specifier = false;
    let mut chars = fmt.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            if let Some(&next) = chars.peek() {
                if VALID_SPECIFIERS.contains(&next) {
                    found_specifier = true;
                    chars.next();
                }
            }
        }
    }

    if !found_specifier {
        return Err(TemplateError::InvalidFormat(format!(
            "format string '{}' contains no valid strftime specifiers",
            fmt
        )));
    }

    Ok(())
}

/// Width of the zero-padded base36 output.
const BASE36_WIDTH: usize = 7;

/// Base36 digit set (0-9, a-z).
const BASE36_DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// Encodes a u64 value as a zero-padded 7-character base36 string.
pub fn encode_base36(mut value: u64) -> String {
    let mut buf = [b'0'; BASE36_WIDTH];
    let mut i = BASE36_WIDTH;

    if value == 0 {
        return String::from_utf8(buf.to_vec()).unwrap();
    }

    while value > 0 && i > 0 {
        i -= 1;
        buf[i] = BASE36_DIGITS[(value % 36) as usize];
        value /= 36;
    }

    String::from_utf8(buf.to_vec()).unwrap()
}

/// Returns the current epoch timestamp as a 7-character base36 string.
fn epoch_base36() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    encode_base36(secs)
}
