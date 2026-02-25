//! Claude backend implementation.
//!
//! Invokes the Claude CLI with `--print --output-format json --tools ""`
//! for analysis. Optionally uses `--json-schema` for structured output.

use super::{
    extract_json, extract_json_inner, parse_rate_limit_info, wait_with_timeout, AgentBackend,
    AnalysisResponse, BackendError, BackendResult, RawMarker, MARKER_JSON_SCHEMA,
};
use crate::analyzer::TokenBudget;
use serde::Deserialize;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Backend for Claude CLI.
///
/// Uses `claude --print --output-format json --tools ""`
/// for non-interactive analysis. Optionally enforces JSON schema.
#[derive(Debug, Clone, Default)]
pub struct ClaudeBackend {
    /// Extra CLI arguments to pass before the stdin passthrough args.
    extra_args: Vec<String>,
}

impl ClaudeBackend {
    /// Create a new Claude backend with no extra arguments.
    pub fn new() -> Self {
        Self {
            extra_args: Vec::new(),
        }
    }

    /// Create a new Claude backend with extra CLI arguments.
    pub fn with_extra_args(extra_args: Vec<String>) -> Self {
        Self { extra_args }
    }

    /// Get the CLI command name.
    fn command() -> &'static str {
        "claude"
    }
}

impl AgentBackend for ClaudeBackend {
    fn name(&self) -> &'static str {
        "Claude"
    }

    fn is_available(&self) -> bool {
        super::command_exists(Self::command())
    }

    fn invoke(&self, prompt: &str, timeout: Duration, use_schema: bool) -> BackendResult<String> {
        if !self.is_available() {
            return Err(BackendError::NotAvailable(
                "claude CLI not found in PATH".to_string(),
            ));
        }

        // Build command with --tools "" to disable tool execution
        let mut cmd = Command::new(Self::command());
        cmd.args(["--print", "--output-format", "json"]);

        // Optionally add schema enforcement (slower but more reliable)
        if use_schema {
            cmd.args(["--json-schema", MARKER_JSON_SCHEMA]);
        }

        // Append extra args from per-agent config BEFORE the stdin passthrough
        for arg in &self.extra_args {
            cmd.arg(arg);
        }

        // Disable tools for read-only analysis
        // Use "-p -" to read prompt from stdin (avoids ARG_MAX limits)
        cmd.args(["--tools", "", "-p", "-"]);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn()?;

        // Write prompt to stdin and close it
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(prompt.as_bytes())?;
            // stdin is dropped here, closing the pipe
        }

        // Wait with timeout
        let result = wait_with_timeout(&mut child, timeout.as_secs());

        match result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if output.status.success() {
                    Ok(stdout)
                } else {
                    // Check for rate limiting in stderr
                    if let Some(info) = parse_rate_limit_info(&stderr) {
                        return Err(BackendError::RateLimited(info));
                    }

                    // Claude CLI may return exit code 1 but put error info in stdout
                    let error_msg = extract_error_from_claude_response(&stdout).unwrap_or(stderr);

                    Err(BackendError::ExitCode {
                        code: output.status.code().unwrap_or(-1),
                        stderr: error_msg,
                    })
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                Err(BackendError::Timeout(timeout))
            }
            Err(e) => Err(BackendError::Io(e)),
        }
    }

    fn parse_response(&self, response: &str) -> BackendResult<Vec<RawMarker>> {
        // Try Claude CLI wrapper envelope first
        if let Some(result) = try_claude_wrapper(response.trim()) {
            return result.map(|a| a.markers);
        }
        // Fall back to generic JSON extraction
        let analysis = extract_json(response)?;
        Ok(analysis.markers)
    }

    fn token_budget(&self) -> TokenBudget {
        TokenBudget::claude()
    }
}

/// Claude CLI wrapper format for error extraction.
#[derive(Debug, Deserialize)]
struct ClaudeErrorWrapper {
    is_error: Option<bool>,
    result: Option<String>,
}

/// Claude CLI wrapper envelope format.
///
/// When invoked with `--output-format json`, the Claude CLI wraps the actual
/// response inside this envelope. The inner `result` contains the model's text
/// output, and `structured_output` (when using `--json-schema`) contains the
/// parsed JSON directly.
#[derive(Debug, Deserialize)]
struct ClaudeWrapper {
    #[serde(rename = "type")]
    response_type: Option<String>,
    is_error: Option<bool>,
    result: Option<String>,
    structured_output: Option<AnalysisResponse>,
}

/// Try to unwrap a Claude CLI wrapper envelope.
///
/// Returns `Some(Ok(response))` if the wrapper was successfully unwrapped,
/// `Some(Err(..))` if the wrapper indicates an error, or `None` if the
/// input is not a Claude wrapper (allowing the caller to fall through to
/// generic extraction).
fn try_claude_wrapper(response: &str) -> Option<BackendResult<AnalysisResponse>> {
    let wrapper: ClaudeWrapper = serde_json::from_str(response).ok()?;

    // Must have type == "result" to be a Claude wrapper
    if wrapper.response_type.as_deref() != Some("result") {
        return None;
    }

    // Check for error responses
    if wrapper.is_error == Some(true) {
        return Some(Err(BackendError::JsonExtraction {
            response: wrapper.result.unwrap_or_else(|| "Claude error".to_string()),
        }));
    }

    // Try structured_output first (from --json-schema)
    if let Some(analysis) = wrapper.structured_output {
        return Some(Ok(analysis));
    }

    // Try to parse the inner result string
    let inner = wrapper.result.as_deref().unwrap_or("");
    if inner.is_empty() {
        return None;
    }

    Some(extract_json_inner(inner))
}

/// Extract error message from Claude's JSON response wrapper.
fn extract_error_from_claude_response(stdout: &str) -> Option<String> {
    let wrapper: ClaudeErrorWrapper = serde_json::from_str(stdout.trim()).ok()?;

    if wrapper.is_error == Some(true) {
        wrapper
            .result
            .or_else(|| Some("Claude returned an error".to_string()))
    } else {
        wrapper.result.filter(|r| !r.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::super::MarkerCategory;
    use super::*;

    // ============================================
    // ClaudeBackend basic tests
    // ============================================

    #[test]
    fn claude_backend_name() {
        let backend = ClaudeBackend::new();
        assert_eq!(backend.name(), "Claude");
    }

    #[test]
    fn claude_backend_token_budget() {
        let backend = ClaudeBackend::new();
        let budget = backend.token_budget();
        assert_eq!(budget.max_input_tokens, 100_000);
    }

    #[test]
    fn claude_backend_parse_valid_response() {
        let backend = ClaudeBackend::new();
        let response = r#"{"markers": [
            {"timestamp": 10.0, "label": "Started planning", "category": "planning"},
            {"timestamp": 45.0, "label": "Build complete", "category": "success"}
        ]}"#;

        let markers = backend.parse_response(response).unwrap();
        assert_eq!(markers.len(), 2);
        assert!((markers[0].timestamp - 10.0).abs() < 0.001);
        assert_eq!(markers[0].label, "Started planning");
    }

    #[test]
    fn claude_backend_parse_empty_markers() {
        let backend = ClaudeBackend::new();
        let response = r#"{"markers": []}"#;

        let markers = backend.parse_response(response).unwrap();
        assert!(markers.is_empty());
    }

    #[test]
    fn claude_backend_parse_invalid_json() {
        let backend = ClaudeBackend::new();
        let response = "not json at all";

        let result = backend.parse_response(response);
        assert!(result.is_err());
    }

    // ============================================
    // try_claude_wrapper unit tests
    // ============================================

    #[test]
    fn try_claude_wrapper_non_wrapper_json() {
        // Valid JSON but type != "result" => returns None (fall through)
        let json = r#"{"type":"other","result":"foo","is_error":false}"#;
        let result = try_claude_wrapper(json);
        assert!(result.is_none());
    }

    #[test]
    fn try_claude_wrapper_non_json() {
        // Not valid JSON at all => returns None
        let result = try_claude_wrapper("this is plain text");
        assert!(result.is_none());
    }

    #[test]
    fn try_claude_wrapper_error_wrapper() {
        // is_error: true => Some(Err(JsonExtraction))
        let json = r#"{"type":"result","is_error":true,"result":"Something went wrong"}"#;
        let result = try_claude_wrapper(json).expect("should return Some");
        assert!(matches!(result, Err(BackendError::JsonExtraction { .. })));
    }

    #[test]
    fn try_claude_wrapper_empty_result_returns_none() {
        // type=result, is_error=false, result="" and no structured_output => None
        let json = r#"{"type":"result","is_error":false,"result":""}"#;
        let result = try_claude_wrapper(json);
        assert!(result.is_none());
    }

    #[test]
    fn try_claude_wrapper_missing_type_returns_none() {
        // No "type" field => response_type is None => not "result" => returns None
        let json = r#"{"is_error":false,"result":"{\"markers\":[]}"}"#;
        let result = try_claude_wrapper(json);
        assert!(result.is_none());
    }

    // ============================================
    // Claude wrapper through parse_response tests
    // ============================================

    #[test]
    fn parse_response_wrapper_with_code_block() {
        let backend = ClaudeBackend::new();
        let response = r#"{"type":"result","subtype":"success","is_error":false,"result":"```json\n{\"markers\":[{\"timestamp\":10.0,\"label\":\"Test\",\"category\":\"success\"}]}\n```"}"#;
        let markers = backend.parse_response(response).unwrap();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].category, MarkerCategory::Success);
    }

    #[test]
    fn parse_response_wrapper_direct_json() {
        let backend = ClaudeBackend::new();
        let response = r#"{"type":"result","is_error":false,"result":"{\"markers\":[{\"timestamp\":5.0,\"label\":\"Plan\",\"category\":\"planning\"}]}"}"#;
        let markers = backend.parse_response(response).unwrap();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].category, MarkerCategory::Planning);
    }

    #[test]
    fn parse_response_wrapper_structured_output() {
        let backend = ClaudeBackend::new();
        let response = r#"{"type":"result","subtype":"success","is_error":false,"result":"","structured_output":{"markers":[{"timestamp":10.0,"label":"Schema output","category":"success"}]}}"#;
        let markers = backend.parse_response(response).unwrap();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].label, "Schema output");
        assert_eq!(markers[0].category, MarkerCategory::Success);
    }

    #[test]
    fn parse_response_wrapper_error() {
        let backend = ClaudeBackend::new();
        let response =
            r#"{"type":"result","is_error":true,"result":"Failed to analyze: content too large"}"#;
        let result = backend.parse_response(response);
        assert!(matches!(result, Err(BackendError::JsonExtraction { .. })));
    }

    #[test]
    fn parse_response_wrapper_empty_markers() {
        let backend = ClaudeBackend::new();
        let response =
            r#"{"type":"result","is_error":false,"result":"```json\n{\"markers\":[]}\n```"}"#;
        let markers = backend.parse_response(response).unwrap();
        assert!(markers.is_empty());
    }
}
