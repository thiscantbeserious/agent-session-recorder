//! Filename generation and sanitization for recordings.
//!
//! Provides configurable filename templates with tags like `{directory}`, `{date}`, `{time}`,
//! and comprehensive sanitization to ensure filesystem-safe names.

use deunicode::deunicode;

/// Minimum allowed value for directory_max_length.
const MIN_DIRECTORY_MAX_LENGTH: usize = 1;

/// Configuration for filename generation.
#[derive(Debug, Clone)]
pub struct Config {
    /// Maximum length for the directory component (default: 50, minimum: 1).
    pub directory_max_length: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            directory_max_length: 50,
        }
    }
}

impl Config {
    /// Creates a new Config, ensuring directory_max_length is at least 1.
    pub fn new(directory_max_length: usize) -> Self {
        Self {
            directory_max_length: directory_max_length.max(MIN_DIRECTORY_MAX_LENGTH),
        }
    }
}

/// Windows reserved device names that cannot be used as filenames.
const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Characters that are invalid in filenames on common filesystems.
const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Default fallback name when sanitization produces an empty result.
const FALLBACK_NAME: &str = "recording";

/// Maximum filename length for most filesystems.
const MAX_FILENAME_LENGTH: usize = 255;

/// Sanitizes a string for use in filenames.
///
/// Applies the following transformations in order:
/// 1. Unicode → ASCII transliteration
/// 2. Whitespace → hyphens
/// 3. Invalid filesystem characters removed
/// 4. Multiple hyphens collapsed to single
/// 5. Leading/trailing dots, spaces, hyphens trimmed
/// 6. Windows reserved names prefixed with `_`
/// 7. Empty results → "recording" fallback
#[allow(dead_code)]
pub fn sanitize(input: &str, _config: &Config) -> String {
    // Step 1: Unicode transliteration
    let ascii = deunicode(input);

    // Step 2 & 3: Process characters
    let mut result = String::with_capacity(ascii.len());
    let mut last_was_hyphen = false;

    for c in ascii.chars() {
        if c.is_whitespace() {
            // Whitespace → hyphen (collapse multiple)
            if !last_was_hyphen {
                result.push('-');
                last_was_hyphen = true;
            }
        } else if INVALID_CHARS.contains(&c) {
            // Invalid chars → removed
            continue;
        } else if c == '-' {
            // Collapse multiple hyphens
            if !last_was_hyphen {
                result.push('-');
                last_was_hyphen = true;
            }
        } else if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
            // Valid chars preserved
            result.push(c);
            last_was_hyphen = false;
        } else if c == '(' || c == ')' || c == '[' || c == ']' {
            // Common brackets → removed (they become empty after deunicode)
            continue;
        }
        // Other non-ASCII chars that survived deunicode are dropped
    }

    // Step 4: Trim leading/trailing dots, spaces, hyphens
    let trimmed = trim_edges(&result);

    // Step 5: Check for Windows reserved names
    let final_name = handle_reserved_name(&trimmed);

    // Step 6: Fallback for empty result
    if final_name.is_empty() {
        FALLBACK_NAME.to_string()
    } else {
        final_name
    }
}

/// Sanitizes a directory name with length truncation.
///
/// Same as `sanitize()` but also truncates to `config.directory_max_length`.
#[allow(dead_code)]
pub fn sanitize_directory(input: &str, config: &Config) -> String {
    let sanitized = sanitize(input, config);
    truncate_to_length(&sanitized, config.directory_max_length)
}

/// Validates that a final filename doesn't exceed filesystem limits.
///
/// Returns an error if the filename exceeds 255 characters.
#[allow(dead_code)]
pub fn validate_length(filename: &str) -> Result<(), FilenameError> {
    if filename.len() > MAX_FILENAME_LENGTH {
        Err(FilenameError::TooLong {
            length: filename.len(),
            max: MAX_FILENAME_LENGTH,
        })
    } else {
        Ok(())
    }
}

/// Generates a filename from a template and directory name.
///
/// This is the main entry point for filename generation. It:
/// 1. Parses the template
/// 2. Renders it with the directory and current datetime
/// 3. Adds `.cast` extension
/// 4. Validates the final length
#[allow(dead_code)]
pub fn generate(directory: &str, template: &str, config: &Config) -> Result<String, GenerateError> {
    let parsed = Template::parse(template)?;
    let rendered = parsed.render(directory, config);

    // Add .cast extension if not present
    let filename = if rendered.ends_with(".cast") {
        rendered
    } else {
        format!("{}.cast", rendered)
    };

    // Validate final length
    validate_length(&filename).map_err(GenerateError::from)?;

    Ok(filename)
}

/// Errors that can occur during filename generation.
#[derive(Debug)]
pub enum GenerateError {
    /// Template parsing error.
    Template(TemplateError),
    /// Filename validation error.
    Filename(FilenameError),
}

impl std::fmt::Display for GenerateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenerateError::Template(e) => write!(f, "Template error: {}", e),
            GenerateError::Filename(e) => write!(f, "Filename error: {}", e),
        }
    }
}

impl std::error::Error for GenerateError {}

impl From<TemplateError> for GenerateError {
    fn from(e: TemplateError) -> Self {
        GenerateError::Template(e)
    }
}

impl From<FilenameError> for GenerateError {
    fn from(e: FilenameError) -> Self {
        GenerateError::Filename(e)
    }
}

/// Trims leading and trailing dots, spaces, and hyphens.
fn trim_edges(s: &str) -> String {
    s.trim_matches(|c| c == '.' || c == ' ' || c == '-')
        .to_string()
}

/// Vowels used for syllable detection.
const VOWELS: &[char] = &['a', 'e', 'i', 'o', 'u', 'A', 'E', 'I', 'O', 'U'];

/// Extracts the first syllable of a word.
///
/// Algorithm:
/// 1. Find first vowel
/// 2. Include consonants after the first vowel until the next vowel or end
/// 3. When there are doubled consonants anywhere in the cluster, split at the double
/// 4. If no next vowel (single-syllable word), keep whole word
///
/// Short words (≤3 chars) are returned unchanged.
///
/// Examples:
/// - "testing" → "test" (t + e + s + t, stop before 'i')
/// - "hello" → "hel" (h + e + l, split doubled 'll')
/// - "session" → "ses" (s + e + s, split doubled 'ss')
/// - "recorder" → "rec" (r + e + c, stop before 'o')
/// - "cool" → "co" (c + o, stop at second 'o' which is a vowel)
/// - "world" → "world" (only one vowel, no more after)
/// - "three" → "three" (vowels at end, no consonants after)
/// - "five" → "fiv" (f + i + v, stop before 'e')
/// - "really" → "re" (r + e, stop at 'a' which is a vowel)
/// - "apple" → "ap" (a + p, split doubled 'pp')
fn first_syllable(word: &str) -> &str {
    // Short words stay unchanged (use char count for unicode safety)
    if word.chars().count() <= 3 {
        return word;
    }

    let chars: Vec<char> = word.chars().collect();

    // Find first vowel
    let first_vowel_idx = match chars.iter().position(|c| VOWELS.contains(c)) {
        Some(idx) => idx,
        None => return word, // No vowel, return whole word
    };

    // Start after the first vowel, collect consonants until next vowel
    let mut idx = first_vowel_idx + 1;
    let consonant_start = idx;

    while idx < chars.len() && !VOWELS.contains(&chars[idx]) {
        idx += 1;
    }

    // idx is now at the next vowel or end of word
    // If we reached the end of the word, this is a single-syllable word - keep it
    if idx >= chars.len() {
        return word;
    }

    // There's another vowel ahead - determine cut point
    let consonant_count = idx - consonant_start;
    let cut_idx = if consonant_count >= 2 {
        // Scan the consonant cluster for any doubled letters (not just at boundary)
        let consonants = &chars[consonant_start..idx];
        let mut double_pos = None;
        for i in 0..consonants.len() - 1 {
            if consonants[i] == consonants[i + 1] {
                // Found doubled consonants - split after first one
                double_pos = Some(consonant_start + i + 1);
                break;
            }
        }
        double_pos.unwrap_or(idx)
    } else {
        // 0 or 1 consonant: cut at the next vowel
        idx
    };

    // Convert char index to byte index
    let byte_idx = word
        .char_indices()
        .nth(cut_idx)
        .map(|(i, _)| i)
        .unwrap_or(word.len());
    &word[..byte_idx]
}

/// Truncates a string to the specified length using smart abbreviation.
///
/// For multi-word strings (separated by `-`, `_`, `.`), applies first syllable
/// extraction to each word when truncation is needed. If still too long after
/// abbreviation, truncates proportionally. Single words are hard-truncated.
fn truncate_to_length(s: &str, max_len: usize) -> String {
    // If it fits, return unchanged (use char count for unicode safety)
    if s.chars().count() <= max_len {
        return s.to_string();
    }

    // Split on word boundaries
    let words: Vec<&str> = s.split(['-', '_', '.']).filter(|w| !w.is_empty()).collect();

    // Single word: just hard truncate (char-based)
    if words.len() <= 1 {
        return s.chars().take(max_len).collect();
    }

    // Multiple words: apply first syllable abbreviation
    let abbreviated: Vec<&str> = words.iter().map(|w| first_syllable(w)).collect();
    let result = abbreviated.join("-");

    // If abbreviated result fits, return it (char-based check)
    if result.chars().count() <= max_len {
        return result;
    }

    // Further truncation needed - distribute chars evenly across words
    let separator_count = words.len() - 1;
    let available = max_len.saturating_sub(separator_count);
    let chars_per_word = available / words.len();

    let truncated: Vec<String> = abbreviated
        .iter()
        .map(|w| w.chars().take(chars_per_word.max(1)).collect::<String>())
        .collect();

    // Join and clean up any trailing hyphens
    let joined = truncated.join("-");
    let cleaned = joined.trim_end_matches('-').to_string();

    // Final safety: hard truncate if still over limit, then trim any trailing hyphen
    if cleaned.chars().count() > max_len {
        let truncated: String = cleaned.chars().take(max_len).collect();
        truncated.trim_end_matches('-').to_string()
    } else {
        cleaned
    }
}

/// Checks if a name is a Windows reserved name and prefixes it if so.
///
/// Handles both exact matches (CON) and names with extensions (CON.txt).
fn handle_reserved_name(name: &str) -> String {
    // Extract the base name (before any extension)
    let base_name = match name.find('.') {
        Some(pos) => &name[..pos],
        None => name,
    };

    let upper = base_name.to_uppercase();
    for reserved in WINDOWS_RESERVED {
        if upper == *reserved {
            return format!("_{}", name);
        }
    }
    name.to_string()
}

/// Errors that can occur during filename operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilenameError {
    /// Filename exceeds 255 character filesystem limit.
    TooLong { length: usize, max: usize },
}

impl std::fmt::Display for FilenameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilenameError::TooLong { length, max } => {
                write!(f, "Filename too long: {} characters (max {})", length, max)
            }
        }
    }
}

impl std::error::Error for FilenameError {}

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
            TemplateError::UnmatchedCloseBrace => write!(f, "Unmatched closing brace in template"),
            TemplateError::UnknownTag(tag) => write!(f, "Unknown template tag: {}", tag),
            TemplateError::InvalidFormat(fmt) => write!(f, "Invalid format string: {}", fmt),
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
}

/// Default date format for {date} tag.
const DEFAULT_DATE_FORMAT: &str = "%y%m%d";

/// Default time format for {time} tag.
const DEFAULT_TIME_FORMAT: &str = "%H%M";

/// Default template string.
const DEFAULT_TEMPLATE: &str = "{directory}_{date}_{time}";

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
                // Save any accumulated literal
                if !literal.is_empty() {
                    segments.push(Segment::Literal(literal.clone()));
                    literal.clear();
                }

                // Parse the tag
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

                // Parse the tag content
                let segment = parse_tag(&tag_content)?;
                segments.push(segment);
            } else if c == '}' {
                // Unmatched closing brace
                return Err(TemplateError::UnmatchedCloseBrace);
            } else {
                literal.push(c);
            }
        }

        // Save any remaining literal
        if !literal.is_empty() {
            segments.push(Segment::Literal(literal));
        }

        Ok(Self { segments })
    }

    /// Returns the parsed segments.
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// Renders the template with the given directory name and config.
    pub fn render(&self, directory: &str, config: &Config) -> String {
        use chrono::Local;

        let now = Local::now();
        let mut result = String::new();

        for segment in &self.segments {
            match segment {
                Segment::Literal(s) => result.push_str(s),
                Segment::Directory => {
                    let sanitized = sanitize_directory(directory, config);
                    result.push_str(&sanitized);
                }
                Segment::Date(fmt) => {
                    let formatted = now.format(fmt).to_string();
                    result.push_str(&formatted);
                }
                Segment::Time(fmt) => {
                    let formatted = now.format(fmt).to_string();
                    result.push_str(&formatted);
                }
            }
        }

        result
    }
}

/// Parses a tag content string (without braces) into a Segment.
fn parse_tag(content: &str) -> Result<Segment, TemplateError> {
    // Split on first colon for format string
    let (tag_name, format) = match content.find(':') {
        Some(pos) => {
            let (name, fmt) = content.split_at(pos);
            (name, Some(&fmt[1..])) // Skip the colon
        }
        None => (content, None),
    };

    match tag_name {
        "directory" => {
            if format.is_some() {
                return Err(TemplateError::InvalidFormat(
                    "directory tag does not accept format".to_string(),
                ));
            }
            Ok(Segment::Directory)
        }
        "date" => {
            let fmt = format.unwrap_or(DEFAULT_DATE_FORMAT);
            if fmt.is_empty() {
                return Err(TemplateError::InvalidFormat(
                    "date format cannot be empty".to_string(),
                ));
            }
            validate_strftime_format(fmt)?;
            Ok(Segment::Date(fmt.to_string()))
        }
        "time" => {
            let fmt = format.unwrap_or(DEFAULT_TIME_FORMAT);
            if fmt.is_empty() {
                return Err(TemplateError::InvalidFormat(
                    "time format cannot be empty".to_string(),
                ));
            }
            validate_strftime_format(fmt)?;
            Ok(Segment::Time(fmt.to_string()))
        }
        _ => Err(TemplateError::UnknownTag(tag_name.to_string())),
    }
}

/// Validates a strftime format string by checking it contains at least one valid specifier.
fn validate_strftime_format(fmt: &str) -> Result<(), TemplateError> {
    // Valid strftime specifiers (common ones)
    const VALID_SPECIFIERS: &[char] = &[
        'Y', 'y', 'm', 'd', 'H', 'M', 'S', 'f', 'j', 'U', 'W', 'w', 'a', 'A', 'b', 'B', 'C', 'e',
        'G', 'g', 'I', 'k', 'l', 'n', 'P', 'p', 'r', 'R', 'T', 's', 't', 'u', 'V', 'z', 'Z', '+',
        '%',
    ];

    // Check if format contains at least one % followed by a valid specifier
    let mut found_specifier = false;
    let mut chars = fmt.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            if let Some(&next) = chars.peek() {
                if VALID_SPECIFIERS.contains(&next) {
                    found_specifier = true;
                    chars.next(); // consume the specifier
                }
                // Invalid specifier after % - we'll let chrono handle it (passes through literally)
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
