//! Codex backend implementation.
//!
//! Invokes the Codex CLI with `exec --sandbox read-only` for read-only analysis.
//! Optionally uses `--output-schema` for structured JSON output.

use super::{
    extract_json, get_schema_file_path, parse_rate_limit_info, wait_with_timeout, AgentBackend,
    BackendError, BackendResult, RawMarker,
};
use crate::analyzer::TokenBudget;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Backend for Codex CLI.
///
/// Uses `codex exec --sandbox read-only` for non-interactive analysis.
/// The sandbox flag prevents tool execution for read-only analysis.
#[derive(Debug, Clone, Default)]
pub struct CodexBackend {
    /// Extra CLI arguments to pass to the codex command.
    extra_args: Vec<String>,
}

impl CodexBackend {
    /// Create a new Codex backend with no extra arguments.
    pub fn new() -> Self {
        Self {
            extra_args: Vec::new(),
        }
    }

    /// Create a new Codex backend with extra CLI arguments.
    pub fn with_extra_args(extra_args: Vec<String>) -> Self {
        Self { extra_args }
    }

    /// Get the CLI command name.
    fn command() -> &'static str {
        "codex"
    }
}

impl AgentBackend for CodexBackend {
    fn name(&self) -> &'static str {
        "Codex"
    }

    fn is_available(&self) -> bool {
        super::command_exists(Self::command())
    }

    fn invoke(&self, prompt: &str, timeout: Duration, use_schema: bool) -> BackendResult<String> {
        if !self.is_available() {
            return Err(BackendError::NotAvailable(
                "codex CLI not found in PATH".to_string(),
            ));
        }

        // Build command: run in /tmp to avoid loading project skills/context.
        // --skip-git-repo-check prevents git repo discovery errors.
        // --sandbox read-only prevents any writes (placed AFTER extra_args
        // so user config cannot override the sandbox restriction).
        // When stdout is piped, codex writes the response to stdout and
        // status/thinking to stderr, so no -o flag needed.
        let mut cmd = Command::new(Self::command());
        cmd.args(["exec", "--cd", "/tmp", "--skip-git-repo-check"]);

        // Optionally add schema enforcement (slower but more reliable)
        if use_schema {
            let schema_path = get_schema_file_path()?;
            cmd.arg("--output-schema");
            cmd.arg(&schema_path);
        }

        // Append extra args from per-agent config BEFORE safety flags
        for arg in &self.extra_args {
            cmd.arg(arg);
        }

        // Safety-critical: sandbox must come last to prevent override by extra_args
        cmd.args(["--sandbox", "read-only"]);

        // Pass prompt via stdin to avoid ARG_MAX limits
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

                if output.status.success() || !stdout.trim().is_empty() {
                    Ok(stdout)
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
        TokenBudget::codex()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::backend::MarkerCategory;

    #[test]
    fn codex_backend_name() {
        let backend = CodexBackend::new();
        assert_eq!(backend.name(), "Codex");
    }

    #[test]
    fn codex_backend_token_budget() {
        let backend = CodexBackend::new();
        let budget = backend.token_budget();
        assert_eq!(budget.max_input_tokens, 192_000);
    }

    #[test]
    fn codex_backend_parse_direct_json() {
        let backend = CodexBackend::new();
        let response =
            r#"{"markers": [{"timestamp": 10.0, "label": "Test", "category": "success"}]}"#;

        let markers = backend.parse_response(response).unwrap();
        assert_eq!(markers.len(), 1);
    }

    #[test]
    fn codex_backend_parse_json_in_text() {
        let backend = CodexBackend::new();
        let response = r#"I analyzed the session and here are the markers:

{"markers": [
    {"timestamp": 5.5, "label": "Planning phase started", "category": "planning"},
    {"timestamp": 30.0, "label": "Implementation began", "category": "implementation"}
]}

Let me know if you need more details."#;

        let markers = backend.parse_response(response).unwrap();
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].category, MarkerCategory::Planning);
        assert_eq!(markers[1].category, MarkerCategory::Implementation);
    }

    #[test]
    fn codex_backend_parse_json_in_code_block() {
        let backend = CodexBackend::new();
        let response = r#"Here's the analysis:

```json
{"markers": [
    {"timestamp": 15.0, "label": "Tests started", "category": "implementation"},
    {"timestamp": 25.0, "label": "All tests passed", "category": "success"}
]}
```

Analysis complete."#;

        let markers = backend.parse_response(response).unwrap();
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[1].label, "All tests passed");
    }

    #[test]
    fn codex_backend_parse_plain_code_block() {
        let backend = CodexBackend::new();
        let response = r#"
```
{"markers": [{"timestamp": 5.0, "label": "Error found", "category": "failure"}]}
```
"#;

        let markers = backend.parse_response(response).unwrap();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].category, MarkerCategory::Failure);
    }

    #[test]
    fn codex_backend_parse_empty_markers() {
        let backend = CodexBackend::new();
        let response = r#"No significant events found: {"markers": []}"#;

        let markers = backend.parse_response(response).unwrap();
        assert!(markers.is_empty());
    }

    #[test]
    fn codex_backend_parse_no_json() {
        let backend = CodexBackend::new();
        let response = "I couldn't analyze the session properly.";

        let result = backend.parse_response(response);
        assert!(matches!(result, Err(BackendError::JsonExtraction { .. })));
    }

    #[test]
    fn codex_backend_parse_malformed_json() {
        let backend = CodexBackend::new();
        let response = r#"{"markers": [{"timestamp": "not a number"}]}"#;

        let result = backend.parse_response(response);
        assert!(result.is_err());
    }
}
