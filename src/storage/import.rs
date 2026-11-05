//! Import of external .cast files into managed storage

use super::StorageManager;
use crate::config::Compression;
use crate::files::codec;
use crate::files::codec::other_variant;
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
    let problem = if trimmed.is_empty() {
        "Agent name cannot be empty".to_string()
    } else if trimmed.contains('/') || trimmed.contains('\\') {
        format!("Agent name cannot contain path separators: {}", agent)
    } else if trimmed.contains("..") {
        format!("Agent name cannot contain '..': {}", agent)
    } else {
        return Ok(trimmed.to_string());
    };
    Err(ImportError::InvalidAgentName(problem))
}

/// Map an open failure to `PermissionDenied` or `NotFound`.
fn open_error(path: &Path, e: &anyhow::Error) -> ImportError {
    let denied = e
        .downcast_ref::<std::io::Error>()
        .is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied);
    let msg = format!("{}: {:#}", path.display(), e);
    if denied {
        ImportError::PermissionDenied(msg)
    } else {
        ImportError::NotFound(msg)
    }
}

/// First decoded line of a session, gunzipping `.cast.gz`.
fn read_first_line(path: &Path) -> Result<String, ImportError> {
    let mut reader = codec::open_reader(path).map_err(|e| open_error(path, &e))?;
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
    if !path.exists() {
        return Err(ImportError::NotFound(format!("{}", path.display())));
    }
    if !codec::is_session_path(path) {
        return Err(ImportError::WrongExtension(format!(
            "Expected .cast or .cast.gz extension, got: {}",
            path.display()
        )));
    }
    check_version(read_first_line(path)?.trim())
}

/// First line must be JSON with a `version` of 2 or 3.
fn check_version(first_line: &str) -> Result<(), ImportError> {
    let json: serde_json::Value = serde_json::from_str(first_line)
        .map_err(|e| ImportError::InvalidFormat(format!("First line is not valid JSON: {}", e)))?;
    let version = json.get("version").ok_or_else(|| {
        ImportError::InvalidFormat("Missing 'version' field in header".to_string())
    })?;
    match version.as_u64() {
        Some(2 | 3) => Ok(()),
        _ => Err(ImportError::InvalidFormat(format!(
            "Unsupported asciicast version: {}",
            version
        ))),
    }
}

/// Resolve the target path for `stem` with `suffix`, adding `-1`, `-2`, ... on conflict.
///
/// A conflict is either variant (`.cast` or `.cast.gz`) already existing. Errors when every
/// candidate is taken, an existing session is never overwritten.
fn resolve_target(dir: &Path, stem: &str, suffix: &str) -> Result<PathBuf, ImportError> {
    let taken = |p: &Path| p.exists() || other_variant(p).exists();
    let first = dir.join(format!("{stem}{suffix}"));
    std::iter::once(first)
        .chain((1..=999).map(|i| dir.join(format!("{stem}-{i}{suffix}"))))
        .find(|c| !taken(c))
        .ok_or_else(|| ImportError::CopyFailed(format!("Too many conflicting names for {stem}")))
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

/// Plain to plain is copied, everything else goes through our writer so `.cast.gz`
/// always carries the exact-size header and is deterministic, even from foreign gzip.
fn store(source: &Path, target: &Path) -> Result<(), ImportError> {
    let result = if !codec::is_gzip_path(source) && !codec::is_gzip_path(target) {
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
        let target = resolve_target(&agent_dir, codec::session_stem(filename), suffix)?;
        ensure_inside(&self.storage_dir(), &target)?;
        store(source, &target)?;
        Ok(target)
    }
}
