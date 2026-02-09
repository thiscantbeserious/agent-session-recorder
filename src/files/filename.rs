//! Filename generation and sanitization for recordings.
//!
//! Provides sanitization, truncation, and the top-level `generate()` entry point.
//! Template parsing and rendering live in `super::template`.

use deunicode::deunicode;

// Re-export template types so existing consumers (tests, recording.rs) keep working.
pub use super::template::encode_base36;
pub use super::template::{RenderContext, Segment, Template, TemplateError};

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
pub const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Default fallback name when sanitization produces an empty result.
const FALLBACK_NAME: &str = "recording";

/// Maximum filename length for most filesystems.
pub const MAX_FILENAME_LENGTH: usize = 255;

/// Check whether a character is valid for use in filenames.
pub fn is_valid_filename_char(c: char) -> bool {
    !INVALID_CHARS.contains(&c) && !c.is_control()
}

/// Sanitizes a string for use in filenames.
///
/// Applies unicode transliteration, whitespace-to-hyphens, invalid char removal,
/// hyphen collapsing, edge trimming, reserved name handling, and empty fallback.
#[allow(dead_code)]
pub fn sanitize(input: &str, _config: &Config) -> String {
    let ascii = deunicode(input);
    let processed = process_chars(&ascii);
    let trimmed = trim_edges(&processed);
    let final_name = handle_reserved_name(&trimmed);

    if final_name.is_empty() {
        FALLBACK_NAME.to_string()
    } else {
        final_name
    }
}

/// Sanitizes a branch name for use in filenames.
///
/// Replaces `/` with `@` to preserve namespace visibility, then removes
/// only filesystem-invalid characters (`INVALID_CHARS`) and control chars.
/// More permissive than `sanitize()` — keeps `@`, `#`, `~`, etc. which
/// are valid on all major filesystems. Does NOT apply length truncation.
#[allow(dead_code)]
pub fn sanitize_branch(input: &str) -> String {
    input
        .chars()
        .filter_map(|c| {
            if c == '/' {
                Some('@')
            } else if INVALID_CHARS.contains(&c) || c.is_control() {
                None
            } else {
                Some(c)
            }
        })
        .collect()
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

/// Generates a filename from a template, context, and config.
///
/// Parses the template, renders it, appends `.cast`, and validates length.
#[allow(dead_code)]
pub fn generate(
    ctx: &RenderContext<'_>,
    template: &str,
    config: &Config,
) -> Result<String, GenerateError> {
    let parsed = Template::parse(template)?;
    let rendered = parsed.render(ctx, config);

    let filename = if rendered.ends_with(".cast") {
        rendered
    } else {
        format!("{}.cast", rendered)
    };

    validate_length(&filename).map_err(GenerateError::from)?;
    Ok(filename)
}

/// Resolves same-second filename collisions by appending a suffix (`a`..`z`).
///
/// If `dir/filename.cast` already exists, tries `dir/filenamea.cast` through
/// `dir/filenamez.cast`. Returns `None` if all 26 suffixes are taken.
pub fn resolve_collision(dir: &std::path::Path, filename: &str) -> Option<String> {
    let path = dir.join(filename);
    if !path.exists() {
        return Some(filename.to_string());
    }

    let (stem, ext) = split_stem_extension(filename);

    for suffix in b'a'..=b'z' {
        let candidate = if ext.is_empty() {
            format!("{}{}", stem, suffix as char)
        } else {
            format!("{}{}.{}", stem, suffix as char, ext)
        };
        if !dir.join(&candidate).exists() {
            return Some(candidate);
        }
    }

    None
}

/// Splits a filename into stem and extension.
fn split_stem_extension(filename: &str) -> (&str, &str) {
    match filename.rfind('.') {
        Some(pos) => (&filename[..pos], &filename[pos + 1..]),
        None => (filename, ""),
    }
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

/// Errors that can occur during file rename.
#[derive(Debug)]
pub enum RenameError {
    /// New name is empty.
    EmptyName,
    /// New name contains invalid characters.
    InvalidChars,
    /// New name is a Windows reserved name.
    ReservedName,
    /// New filename exceeds filesystem length limit.
    TooLong,
    /// A file with the new name already exists.
    AlreadyExists(String),
    /// Filesystem I/O error.
    IoError(std::io::Error),
}

impl std::fmt::Display for RenameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenameError::EmptyName => write!(f, "Name cannot be empty"),
            RenameError::InvalidChars => write!(f, "Name contains invalid characters"),
            RenameError::ReservedName => write!(f, "Name is a reserved system name"),
            RenameError::TooLong => write!(f, "Name is too long"),
            RenameError::AlreadyExists(name) => write!(f, "File already exists: {}", name),
            RenameError::IoError(e) => write!(f, "Rename failed: {}", e),
        }
    }
}

impl std::error::Error for RenameError {}

/// Rename a file on disk, preserving its original extension.
///
/// Validates the new stem, builds the new path in the same directory with
/// the original extension, and performs the filesystem rename. If a backup
/// file exists (via `backup_path_for`), it is renamed too (best-effort).
///
/// Returns the new path on success, or the original path if the name is unchanged.
pub fn rename_file(
    old_path: &std::path::Path,
    new_stem: &str,
) -> Result<std::path::PathBuf, RenameError> {
    use crate::files::backup::backup_path_for;

    if new_stem.trim().is_empty() {
        return Err(RenameError::EmptyName);
    }

    if new_stem.chars().any(|c| !is_valid_filename_char(c)) {
        return Err(RenameError::InvalidChars);
    }

    let base_name = new_stem.split('.').next().unwrap_or(new_stem);
    let upper = base_name.to_uppercase();
    if WINDOWS_RESERVED.iter().any(|r| upper == *r) {
        return Err(RenameError::ReservedName);
    }

    let current_stem = old_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if new_stem == current_stem {
        return Ok(old_path.to_path_buf());
    }

    let ext = old_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let new_filename = if ext.is_empty() {
        new_stem.to_string()
    } else {
        format!("{}.{}", new_stem, ext)
    };

    if new_filename.len() > MAX_FILENAME_LENGTH {
        return Err(RenameError::TooLong);
    }

    let parent = old_path.parent().unwrap_or(std::path::Path::new("."));
    let new_path = parent.join(&new_filename);

    if new_path.exists() {
        return Err(RenameError::AlreadyExists(new_filename));
    }

    std::fs::rename(old_path, &new_path).map_err(RenameError::IoError)?;

    let old_backup = backup_path_for(old_path);
    if old_backup.exists() {
        let new_backup = backup_path_for(&new_path);
        let _ = std::fs::rename(&old_backup, &new_backup);
    }

    Ok(new_path)
}

// ---- Internal helpers ----

/// Processes characters: whitespace to hyphens, remove invalid/brackets, collapse hyphens.
fn process_chars(ascii: &str) -> String {
    let mut result = String::with_capacity(ascii.len());
    let mut last_was_hyphen = false;

    for c in ascii.chars() {
        if c.is_whitespace() {
            if !last_was_hyphen {
                result.push('-');
                last_was_hyphen = true;
            }
        } else if INVALID_CHARS.contains(&c) {
            continue;
        } else if c == '-' {
            if !last_was_hyphen {
                result.push('-');
                last_was_hyphen = true;
            }
        } else if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
            result.push(c);
            last_was_hyphen = false;
        } else if c == '(' || c == ')' || c == '[' || c == ']' {
            continue;
        }
    }

    result
}

/// Trims leading and trailing dots, spaces, and hyphens.
fn trim_edges(s: &str) -> String {
    s.trim_matches(|c| c == '.' || c == ' ' || c == '-')
        .to_string()
}

/// Checks if a name is a Windows reserved name and prefixes it if so.
fn handle_reserved_name(name: &str) -> String {
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

/// Vowels used for syllable detection and removal.
const VOWELS: &[char] = &['a', 'e', 'i', 'o', 'u', 'A', 'E', 'I', 'O', 'U'];

/// Minimum length for first word abbreviation before switching strategies.
const MIN_FIRST_WORD_ABBREV_LEN: usize = 3;

/// Minimum result length for abbreviation fallback.
const MIN_ABBREV_RESULT_LEN: usize = 2;

/// Extracts the first syllable of a word.
///
/// Short words (<=3 chars) returned unchanged. Finds first vowel, collects
/// consonants until next vowel, splits at doubled consonants if found.
fn first_syllable(word: &str) -> &str {
    if word.chars().count() <= 3 {
        return word;
    }

    let chars: Vec<char> = word.chars().collect();

    let first_vowel_idx = match chars.iter().position(|c| VOWELS.contains(c)) {
        Some(idx) => idx,
        None => return word,
    };

    let mut idx = first_vowel_idx + 1;
    let consonant_start = idx;

    while idx < chars.len() && !VOWELS.contains(&chars[idx]) {
        idx += 1;
    }

    if idx >= chars.len() {
        return word;
    }

    let consonant_count = idx - consonant_start;
    let cut_idx = if consonant_count >= 2 {
        let consonants = &chars[consonant_start..idx];
        let mut double_pos = None;
        for i in 0..consonants.len() - 1 {
            if consonants[i] == consonants[i + 1] {
                double_pos = Some(consonant_start + i + 1);
                break;
            }
        }
        double_pos.unwrap_or(idx)
    } else {
        idx
    };

    let byte_idx = word
        .char_indices()
        .nth(cut_idx)
        .map(|(i, _)| i)
        .unwrap_or(word.len());
    &word[..byte_idx]
}

/// Removes vowels from a word, keeping at least the first character.
fn remove_vowels(word: &str) -> String {
    let mut chars = word.chars();
    let mut result = String::with_capacity(word.len());

    if let Some(first) = chars.next() {
        result.push(first);
    }

    for c in chars {
        if !VOWELS.contains(&c) {
            result.push(c);
        }
    }

    result
}

/// Abbreviates the first word using vowel removal when syllable extraction is too short.
fn abbreviate_first_word(word: &str) -> String {
    let syllable = first_syllable(word);
    let syllable_len = syllable.chars().count();

    if syllable_len >= MIN_FIRST_WORD_ABBREV_LEN {
        return syllable.to_string();
    }

    let vowel_removed = remove_vowels(word);
    let vowel_removed_len = vowel_removed.chars().count();

    if vowel_removed_len >= MIN_ABBREV_RESULT_LEN && vowel_removed_len > syllable_len {
        vowel_removed
    } else if syllable_len >= MIN_ABBREV_RESULT_LEN {
        syllable.to_string()
    } else {
        let word_len = word.chars().count();
        if word_len >= MIN_ABBREV_RESULT_LEN {
            word.chars().take(MIN_ABBREV_RESULT_LEN).collect()
        } else if vowel_removed_len >= syllable_len {
            vowel_removed
        } else {
            syllable.to_string()
        }
    }
}

/// Truncates a string to the specified length using smart abbreviation.
///
/// Multi-word strings get syllable-based abbreviation; single words are hard-truncated.
fn truncate_to_length(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        return s.to_string();
    }

    let words: Vec<&str> = s.split(['-', '_', '.']).filter(|w| !w.is_empty()).collect();

    if words.len() <= 1 {
        return s.chars().take(max_len).collect();
    }

    let abbreviated: Vec<String> = words
        .iter()
        .enumerate()
        .map(|(i, w)| {
            if i == 0 {
                abbreviate_first_word(w)
            } else {
                first_syllable(w).to_string()
            }
        })
        .collect();
    let result = abbreviated.join("-");

    if result.chars().count() <= max_len {
        return result;
    }

    let separator_count = words.len() - 1;
    let available = max_len.saturating_sub(separator_count);
    let chars_per_word = available / words.len();

    let truncated: Vec<String> = abbreviated
        .iter()
        .map(|w| w.chars().take(chars_per_word.max(1)).collect::<String>())
        .collect();

    let joined = truncated.join("-");
    let cleaned = joined.trim_end_matches('-').to_string();

    if cleaned.chars().count() > max_len {
        let truncated: String = cleaned.chars().take(max_len).collect();
        truncated.trim_end_matches('-').to_string()
    } else {
        cleaned
    }
}
