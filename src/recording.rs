//! Recording logic for AI agent sessions

use anyhow::{bail, Context, Result};
use chrono::Local;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::analyzer::Analyzer;
use crate::config::Config;
use crate::storage::StorageManager;

/// Session recorder that wraps asciinema
pub struct Recorder {
    #[allow(dead_code)]
    config: Config,
    storage: StorageManager,
    interrupted: Arc<AtomicBool>,
}

impl Recorder {
    /// Create a new recorder with the given config
    pub fn new(config: Config) -> Self {
        let storage = StorageManager::new(config.clone());
        Self {
            config,
            storage,
            interrupted: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Generate a timestamp-based filename
    pub fn generate_filename() -> String {
        let now = Local::now();
        format!("{}.cast", now.format("%Y%m%d-%H%M%S-%3f"))
    }

    /// Sanitize a user-provided filename
    pub fn sanitize_filename(name: &str) -> String {
        let sanitized: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                    c
                } else if c.is_whitespace() {
                    '-'
                } else {
                    '_'
                }
            })
            .collect();

        // Ensure it ends with .cast
        if sanitized.ends_with(".cast") {
            sanitized
        } else {
            format!("{}.cast", sanitized)
        }
    }

    /// Check if asciinema is available
    pub fn check_asciinema() -> Result<()> {
        let output = Command::new("asciinema")
            .arg("--version")
            .output()
            .context("asciinema not found. Please install it first.")?;

        if !output.status.success() {
            bail!("asciinema check failed");
        }

        Ok(())
    }

    /// Record an agent session
    pub fn record(
        &mut self,
        agent: &str,
        session_name: Option<&str>,
        args: &[String],
    ) -> Result<()> {
        Self::check_asciinema()?;

        // Ensure agent directory exists
        let agent_dir = self.storage.ensure_agent_dir(agent)?;

        // Generate filename - use provided name or timestamp-based
        let filename = match session_name {
            Some(name) => Self::sanitize_filename(name),
            None => Self::generate_filename(),
        };
        let filepath = agent_dir.join(&filename);

        // Build the command to run
        let command = if args.is_empty() {
            agent.to_string()
        } else {
            format!("{} {}", agent, args.join(" "))
        };

        // Set up interrupt handler
        let interrupted = self.interrupted.clone();
        ctrlc::set_handler(move || {
            interrupted.store(true, Ordering::SeqCst);
        })
        .ok(); // Ignore if handler already set

        println!("Recording session to: {}", filepath.display());
        println!("Running: {}", command);
        println!();

        // Run asciinema rec
        let status = Command::new("asciinema")
            .arg("rec")
            .arg(&filepath)
            .arg("--title")
            .arg(format!("{} session", agent))
            .arg("-c")
            .arg(&command)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to start asciinema")?;

        println!();

        // Handle exit and get final filepath (may have been renamed)
        let final_filepath = if self.interrupted.load(Ordering::SeqCst) {
            println!("Session interrupted. Saved as: {}", filename);
            filepath.clone()
        } else if status.success() {
            // Skip rename prompt if name was explicitly provided
            if session_name.is_some() {
                println!("Saved as: {}", filename);
                filepath.clone()
            } else {
                // Prompt for rename on normal exit
                self.prompt_rename(&filepath, &filename)?
            }
        } else {
            println!("Session ended with error. Saved as: {}", filename);
            filepath.clone()
        };

        // Run auto-analyze if enabled
        self.maybe_auto_analyze(&final_filepath);

        // Show storage warning if threshold exceeded
        self.show_storage_warning()?;

        Ok(())
    }

    /// Prompt user to rename the session file, returns final filepath
    fn prompt_rename(&self, filepath: &PathBuf, original_filename: &str) -> Result<PathBuf> {
        // Skip prompt if stdin is not a TTY (non-interactive)
        if !atty::is(atty::Stream::Stdin) {
            println!("Saved as: {}", original_filename);
            return Ok(filepath.clone());
        }

        print!(
            "Session complete. Enter a name (or press Enter to keep '{}'): ",
            original_filename
        );
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().lock().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            println!("Keeping filename: {}", original_filename);
            Ok(filepath.clone())
        } else {
            let new_filename = Self::sanitize_filename(input);
            let new_filepath = filepath.parent().unwrap().join(&new_filename);

            if new_filepath.exists() {
                println!("File '{}' already exists. Keeping original.", new_filename);
                Ok(filepath.clone())
            } else {
                std::fs::rename(filepath, &new_filepath).context("Failed to rename file")?;
                println!("Saved as: {}", new_filename);
                Ok(new_filepath)
            }
        }
    }

    /// Show storage warning if threshold exceeded
    fn show_storage_warning(&self) -> Result<()> {
        if self.storage.exceeds_threshold()? {
            let stats = self.storage.get_stats()?;
            eprintln!();
            eprintln!("⚠️  Storage threshold exceeded!");
            eprintln!(
                "   Current: {} ({} sessions)",
                stats.size_human(),
                stats.session_count
            );
            eprintln!("   Run 'asr cleanup' to free space.");
        }
        Ok(())
    }

    /// Run auto-analysis if enabled in config
    fn maybe_auto_analyze(&self, filepath: &Path) {
        if !self.config.recording.auto_analyze {
            return;
        }

        let agent = &self.config.recording.analysis_agent;

        // Check if agent is installed
        if !Analyzer::is_agent_installed(agent) {
            println!();
            println!(
                "Auto-analyze skipped: '{}' not installed. Install it or change analysis_agent in config.",
                agent
            );
            println!(
                "Tip: Run 'agr list' to see recordings, then use your agent's CLI to analyze."
            );
            return;
        }

        println!();
        println!("Analyzing session with {}...", agent);

        let analyzer = Analyzer::new(agent);
        if let Err(e) = analyzer.analyze(filepath) {
            eprintln!("Auto-analyze failed: {}", e);
            println!(
                "Tip: Run 'agr list' to see recordings, then use your agent's CLI to analyze."
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_filename_has_correct_format() {
        let filename = Recorder::generate_filename();
        assert!(filename.ends_with(".cast"));
        // Format: YYYYMMDD-HHMMSS-mmm.cast
        assert!(filename.len() > 20);
        assert!(filename.contains('-'));
    }

    #[test]
    fn sanitize_filename_preserves_valid_chars() {
        assert_eq!(Recorder::sanitize_filename("my-session"), "my-session.cast");
        assert_eq!(Recorder::sanitize_filename("test_123"), "test_123.cast");
        assert_eq!(Recorder::sanitize_filename("file.cast"), "file.cast");
    }

    #[test]
    fn sanitize_filename_replaces_spaces_with_dashes() {
        assert_eq!(Recorder::sanitize_filename("my session"), "my-session.cast");
        assert_eq!(Recorder::sanitize_filename("a b c"), "a-b-c.cast");
    }

    #[test]
    fn sanitize_filename_replaces_special_chars() {
        assert_eq!(Recorder::sanitize_filename("test@#$%"), "test____.cast");
        assert_eq!(Recorder::sanitize_filename("file/name"), "file_name.cast");
    }

    #[test]
    fn sanitize_filename_adds_extension() {
        assert_eq!(Recorder::sanitize_filename("session"), "session.cast");
    }

    #[test]
    fn sanitize_filename_keeps_existing_extension() {
        assert_eq!(Recorder::sanitize_filename("session.cast"), "session.cast");
    }
}
