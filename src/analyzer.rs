//! Auto-analysis of recording sessions using AI agents

use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

/// Prompt template for session analysis
const ANALYSIS_PROMPT: &str = r#"Analyze the terminal recording at {filepath} and mark ONLY the most important moments.

CONSTRAINTS:
- Maximum 5-7 markers total (fewer for short sessions)
- Skip routine/expected output
- Never mark similar events twice

MARK THESE (in priority order):
1. ERRORS: Exceptions, stack traces, failed commands, unexpected exit codes
2. MILESTONES: Build success, tests passed, deploy complete
3. KEY DECISIONS: Configuration changes, important user choices

DO NOT MARK:
- Directory listings (ls output)
- Help text (--help, --version)
- Normal command prompts ($ )
- Repeated similar messages
- Status updates without errors
- Expected/routine output

PROCESS:
1. First, read the entire .cast file to understand the session
2. Identify candidate moments from the categories above
3. Select only the TOP 5-7 most significant
4. Skip any that duplicate previous markers
5. For each selected moment:
   agr marker add {filepath} <timestamp> "brief description"

Example markers:
  agr marker add {filepath} 45.2 "ERROR: Build failed - missing dependency"
  agr marker add {filepath} 120.5 "MILESTONE: All 47 tests passed"
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
    fn gemini_agent_name_is_preserved() {
        // Verify the analyzer stores the agent name as provided
        let analyzer = Analyzer::new("gemini");
        assert_eq!(analyzer.agent, "gemini");
    }

    #[test]
    fn build_prompt_contains_constraints() {
        let analyzer = Analyzer::new("claude");
        let filepath = PathBuf::from("/tmp/session.cast");
        let prompt = analyzer.build_prompt(&filepath);

        // Verify prompt includes marker limit constraint
        assert!(
            prompt.contains("Maximum 5-7 markers"),
            "Prompt should contain marker limit"
        );
        assert!(
            prompt.contains("CONSTRAINTS"),
            "Prompt should have CONSTRAINTS section"
        );
    }

    #[test]
    fn build_prompt_contains_negative_examples() {
        let analyzer = Analyzer::new("claude");
        let filepath = PathBuf::from("/tmp/session.cast");
        let prompt = analyzer.build_prompt(&filepath);

        // Verify prompt includes "DO NOT MARK" section
        assert!(
            prompt.contains("DO NOT MARK"),
            "Prompt should contain DO NOT MARK section"
        );
        assert!(
            prompt.contains("Directory listings"),
            "Prompt should mention directory listings"
        );
        assert!(
            prompt.contains("Help text"),
            "Prompt should mention help text"
        );
    }

    #[test]
    fn build_prompt_contains_priority_categories() {
        let analyzer = Analyzer::new("claude");
        let filepath = PathBuf::from("/tmp/session.cast");
        let prompt = analyzer.build_prompt(&filepath);

        // Verify priority categories are present
        assert!(prompt.contains("ERRORS"), "Prompt should mention ERRORS");
        assert!(
            prompt.contains("MILESTONES"),
            "Prompt should mention MILESTONES"
        );
        assert!(
            prompt.contains("KEY DECISIONS"),
            "Prompt should mention KEY DECISIONS"
        );
    }

    #[test]
    fn build_prompt_contains_example_markers() {
        let analyzer = Analyzer::new("claude");
        let filepath = PathBuf::from("/tmp/session.cast");
        let prompt = analyzer.build_prompt(&filepath);

        // Verify example markers include error and milestone prefixes
        assert!(
            prompt.contains("ERROR:"),
            "Prompt should have ERROR: example"
        );
        assert!(
            prompt.contains("MILESTONE:"),
            "Prompt should have MILESTONE: example"
        );
    }

    #[test]
    fn analyze_returns_error_for_missing_agent() {
        let analyzer = Analyzer::new("definitely-not-installed-agent-xyz");
        let filepath = PathBuf::from("/tmp/session.cast");

        let result = analyzer.analyze(&filepath);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not installed"),
            "Error should mention agent not installed"
        );
    }
}
