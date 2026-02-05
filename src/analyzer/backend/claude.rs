//! Claude backend implementation.
//!
//! Invokes the Claude CLI with `--print --output-format json --tools ""` for analysis.
//! Disabling tools ensures Claude responds directly without trying to execute commands.

use super::{
    extract_json, parse_rate_limit_info, AgentBackend, BackendError, BackendResult, RawMarker,
};
use crate::analyzer::TokenBudget;
use serde::Deserialize;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Backend for Claude CLI.
///
/// Uses `claude --print --output-format json --tools ""` for non-interactive analysis.
/// Tools are disabled to ensure Claude just responds with text/JSON.
#[derive(Debug, Clone, Default)]
pub struct ClaudeBackend;

impl ClaudeBackend {
    /// Create a new Claude backend.
    pub fn new() -> Self {
        Self
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

    fn invoke(&self, prompt: &str, timeout: Duration) -> BackendResult<String> {
        if !self.is_available() {
            return Err(BackendError::NotAvailable(
                "claude CLI not found in PATH".to_string(),
            ));
        }

        // Use --tools "" to disable all tools and get direct text responses.
        // This prevents Claude from trying to execute tools and speeds up responses.
        let mut child = Command::new(Self::command())
            .args([
                "--print",
                "--output-format",
                "json",
                "--tools",
                "",
                "-p",
            ])
            .arg(prompt)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Wait with timeout
        let timeout_secs = timeout.as_secs();
        let result = wait_with_timeout(&mut child, timeout_secs);

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
                    // (in the JSON wrapper with is_error: true)
                    let error_msg = extract_error_from_claude_response(&stdout)
                        .unwrap_or(stderr);

                    Err(BackendError::ExitCode {
                        code: output.status.code().unwrap_or(-1),
                        stderr: error_msg,
                    })
                }
            }
            Err(_) => {
                // Kill the process if timeout
                let _ = child.kill();
                Err(BackendError::Timeout(timeout))
            }
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
///
/// When Claude CLI exits with non-zero status, the error details may be
/// in stdout as JSON with `is_error: true`.
fn extract_error_from_claude_response(stdout: &str) -> Option<String> {
    let wrapper: ClaudeErrorWrapper = serde_json::from_str(stdout.trim()).ok()?;

    if wrapper.is_error == Some(true) {
        wrapper.result.or_else(|| Some("Claude returned an error".to_string()))
    } else {
        // Not an error response, check for result content that might explain the issue
        wrapper.result.filter(|r| !r.is_empty())
    }
}

/// Wait for child process with timeout.
///
/// Uses a simple polling approach since std::process doesn't have
/// native timeout support.
fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout_secs: u64,
) -> std::io::Result<std::process::Output> {
    use std::thread;
    use std::time::Instant;

    let start = Instant::now();
    let poll_interval = Duration::from_millis(100);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Process finished
                let stdout = child
                    .stdout
                    .take()
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        std::io::Read::read_to_end(&mut s, &mut buf).ok();
                        buf
                    })
                    .unwrap_or_default();

                let stderr = child
                    .stderr
                    .take()
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        std::io::Read::read_to_end(&mut s, &mut buf).ok();
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
                // Still running
                if start.elapsed().as_secs() >= timeout_secs {
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

    // Note: Integration tests with actual CLI would go in tests/integration/
}
