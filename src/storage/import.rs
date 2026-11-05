//! Import of external .cast files into managed storage

use super::sessions::other_variant;
use super::StorageManager;
use crate::config::Compression;
use crate::files::codec;
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};

/// Errors that can occur during cast file import
#[derive(Debug, Clone)]
pub enum ImportError {
    /// File was not found at the given path
    NotFound(String),
    /// Permission denied when accessing the file
    PermissionDenied(String),
    /// File does not have .cast or .cast.gz extension
    WrongExtension(String),
    /// File format is invalid (not valid asciicast JSON)
    InvalidFormat(String),
    /// Copy operation failed
    CopyFailed(String),
    /// Agent name is invalid (path traversal attempt or empty)
    InvalidAgentName(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::NotFound(msg) => write!(f, "File not found: {}", msg),
            ImportError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            ImportError::WrongExtension(msg) => write!(f, "Wrong extension: {}", msg),
            ImportError::InvalidFormat(msg) => write!(f, "Invalid format: {}", msg),
            ImportError::CopyFailed(msg) => write!(f, "Copy failed: {}", msg),
            ImportError::InvalidAgentName(msg) => write!(f, "Invalid agent name: {}", msg),
        }
    }
}

impl std::error::Error for ImportError {}

/// Validates that an agent name is safe to use for directory creation.
///
/// Rejects agent names that:
/// - Contain path separators (`/` or `\`)
/// - Contain path traversal sequences (`..`)
/// - Are empty or contain only whitespace
///
/// This prevents path traversal attacks when creating agent directories.
/// Returns the trimmed agent name on success.
fn validate_agent_name(agent: &str) -> Result<String, ImportError> {
    let trimmed = agent.trim();

    // Reject empty or whitespace-only names
    if trimmed.is_empty() {
        return Err(ImportError::InvalidAgentName(
            "Agent name cannot be empty".to_string(),
        ));
    }

    // Reject path separators
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(ImportError::InvalidAgentName(format!(
            "Agent name cannot contain path separators: {}",
            agent
        )));
    }

    // Reject path traversal attempts
    if trimmed.contains("..") {
        return Err(ImportError::InvalidAgentName(format!(
            "Agent name cannot contain '..': {}",
            agent
        )));
    }

    Ok(trimmed.to_string())
}

/// First decoded line of a session, gunzipping `.cast.gz`.
fn read_first_line(path: &Path) -> Result<String, ImportError> {
    let mut reader = codec::open_reader(path).map_err(|e| {
        let denied = e
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied);
        let msg = format!("{}: {:#}", path.display(), e);
        if denied {
            ImportError::PermissionDenied(msg)
        } else {
            ImportError::NotFound(msg)
        }
    })?;
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| ImportError::InvalidFormat(format!("Failed to read first line: {}", e)))?;
    Ok(line)
}

/// Validates that a file is a valid asciicast file by checking its header.
///
/// Checks:
/// - File exists and is readable
/// - Has .cast extension
/// - First line is valid JSON with a "version" field
/// - Version is 2 or 3
pub fn validate_cast_header(path: &Path) -> Result<(), ImportError> {
    // Check if file exists
    if !path.exists() {
        return Err(ImportError::NotFound(format!("{}", path.display())));
    }

    if !codec::is_session_path(path) {
        return Err(ImportError::WrongExtension(format!(
            "Expected .cast or .cast.gz extension, got: {}",
            path.display()
        )));
    }

    let first_line = read_first_line(path)?;

    // Parse as JSON and check for version field
    let json: serde_json::Value = serde_json::from_str(first_line.trim())
        .map_err(|e| ImportError::InvalidFormat(format!("First line is not valid JSON: {}", e)))?;

    // Check for version field
    let version = json.get("version").ok_or_else(|| {
        ImportError::InvalidFormat("Missing 'version' field in header".to_string())
    })?;

    // Accept version 2 or 3
    if let Some(v) = version.as_u64() {
        if v == 2 || v == 3 {
            return Ok(());
        }
    }

    Err(ImportError::InvalidFormat(format!(
        "Unsupported asciicast version: {}",
        version
    )))
}

/// Resolve the target path for `stem` with `suffix`, adding `-1`, `-2`, ... on conflict.
///
/// A conflict is either variant (`.cast` or `.cast.gz`) already existing.
fn resolve_target(dir: &Path, stem: &str, suffix: &str) -> PathBuf {
    let taken = |p: &Path| p.exists() || other_variant(p).exists();
    let target = dir.join(format!("{stem}{suffix}"));
    if !taken(&target) {
        return target;
    }
    (1..=999)
        .map(|i| dir.join(format!("{stem}-{i}{suffix}")))
        .find(|c| !taken(c))
        // Fallback - return original target (should be very rare)
        .unwrap_or(target)
}

/// Defense in depth: verify the target path is within the storage directory.
fn ensure_inside(storage_dir: &Path, target: &Path) -> Result<(), ImportError> {
    let canonical_storage = storage_dir.canonicalize().map_err(|e| {
        ImportError::CopyFailed(format!("Failed to canonicalize storage dir: {}", e))
    })?;
    let parent = target
        .parent()
        .ok_or_else(|| ImportError::CopyFailed("Target path has no parent".to_string()))?;
    let canonical_parent = parent.canonicalize().map_err(|e| {
        ImportError::CopyFailed(format!("Failed to canonicalize target path: {}", e))
    })?;
    if canonical_parent.starts_with(&canonical_storage) {
        return Ok(());
    }
    Err(ImportError::InvalidAgentName(format!(
        "Target path is outside storage directory: {}",
        target.display()
    )))
}

/// Copy when formats match, else transcode to the target format.
fn store(source: &Path, target: &Path) -> Result<(), ImportError> {
    let result = if codec::is_gzip_path(source) == codec::is_gzip_path(target) {
        fs::copy(source, target)
            .map(drop)
            .map_err(anyhow::Error::from)
    } else {
        codec::transcode(source, target)
    };
    result.map_err(|e| ImportError::CopyFailed(format!("Failed to copy file: {:#}", e)))
}

impl StorageManager {
    /// Imports a cast file into managed storage under the specified agent directory.
    ///
    /// Validates the file header, ensures the agent directory exists, resolves
    /// filename conflicts by adding numeric suffixes if needed, and copies the file.
    ///
    /// # Arguments
    /// * `source` - Path to the source .cast file to import
    /// * `agent` - Agent name (subdirectory) to import the file into
    ///
    /// # Returns
    /// * `Ok(PathBuf)` - Full path to the imported file in managed storage
    /// * `Err(ImportError)` - If validation or copy fails
    pub fn import_cast_file(&self, source: &Path, agent: &str) -> Result<PathBuf, ImportError> {
        // Validate agent name to prevent path traversal and get trimmed name
        let agent = validate_agent_name(agent)?;

        // Validate the source file
        validate_cast_header(source)?;

        // Ensure agent directory exists
        let agent_dir = self
            .ensure_agent_dir(&agent)
            .map_err(|e| ImportError::CopyFailed(format!("Failed to create agent dir: {}", e)))?;

        let filename = source
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or_else(|| ImportError::CopyFailed("Invalid source filename".to_string()))?;

        let suffix = match self.config.storage.compression {
            Compression::Gzip => codec::GZIP_SUFFIX,
            Compression::None => codec::SESSION_SUFFIX,
        };
        let target = resolve_target(&agent_dir, codec::session_stem(filename), suffix);
        ensure_inside(&self.storage_dir(), &target)?;
        store(source, &target)?;
        Ok(target)
    }
}
