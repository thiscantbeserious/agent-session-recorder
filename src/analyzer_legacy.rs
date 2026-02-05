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
    /// The agent name
    pub agent: String,
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
