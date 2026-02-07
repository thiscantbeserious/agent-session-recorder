//! Gemini backend implementation.
//!
//! Invokes the Gemini CLI with `--output-format json --approval-mode plan` for analysis.
//! The `--approval-mode plan` flag enables read-only mode (no tool execution).
//! Note: Gemini CLI does not support JSON schema enforcement.

use super::{
    extract_json, parse_rate_limit_info, wait_with_timeout, AgentBackend, BackendError,
    BackendResult, RawMarker,
};
use crate::analyzer::TokenBudget;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Backend for Gemini CLI.
///
/// Uses `gemini --output-format json --approval-mode plan` for non-interactive analysis.
/// The `--approval-mode plan` ensures read-only operation (no tool execution).
/// Note: Gemini does not support JSON schema enforcement (use_schema is ignored).
#[derive(Debug, Clone, Default)]
pub struct GeminiBackend {
    /// Extra CLI arguments to pass to the gemini command.
    extra_args: Vec<String>,
}

impl GeminiBackend {
    /// Create a new Gemini backend with no extra arguments.
    pub fn new() -> Self {
        Self {
            extra_args: Vec::new(),
        }
    }

    /// Create a new Gemini backend with extra CLI arguments.
    pub fn with_extra_args(extra_args: Vec<String>) -> Self {
        Self { extra_args }
    }

    /// Get the CLI command name.
    fn command() -> &'static str {
        "gemini"
    }
}

impl AgentBackend for GeminiBackend {
    fn name(&self) -> &'static str {
        "Gemini"
    }

    fn is_available(&self) -> bool {
        super::command_exists(Self::command())
    }

    fn invoke(&self, prompt: &str, timeout: Duration, _use_schema: bool) -> BackendResult<String> {
        if !self.is_available() {
            return Err(BackendError::NotAvailable(
                "gemini CLI not found in PATH".to_string(),
            ));
        }

        // Note: Gemini CLI does not support JSON schema enforcement
        // use_schema parameter is ignored

        // Use --approval-mode plan for read-only operation (no tool execution)
        // Safety flags placed AFTER extra_args to prevent override.
        // Pass prompt via stdin to avoid ARG_MAX limits
        let mut cmd = Command::new(Self::command());
        cmd.args(["--output-format", "json"]);

        // Append extra args from per-agent config BEFORE safety flags
        for arg in &self.extra_args {
            cmd.arg(arg);
        }

        // Safety-critical: approval-mode and prompt source must come last
        cmd.args(["--approval-mode", "plan", "--prompt", "-"]);

        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

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
                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).to_string())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                    // Check for rate limiting
                    if let Some(info) = parse_rate_limit_info(&stderr) {
                        return Err(BackendError::RateLimited(info));
                    }

                    Err(BackendError::ExitCode {
                        code: output.status.code().unwrap_or(-1),
                        stderr,
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
        TokenBudget::gemini()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::backend::MarkerCategory;

    #[test]
    fn gemini_backend_name() {
        let backend = GeminiBackend::new();
        assert_eq!(backend.name(), "Gemini");
    }

    #[test]
    fn gemini_backend_token_budget() {
        let backend = GeminiBackend::new();
        let budget = backend.token_budget();
        // Gemini has 1M context window
        assert_eq!(budget.max_input_tokens, 1_000_000);
    }

    #[test]
    fn gemini_backend_parse_valid_response() {
        let backend = GeminiBackend::new();
        let response = r#"{"markers": [
            {"timestamp": 10.0, "label": "Analysis started", "category": "planning"},
            {"timestamp": 120.5, "label": "Design decision made", "category": "design"},
            {"timestamp": 300.0, "label": "Feature complete", "category": "success"}
        ]}"#;

        let markers = backend.parse_response(response).unwrap();
        assert_eq!(markers.len(), 3);
        assert_eq!(markers[0].category, MarkerCategory::Planning);
        assert_eq!(markers[1].category, MarkerCategory::Design);
        assert_eq!(markers[2].category, MarkerCategory::Success);
    }

    #[test]
    fn gemini_backend_parse_empty_markers() {
        let backend = GeminiBackend::new();
        let response = r#"{"markers": []}"#;

        let markers = backend.parse_response(response).unwrap();
        assert!(markers.is_empty());
    }

    #[test]
    fn gemini_backend_parse_with_whitespace() {
        let backend = GeminiBackend::new();
        let response = r#"

        {
            "markers": [
                {"timestamp": 5.0, "label": "Test marker", "category": "implementation"}
            ]
        }

        "#;

        let markers = backend.parse_response(response).unwrap();
        assert_eq!(markers.len(), 1);
    }

    #[test]
    fn gemini_backend_parse_invalid_json() {
        let backend = GeminiBackend::new();
        let response = "This is not valid JSON";

        let result = backend.parse_response(response);
        assert!(result.is_err());
    }

    #[test]
    fn gemini_backend_large_context_budget() {
        let backend = GeminiBackend::new();
        let budget = backend.token_budget();

        // Gemini should have much more available than Claude/Codex
        let available = budget.available_for_content();
        assert!(available > 800_000); // Should be around 841K
    }
}
