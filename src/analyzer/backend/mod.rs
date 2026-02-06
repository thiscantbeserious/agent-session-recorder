//! Agent backend implementations for LLM analysis.
//!
//! This module provides the Strategy pattern for different AI agent backends.
//! Each backend knows how to invoke its CLI and parse responses.
//!
//! # Supported Agents
//!
//! - **Claude**: `claude --print --output-format json --json-schema --tools ""`
//! - **Codex**: `codex exec --output-schema` (structured JSON output)
//! - **Gemini**: `gemini --output-format json --approval-mode plan`
//!
//! # Design
//!
//! The `AgentBackend` trait defines the interface for all backends.
//! Backends are stateless and can be used concurrently from multiple threads.

mod claude;
mod codex;
mod gemini;

pub use claude::ClaudeBackend;
pub use codex::CodexBackend;
pub use gemini::GeminiBackend;

use crate::analyzer::chunk::TokenBudget;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

/// JSON Schema for marker output (minified, for inline CLI args like Claude's --json-schema).
pub const MARKER_JSON_SCHEMA: &str = r#"{"type":"object","properties":{"markers":{"type":"array","items":{"type":"object","properties":{"timestamp":{"type":"number"},"label":{"type":"string"},"category":{"type":"string","enum":["planning","design","implementation","success","failure"]}},"required":["timestamp","label","category"]}}},"required":["markers"]}"#;

/// Get the path to the marker schema JSON file (for CLIs that need a file path like Codex).
///
/// Returns the path relative to the binary's location or falls back to a temp file.
pub fn get_schema_file_path() -> std::io::Result<PathBuf> {
    // Write schema to temp file for CLIs that require file paths
    let temp_dir = std::env::temp_dir();
    let schema_path = temp_dir.join("agr_marker_schema.json");

    // Write schema if it doesn't exist or is stale
    if !schema_path.exists() {
        std::fs::write(&schema_path, MARKER_JSON_SCHEMA)?;
    }

    Ok(schema_path)
}

/// Wait for child process with timeout.
///
/// Uses a simple polling approach since std::process doesn't have
/// native timeout support. Includes proper process reaping to prevent zombies.
pub(crate) fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout_secs: u64,
) -> std::io::Result<std::process::Output> {
    use std::io::Read;
    use std::thread;
    use std::time::Instant;

    let start = Instant::now();
    let poll_interval = Duration::from_millis(100);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Process finished - collect output
                let stdout = child
                    .stdout
                    .take()
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        s.read_to_end(&mut buf).ok();
                        buf
                    })
                    .unwrap_or_default();

                let stderr = child
                    .stderr
                    .take()
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        s.read_to_end(&mut buf).ok();
                        buf
                    })
                    .unwrap_or_default();

                return Ok(std::process::Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                // Still running - check timeout
                if start.elapsed().as_secs() >= timeout_secs {
                    // Kill and reap to prevent zombie process
                    let _ = child.kill();
                    let _ = child.wait(); // Reap the zombie
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "Process timed out",
                    ));
                }
                thread::sleep(poll_interval);
            }
            Err(e) => return Err(e),
        }
    }
}

/// Result type for agent backend operations.
pub type BackendResult<T> = Result<T, BackendError>;

/// Trait for AI agent backends (Strategy pattern).
///
/// Implementors must be thread-safe as they may be used from multiple
/// threads during parallel chunk processing.
pub trait AgentBackend: Send + Sync {
    /// Human-readable name for logging.
    fn name(&self) -> &'static str;

    /// Check if the agent CLI is available on the system.
    fn is_available(&self) -> bool;

    /// Invoke the agent with a prompt and return raw response.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The analysis prompt to send to the agent
    /// * `timeout` - Maximum time to wait for response
    /// * `use_schema` - Whether to enforce JSON schema (slower but more reliable)
    ///
    /// # Returns
    ///
    /// The raw response string from the agent CLI.
    fn invoke(&self, prompt: &str, timeout: Duration, use_schema: bool) -> BackendResult<String>;

    /// Parse raw response into markers.
    ///
    /// Handles JSON extraction and validation. For agents without
    /// native JSON output (Codex), this extracts JSON from text.
    fn parse_response(&self, response: &str) -> BackendResult<Vec<RawMarker>>;

    /// Get the token budget for this agent.
    fn token_budget(&self) -> TokenBudget;
}

/// Agent types supported for analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentType {
    Claude,
    Codex,
    Gemini,
}

impl AgentType {
    /// Create the appropriate backend for this agent type.
    pub fn create_backend(&self) -> Box<dyn AgentBackend> {
        match self {
            AgentType::Claude => Box::new(ClaudeBackend::new()),
            AgentType::Codex => Box::new(CodexBackend::new()),
            AgentType::Gemini => Box::new(GeminiBackend::new()),
        }
    }

    /// Get the CLI command name for this agent.
    pub fn command_name(&self) -> &'static str {
        match self {
            AgentType::Claude => "claude",
            AgentType::Codex => "codex",
            AgentType::Gemini => "gemini",
        }
    }

    /// Get the token budget for this agent type.
    pub fn token_budget(&self) -> TokenBudget {
        match self {
            AgentType::Claude => TokenBudget::claude(),
            AgentType::Codex => TokenBudget::codex(),
            AgentType::Gemini => TokenBudget::gemini(),
        }
    }
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentType::Claude => write!(f, "Claude"),
            AgentType::Codex => write!(f, "Codex"),
            AgentType::Gemini => write!(f, "Gemini"),
        }
    }
}

/// Errors from agent backends.
#[derive(Debug, Error)]
pub enum BackendError {
    #[error("Agent CLI not found: {0}")]
    NotAvailable(String),

    #[error("Agent timed out after {0:?}")]
    Timeout(Duration),

    #[error("Exit code {code}: {}", truncate_stderr(stderr))]
    ExitCode { code: i32, stderr: String },

    #[error("Rate limited: {0}")]
    RateLimited(RateLimitInfo),

    #[error("Failed to parse response as JSON: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("Failed to extract JSON from response")]
    JsonExtraction { response: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Rate limit information extracted from agent response.
#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    /// When the rate limit resets (if provided by agent)
    pub retry_after: Option<Duration>,
    /// Human-readable message
    pub message: String,
}

impl std::fmt::Display for RateLimitInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(retry_after) = self.retry_after {
            write!(f, "{} (retry after {:?})", self.message, retry_after)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl BackendError {
    /// Extract wait duration for retry logic.
    ///
    /// Uses agent-provided retry_after if available, otherwise falls back
    /// to the provided default duration.
    pub fn wait_duration(&self, fallback: Duration) -> Duration {
        match self {
            BackendError::RateLimited(info) => info.retry_after.unwrap_or(fallback),
            _ => fallback,
        }
    }
}

/// Raw marker from LLM response (before timestamp resolution).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawMarker {
    /// Relative timestamp (seconds from chunk start)
    pub timestamp: f64,
    /// Description of the moment
    pub label: String,
    /// Engineering category
    pub category: MarkerCategory,
}

/// Engineering workflow categories for markers.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MarkerCategory {
    /// Task breakdown, approach decisions, strategy discussion
    Planning,
    /// Architecture decisions, API design, data model choices
    Design,
    /// Code writing, file modifications, command execution
    Implementation,
    /// Tests passing, builds working, feature complete
    Success,
    /// Errors, test failures, failed approaches
    Failure,
}

impl std::fmt::Display for MarkerCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarkerCategory::Planning => write!(f, "PLAN"),
            MarkerCategory::Design => write!(f, "DESIGN"),
            MarkerCategory::Implementation => write!(f, "IMPL"),
            MarkerCategory::Success => write!(f, "SUCCESS"),
            MarkerCategory::Failure => write!(f, "FAILURE"),
        }
    }
}

/// LLM response wrapper for JSON parsing.
#[derive(Debug, Deserialize)]
pub struct AnalysisResponse {
    pub markers: Vec<RawMarker>,
}

/// Claude CLI wrapper format when using `--output-format json`.
///
/// Claude wraps the actual response in a metadata envelope:
/// - Without schema: `{"type":"result","result":"...","is_error":false,...}`
/// - With schema: `{"type":"result","result":"","structured_output":{...},"is_error":false,...}`
#[derive(Debug, Deserialize)]
struct ClaudeWrapper {
    #[serde(rename = "type")]
    response_type: Option<String>,
    result: Option<String>,
    is_error: Option<bool>,
    /// When using --json-schema, structured output appears here instead of result
    structured_output: Option<serde_json::Value>,
}

/// Extract JSON from a potentially wrapped text response.
///
/// Handles multiple response formats:
/// 1. Claude CLI wrapper (`{"type":"result","result":"..."}`)
/// 2. Direct JSON object
/// 3. JSON embedded in text
/// 4. JSON in markdown code blocks
///
/// # Arguments
///
/// * `response` - The raw response string from the agent
///
/// # Returns
///
/// The parsed `AnalysisResponse` containing markers.
pub fn extract_json(response: &str) -> BackendResult<AnalysisResponse> {
    let trimmed = response.trim();

    // Try Claude CLI wrapper format first
    // Without schema: {"type":"result","result":"```json\n{...}\n```",...}
    // With schema: {"type":"result","result":"","structured_output":{...},...}
    if let Ok(wrapper) = serde_json::from_str::<ClaudeWrapper>(trimmed) {
        if wrapper.response_type.as_deref() == Some("result") {
            // Check for error response
            if wrapper.is_error == Some(true) {
                return Err(BackendError::JsonExtraction {
                    response: wrapper
                        .result
                        .unwrap_or_else(|| "Claude returned an error".to_string()),
                });
            }

            // Check for structured_output first (when using --json-schema)
            if let Some(structured) = wrapper.structured_output {
                // Parse the structured output directly
                return serde_json::from_value(structured).map_err(BackendError::JsonParse);
            }

            // Fall back to result field (without --json-schema)
            if let Some(inner) = wrapper.result {
                if !inner.is_empty() {
                    return extract_json_inner(&inner);
                }
            }
        }
    }

    // Fall back to standard extraction
    extract_json_inner(trimmed)
}

/// Inner JSON extraction logic (handles direct JSON, text-embedded, code blocks).
fn extract_json_inner(response: &str) -> BackendResult<AnalysisResponse> {
    let trimmed = response.trim();

    // Try direct parse first
    if let Ok(parsed) = serde_json::from_str(trimmed) {
        return Ok(parsed);
    }

    // Try code block extraction (check before JSON boundaries to handle
    // code-fenced responses with surrounding text)
    if let Some(json_str) = extract_from_code_block(trimmed) {
        if let Ok(parsed) = serde_json::from_str(json_str) {
            return Ok(parsed);
        }
    }

    // Try to find JSON object boundaries
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        let json_str = &trimmed[start..=end];
        if let Ok(parsed) = serde_json::from_str(json_str) {
            return Ok(parsed);
        }
    }

    Err(BackendError::JsonExtraction {
        response: response.to_string(),
    })
}

/// Extract JSON from markdown code blocks.
fn extract_from_code_block(text: &str) -> Option<&str> {
    let patterns = ["```json\n", "```json\r\n", "```\n", "```\r\n"];

    for pattern in patterns {
        if let Some(start) = text.find(pattern) {
            let json_start = start + pattern.len();
            if let Some(end) = text[json_start..].find("```") {
                return Some(&text[json_start..json_start + end]);
            }
        }
    }
    None
}

/// Parse rate limit info from agent CLI stderr.
///
/// Each agent signals rate limiting differently. This function
/// attempts to extract retry-after timing from various formats.
pub fn parse_rate_limit_info(stderr: &str) -> Option<RateLimitInfo> {
    let stderr_lower = stderr.to_lowercase();

    // Check for rate limit indicators
    let is_rate_limited = stderr_lower.contains("rate limit")
        || stderr_lower.contains("throttled")
        || stderr_lower.contains("resource_exhausted")
        || stderr_lower.contains("429")
        || stderr_lower.contains("too many requests")
        || stderr_lower.contains("quota exceeded");

    if !is_rate_limited {
        return None;
    }

    let retry_after = extract_retry_seconds(&stderr_lower).map(Duration::from_secs);

    Some(RateLimitInfo {
        retry_after,
        message: stderr.lines().next().unwrap_or("Rate limited").to_string(),
    })
}

/// Extract retry delay from various formats.
///
/// Parses common rate-limit retry timing patterns without regex dependency.
fn extract_retry_seconds(stderr: &str) -> Option<u64> {
    // Helper to extract number after a keyword
    let extract_after = |text: &str, keyword: &str| -> Option<u64> {
        text.find(keyword).and_then(|pos| {
            let after = &text[pos + keyword.len()..];
            extract_first_number(after)
        })
    };

    // Try various patterns
    // "retry after 45 seconds" or "retry after 45"
    if let Some(secs) = extract_after(stderr, "retry after ") {
        return Some(secs);
    }

    // "retry_after_seconds: 45" or "retry_after: 45"
    if let Some(secs) = extract_after(stderr, "retry_after") {
        return Some(secs);
    }

    // "retry in 30s" or "retry in 30"
    if let Some(secs) = extract_after(stderr, "retry in ") {
        return Some(secs);
    }

    // "retrydelay: 60" (Gemini style)
    if let Some(secs) = extract_after(stderr, "retrydelay:") {
        return Some(secs);
    }

    // "wait 30 seconds"
    if let Some(secs) = extract_after(stderr, "wait ") {
        return Some(secs);
    }

    // "45 seconds remaining"
    if stderr.contains("seconds") {
        return extract_first_number(stderr);
    }

    None
}

/// Extract the first number from a string.
fn extract_first_number(s: &str) -> Option<u64> {
    let mut num_str = String::new();
    let mut found_digit = false;

    for c in s.chars() {
        if c.is_ascii_digit() {
            num_str.push(c);
            found_digit = true;
        } else if found_digit {
            // Stop at first non-digit after finding digits
            break;
        } else if !c.is_whitespace() && c != ':' {
            // Skip whitespace and colons, but stop at other chars before digits
            if !num_str.is_empty() {
                break;
            }
        }
    }

    num_str.parse().ok()
}

/// Truncate stderr for error display.
///
/// Takes the first line and limits to 200 characters for readability.
fn truncate_stderr(stderr: &str) -> String {
    let first_line = stderr.lines().next().unwrap_or("").trim();
    if first_line.len() <= 200 {
        first_line.to_string()
    } else {
        format!("{}...", &first_line[..200])
    }
}

/// Check if a command is available in PATH.
///
/// Uses platform-specific command lookup:
/// - Unix: `which` command
/// - Windows: `where` command
pub fn command_exists(command: &str) -> bool {
    #[cfg(windows)]
    let lookup_cmd = "where";
    #[cfg(not(windows))]
    let lookup_cmd = "which";

    std::process::Command::new(lookup_cmd)
        .arg(command)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================
    // RawMarker and MarkerCategory Tests
    // ============================================

    #[test]
    fn marker_category_deserialize() {
        let json = r#"{"timestamp": 10.5, "label": "Test", "category": "planning"}"#;
        let marker: RawMarker = serde_json::from_str(json).unwrap();
        assert_eq!(marker.category, MarkerCategory::Planning);
    }

    #[test]
    fn marker_category_all_variants() {
        let variants = [
            ("planning", MarkerCategory::Planning),
            ("design", MarkerCategory::Design),
            ("implementation", MarkerCategory::Implementation),
            ("success", MarkerCategory::Success),
            ("failure", MarkerCategory::Failure),
        ];

        for (json_val, expected) in variants {
            let json = format!(
                r#"{{"timestamp": 0.0, "label": "x", "category": "{}"}}"#,
                json_val
            );
            let marker: RawMarker = serde_json::from_str(&json).unwrap();
            assert_eq!(marker.category, expected);
        }
    }

    #[test]
    fn marker_category_display() {
        assert_eq!(format!("{}", MarkerCategory::Planning), "PLAN");
        assert_eq!(format!("{}", MarkerCategory::Design), "DESIGN");
        assert_eq!(format!("{}", MarkerCategory::Implementation), "IMPL");
        assert_eq!(format!("{}", MarkerCategory::Success), "SUCCESS");
        assert_eq!(format!("{}", MarkerCategory::Failure), "FAILURE");
    }

    #[test]
    fn raw_marker_deserialize_full() {
        let json = r#"{
            "timestamp": 45.2,
            "label": "Build failed - missing dependency",
            "category": "failure"
        }"#;

        let marker: RawMarker = serde_json::from_str(json).unwrap();
        assert!((marker.timestamp - 45.2).abs() < 0.001);
        assert_eq!(marker.label, "Build failed - missing dependency");
        assert_eq!(marker.category, MarkerCategory::Failure);
    }

    // ============================================
    // JSON Extraction Tests
    // ============================================

    #[test]
    fn extract_json_direct() {
        let response =
            r#"{"markers": [{"timestamp": 10.0, "label": "Test", "category": "success"}]}"#;
        let result = extract_json(response).unwrap();
        assert_eq!(result.markers.len(), 1);
        assert_eq!(result.markers[0].category, MarkerCategory::Success);
    }

    #[test]
    fn extract_json_with_whitespace() {
        let response = r#"

        {"markers": [{"timestamp": 10.0, "label": "Test", "category": "planning"}]}

        "#;
        let result = extract_json(response).unwrap();
        assert_eq!(result.markers.len(), 1);
    }

    #[test]
    fn extract_json_from_text() {
        let response = r#"Here is the analysis:
        {"markers": [{"timestamp": 10.0, "label": "Test", "category": "design"}]}
        That's all."#;
        let result = extract_json(response).unwrap();
        assert_eq!(result.markers.len(), 1);
    }

    #[test]
    fn extract_json_from_code_block() {
        let response = r#"Here is the result:
```json
{"markers": [{"timestamp": 5.0, "label": "Started", "category": "implementation"}]}
```
Done."#;
        let result = extract_json(response).unwrap();
        assert_eq!(result.markers.len(), 1);
        assert_eq!(result.markers[0].category, MarkerCategory::Implementation);
    }

    #[test]
    fn extract_json_from_plain_code_block() {
        let response = r#"
```
{"markers": [{"timestamp": 5.0, "label": "Test", "category": "success"}]}
```
"#;
        let result = extract_json(response).unwrap();
        assert_eq!(result.markers.len(), 1);
    }

    #[test]
    fn extract_json_multiple_markers() {
        let response = r#"{"markers": [
            {"timestamp": 12.5, "label": "Started planning", "category": "planning"},
            {"timestamp": 45.2, "label": "Build failed", "category": "failure"},
            {"timestamp": 78.9, "label": "Tests passing", "category": "success"}
        ]}"#;
        let result = extract_json(response).unwrap();
        assert_eq!(result.markers.len(), 3);
    }

    #[test]
    fn extract_json_malformed_error() {
        let response = "This is not JSON at all";
        let result = extract_json(response);
        assert!(matches!(result, Err(BackendError::JsonExtraction { .. })));
    }

    #[test]
    fn extract_json_claude_wrapper_with_code_block() {
        // This is the actual format Claude CLI returns with --output-format json
        let response = r#"{"type":"result","subtype":"success","is_error":false,"result":"```json\n{\"markers\":[{\"timestamp\":10.0,\"label\":\"Test\",\"category\":\"success\"}]}\n```"}"#;
        let result = extract_json(response).unwrap();
        assert_eq!(result.markers.len(), 1);
        assert_eq!(result.markers[0].category, MarkerCategory::Success);
    }

    #[test]
    fn extract_json_claude_wrapper_direct_json() {
        // Claude wrapper with direct JSON (no code blocks)
        let response = r#"{"type":"result","is_error":false,"result":"{\"markers\":[{\"timestamp\":5.0,\"label\":\"Plan\",\"category\":\"planning\"}]}"}"#;
        let result = extract_json(response).unwrap();
        assert_eq!(result.markers.len(), 1);
        assert_eq!(result.markers[0].category, MarkerCategory::Planning);
    }

    #[test]
    fn extract_json_claude_wrapper_structured_output() {
        // Claude wrapper with structured_output (when using --json-schema)
        let response = r#"{"type":"result","subtype":"success","is_error":false,"result":"","structured_output":{"markers":[{"timestamp":10.0,"label":"Schema output","category":"success"}]}}"#;
        let result = extract_json(response).unwrap();
        assert_eq!(result.markers.len(), 1);
        assert_eq!(result.markers[0].label, "Schema output");
        assert_eq!(result.markers[0].category, MarkerCategory::Success);
    }

    #[test]
    fn extract_json_claude_wrapper_error() {
        // Claude wrapper with is_error: true
        let response =
            r#"{"type":"result","is_error":true,"result":"Failed to analyze: content too large"}"#;
        let result = extract_json(response);
        assert!(matches!(result, Err(BackendError::JsonExtraction { .. })));
    }

    #[test]
    fn extract_json_claude_wrapper_empty_markers() {
        let response =
            r#"{"type":"result","is_error":false,"result":"```json\n{\"markers\":[]}\n```"}"#;
        let result = extract_json(response).unwrap();
        assert!(result.markers.is_empty());
    }

    #[test]
    fn extract_json_empty_markers() {
        let response = r#"{"markers": []}"#;
        let result = extract_json(response).unwrap();
        assert!(result.markers.is_empty());
    }

    // ============================================
    // Rate Limit Parsing Tests
    // ============================================

    #[test]
    fn parse_rate_limit_claude_format() {
        let stderr = "Error: Rate limited. Retry after 45 seconds";
        let info = parse_rate_limit_info(stderr).unwrap();
        assert_eq!(info.retry_after, Some(Duration::from_secs(45)));
    }

    #[test]
    fn parse_rate_limit_codex_format() {
        let stderr = "Request throttled, retry in 30s";
        let info = parse_rate_limit_info(stderr).unwrap();
        assert_eq!(info.retry_after, Some(Duration::from_secs(30)));
    }

    #[test]
    fn parse_rate_limit_gemini_format() {
        let stderr = "RESOURCE_EXHAUSTED: Quota exceeded. retryDelay: 60";
        let info = parse_rate_limit_info(stderr).unwrap();
        assert_eq!(info.retry_after, Some(Duration::from_secs(60)));
    }

    #[test]
    fn parse_rate_limit_429_status() {
        let stderr = "HTTP 429: Too many requests";
        let info = parse_rate_limit_info(stderr).unwrap();
        assert!(info.retry_after.is_none());
    }

    #[test]
    fn parse_rate_limit_not_rate_limited() {
        let stderr = "Error: Connection failed";
        let info = parse_rate_limit_info(stderr);
        assert!(info.is_none());
    }

    // ============================================
    // BackendError Tests
    // ============================================

    #[test]
    fn backend_error_wait_duration_rate_limited() {
        let err = BackendError::RateLimited(RateLimitInfo {
            retry_after: Some(Duration::from_secs(30)),
            message: "Rate limited".to_string(),
        });
        assert_eq!(
            err.wait_duration(Duration::from_secs(5)),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn backend_error_wait_duration_fallback() {
        let err = BackendError::Timeout(Duration::from_secs(60));
        assert_eq!(
            err.wait_duration(Duration::from_secs(5)),
            Duration::from_secs(5)
        );
    }

    #[test]
    fn backend_error_wait_duration_rate_limited_no_retry_after() {
        let err = BackendError::RateLimited(RateLimitInfo {
            retry_after: None,
            message: "Rate limited".to_string(),
        });
        assert_eq!(
            err.wait_duration(Duration::from_secs(10)),
            Duration::from_secs(10)
        );
    }

    #[test]
    fn backend_error_exit_code_displays_stderr() {
        let err = BackendError::ExitCode {
            code: 1,
            stderr: "error: prompt too large for context window".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Exit code 1"));
        assert!(msg.contains("prompt too large"));
    }

    #[test]
    fn backend_error_exit_code_truncates_long_stderr() {
        let long_stderr = "x".repeat(300);
        let err = BackendError::ExitCode {
            code: 1,
            stderr: long_stderr,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Exit code 1"));
        assert!(msg.contains("..."));
        // Message should be truncated to ~200 chars plus "..."
        assert!(msg.len() < 250);
    }

    #[test]
    fn backend_error_exit_code_uses_first_line() {
        let err = BackendError::ExitCode {
            code: 1,
            stderr: "first line error\nsecond line details\nthird line".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("first line error"));
        assert!(!msg.contains("second line"));
    }

    #[test]
    fn truncate_stderr_short_message() {
        let result = truncate_stderr("short error");
        assert_eq!(result, "short error");
    }

    #[test]
    fn truncate_stderr_long_message() {
        let long = "x".repeat(250);
        let result = truncate_stderr(&long);
        assert!(result.len() <= 203); // 200 + "..."
        assert!(result.ends_with("..."));
    }

    #[test]
    fn truncate_stderr_multiline() {
        let result = truncate_stderr("first line\nsecond line\nthird");
        assert_eq!(result, "first line");
    }

    #[test]
    fn truncate_stderr_empty() {
        let result = truncate_stderr("");
        assert_eq!(result, "");
    }

    // ============================================
    // AgentType Tests
    // ============================================

    #[test]
    fn agent_type_command_name() {
        assert_eq!(AgentType::Claude.command_name(), "claude");
        assert_eq!(AgentType::Codex.command_name(), "codex");
        assert_eq!(AgentType::Gemini.command_name(), "gemini");
    }

    #[test]
    fn agent_type_display() {
        assert_eq!(format!("{}", AgentType::Claude), "Claude");
        assert_eq!(format!("{}", AgentType::Codex), "Codex");
        assert_eq!(format!("{}", AgentType::Gemini), "Gemini");
    }

    #[test]
    fn agent_type_create_backend() {
        // Just verify it creates without panic
        let _ = AgentType::Claude.create_backend();
        let _ = AgentType::Codex.create_backend();
        let _ = AgentType::Gemini.create_backend();
    }
}
