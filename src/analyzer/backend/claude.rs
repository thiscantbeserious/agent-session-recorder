//! Claude backend implementation.
//!
//! Invokes the Claude CLI with `--print --output-format json --tools ""`
//! for analysis. Optionally uses `--json-schema` for structured output.

use super::{
    extract_json, parse_rate_limit_info, wait_with_timeout, AgentBackend, BackendError,
    BackendResult, RawMarker, MARKER_JSON_SCHEMA,
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
    use super::*;

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
}
