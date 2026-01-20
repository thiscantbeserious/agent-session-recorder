//! Auto-analysis of recording sessions using AI agents

use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

/// Prompt template for session analysis
const ANALYSIS_PROMPT: &str = r#"Analyze the recording at {filepath} and add markers for key moments.

Instructions:
1. Read the .cast file at the path above
2. Parse JSON lines - look for output events (type "o")
3. Calculate absolute timestamps by summing up the times from the start
4. Identify key moments:
   - Errors, exceptions, stack traces
   - Important commands executed
   - Decisions or turning points
   - Significant results
5. For each moment, run:
   agr marker add {filepath} <timestamp_seconds> "description"

The timestamp is cumulative seconds from recording start.

Example:
  agr marker add {filepath} 45.2 "Build failed: missing dependency"
  agr marker add {filepath} 120.5 "All tests passed"
"#;

/// Analyzer spawns AI agents to analyze session recordings
pub struct Analyzer {
    agent: String,
}

impl Analyzer {
    /// Create a new analyzer for the specified agent
    pub fn new(agent: &str) -> Self {
        Self {
            agent: agent.to_string(),
        }
    }

    /// Check if the specified agent CLI is installed
    pub fn is_agent_installed(agent: &str) -> bool {
        let binary = match agent {
            "gemini-cli" | "gemini" => "gemini",
            other => other,
        };
        Command::new(binary)
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Build the analysis prompt for a given cast file
    pub fn build_prompt(&self, cast_file: &Path) -> String {
        ANALYSIS_PROMPT.replace("{filepath}", &cast_file.display().to_string())
    }

    /// Analyze a recording file using the configured agent
    pub fn analyze(&self, cast_file: &Path) -> Result<()> {
        if !Self::is_agent_installed(&self.agent) {
            bail!(
                "Analysis agent '{}' is not installed. Skipping auto-analyze.",
                self.agent
            );
        }

        let prompt = self.build_prompt(cast_file);
        self.spawn_agent(&prompt)
    }

    /// Spawn the agent CLI with the given prompt
    fn spawn_agent(&self, prompt: &str) -> Result<()> {
        let status = match self.agent.as_str() {
            "claude" => Command::new("claude").args(["-p", prompt]).status(),
            "codex" => Command::new("codex").args(["exec", prompt]).status(),
            "gemini-cli" | "gemini" => Command::new("gemini").args(["-p", prompt]).status(),
            _ => bail!("Unknown analysis agent: {}", self.agent),
        };

        match status {
            Ok(exit_status) if exit_status.success() => Ok(()),
            Ok(exit_status) => bail!(
                "Analysis agent '{}' exited with code: {:?}",
                self.agent,
                exit_status.code()
            ),
            Err(e) => bail!("Failed to spawn analysis agent '{}': {}", self.agent, e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn analyzer_new_sets_agent() {
        let analyzer = Analyzer::new("claude");
        assert_eq!(analyzer.agent, "claude");
    }

    #[test]
    fn build_prompt_includes_filepath() {
        let analyzer = Analyzer::new("claude");
        let filepath = PathBuf::from("/home/user/sessions/test.cast");
        let prompt = analyzer.build_prompt(&filepath);

        assert!(prompt.contains("/home/user/sessions/test.cast"));
        assert!(prompt.contains("agr marker add"));
    }

    #[test]
    fn build_prompt_replaces_all_placeholders() {
        let analyzer = Analyzer::new("claude");
        let filepath = PathBuf::from("/tmp/session.cast");
        let prompt = analyzer.build_prompt(&filepath);

        // Should not contain any unreplaced placeholders
        assert!(!prompt.contains("{filepath}"));
        // Count occurrences of the path - should be multiple
        let count = prompt.matches("/tmp/session.cast").count();
        assert!(count >= 3, "Expected at least 3 occurrences of filepath");
    }

    #[test]
    fn is_agent_installed_returns_false_for_missing() {
        // Test with a binary that definitely doesn't exist
        let result = Analyzer::is_agent_installed("definitely-not-a-real-binary-12345");
        assert!(!result);
    }

    #[test]
    fn gemini_cli_maps_to_gemini_binary() {
        // gemini-cli should check for "gemini" binary
        // We can't easily test the actual mapping without mocking,
        // but we can verify the analyzer accepts both names
        let analyzer1 = Analyzer::new("gemini-cli");
        let analyzer2 = Analyzer::new("gemini");
        assert_eq!(analyzer1.agent, "gemini-cli");
        assert_eq!(analyzer2.agent, "gemini");
    }
}
