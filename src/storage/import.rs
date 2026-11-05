//! Import of external .cast files into managed storage

use super::StorageManager;
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
    /// File does not have .cast extension
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

    // Check extension
    if path.extension().and_then(std::ffi::OsStr::to_str) != Some("cast") {
        return Err(ImportError::WrongExtension(format!(
            "Expected .cast extension, got: {}",
            path.display()
        )));
    }

    // Try to open file
    let file = fs::File::open(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            ImportError::PermissionDenied(format!("{}: {}", path.display(), e))
        } else {
            ImportError::NotFound(format!("{}: {}", path.display(), e))
        }
    })?;

    // Read first line
    let mut reader = std::io::BufReader::new(file);
    let mut first_line = String::new();
    reader
        .read_line(&mut first_line)
        .map_err(|e| ImportError::InvalidFormat(format!("Failed to read first line: {}", e)))?;

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

/// Helper function to resolve filename conflicts by adding a numeric suffix.
///
/// If the target path does not exist, returns it unchanged.
/// If it exists, tries stem-1.ext, stem-2.ext, etc. up to stem-999.ext.
fn resolve_filename_conflict(dir: &Path, filename: &str) -> PathBuf {
    let target = dir.join(filename);

    // No conflict - return as-is
    if !target.exists() {
        return target;
    }

    // Split filename into stem and extension
    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or(filename);
    let ext = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("");

    // Try numbered variants
    for i in 1..=999 {
        let new_filename = if ext.is_empty() {
            format!("{}-{}", stem, i)
        } else {
            format!("{}-{}.{}", stem, i, ext)
        };

        let candidate = dir.join(&new_filename);
        if !candidate.exists() {
            return candidate;
        }
    }

    // Fallback - return original target (should be very rare)
    target
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

        // Get source filename
        let filename = source
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or_else(|| ImportError::CopyFailed("Invalid source filename".to_string()))?;

        // Resolve conflicts
        let target = resolve_filename_conflict(&agent_dir, filename);

        // Defense in depth: verify target path is within storage directory
        let storage_dir = self.storage_dir();
        let canonical_storage = storage_dir.canonicalize().map_err(|e| {
            ImportError::CopyFailed(format!("Failed to canonicalize storage dir: {}", e))
        })?;

        // Get the parent of target and canonicalize it (target doesn't exist yet)
        let target_parent = target
            .parent()
            .ok_or_else(|| ImportError::CopyFailed("Target path has no parent".to_string()))?;
        let canonical_target_parent = target_parent.canonicalize().map_err(|e| {
            ImportError::CopyFailed(format!("Failed to canonicalize target path: {}", e))
        })?;

        if !canonical_target_parent.starts_with(&canonical_storage) {
            return Err(ImportError::InvalidAgentName(format!(
                "Target path is outside storage directory: {}",
                target.display()
            )));
        }

        // Copy file
        fs::copy(source, &target)
            .map_err(|e| ImportError::CopyFailed(format!("Failed to copy file: {}", e)))?;

        Ok(target)
    }
}
