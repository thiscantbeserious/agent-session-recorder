//! Filename generation and sanitization for recordings.
//!
//! Provides sanitization, truncation, and the top-level `generate()` entry point.
//! Template parsing and rendering live in `super::template`.

mod rename;
mod sanitize;

pub use rename::{apply_rename, current_stem, rename_file, target_path, RenameError};
pub use sanitize::{is_valid_filename_char, sanitize, sanitize_branch, sanitize_directory};

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
pub(super) const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Characters that are invalid in filenames on common filesystems.
pub const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Default fallback name when sanitization produces an empty result.
pub(super) const FALLBACK_NAME: &str = "recording";

/// Maximum filename length for most filesystems.
pub const MAX_FILENAME_LENGTH: usize = 255;

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

    validate_length(&filename)?;
    Ok(filename)
}

/// Resolves same-second filename collisions by appending a suffix (`a`..`z`).
///
/// If `dir/filename.cast` already exists, tries `dir/filenamea.cast` through
/// `dir/filenamez.cast`. Returns `None` if all 26 suffixes are taken.
pub fn resolve_collision(dir: &std::path::Path, filename: &str) -> Option<String> {
    let taken = |name: &str| {
        let path = dir.join(name);
        path.exists() || super::codec::other_variant(&path).exists()
    };
    if !taken(filename) {
        return Some(filename.to_string());
    }

    let (stem, ext) = split_stem_extension(filename);

    for suffix in b'a'..=b'z' {
        let candidate = if ext.is_empty() {
            format!("{}{}", stem, suffix as char)
        } else {
            format!("{}{}.{}", stem, suffix as char, ext)
        };
        if !taken(&candidate) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_appends_cast_only_when_missing() {
        let ctx = RenderContext {
            directory: "d",
            branch: None,
        };
        let cfg = Config::default();
        assert_eq!(generate(&ctx, "x", &cfg).unwrap(), "x.cast");
        assert_eq!(generate(&ctx, "x.cast", &cfg).unwrap(), "x.cast");
    }

    #[test]
    fn collision_without_extension_appends_the_suffix_letter() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("plain"), "").unwrap();
        assert_eq!(
            resolve_collision(dir.path(), "plain"),
            Some("plaina".into())
        );
    }

    #[test]
    fn errors_display_their_cause() {
        let long = FilenameError::TooLong {
            length: 300,
            max: 255,
        };
        assert_eq!(
            long.to_string(),
            "Filename too long: 300 characters (max 255)"
        );
        let g = GenerateError::from(long);
        assert!(g.to_string().starts_with("Filename error: "));
        let t = GenerateError::Template(TemplateError::Empty);
        assert!(t.to_string().starts_with("Template error: "));
    }
}
