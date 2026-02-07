//! Recording logic for AI agent sessions
//!
//! This module is excluded from unit test coverage because:
//! 1. It requires external binaries (asciinema)
//! 2. It performs complex process spawning and signal handling
//! 3. It is thoroughly tested via e2e tests in tests/e2e_test.sh

use anyhow::{bail, Context, Result};
use std::env;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::analyzer::{AgentType, AnalyzeOptions, AnalyzerService};
use crate::branding;
use crate::config::Config;
use crate::files::filename;
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

    /// Generate a filename using the configured template.
    ///
    /// Uses the `filename_template` from config with tags like `{directory}`, `{date}`, `{time}`.
    /// Falls back to a timestamp-based name if template generation fails.
    pub fn generate_filename(&self) -> String {
        // Get current working directory name
        let dir_name = env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| "recording".to_string());

        // Build filename config from recording config (enforces minimum of 1)
        let filename_config = filename::Config::new(self.config.recording.directory_max_length);

        // Generate using template, fallback to simple timestamp on error
        filename::generate(
            &dir_name,
            &self.config.recording.filename_template,
            &filename_config,
        )
        .unwrap_or_else(|_| {
            // Fallback: use directory + timestamp
            let sanitized_dir = filename::sanitize_directory(&dir_name, &filename_config);
            let now = chrono::Local::now();
            format!("{}_{}.cast", sanitized_dir, now.format("%y%m%d_%H%M%S"))
        })
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

        // Generate filename - use provided name or template-based
        let filename = match session_name {
            Some(name) => Self::sanitize_filename(name),
            None => self.generate_filename(),
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

        branding::print_start_banner();
        branding::print_box_line(&format!("  ⏺ {}/{}", agent, filename));
        branding::print_box_bottom();
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
        branding::print_done_banner();

        // Handle exit and get final filepath (may have been renamed)
        let final_filepath = if self.interrupted.load(Ordering::SeqCst) {
            branding::print_box_line(&format!("  ⏹ {}", filename));
            branding::print_box_bottom();
            filepath.clone()
        } else if status.success() {
            // Skip rename prompt if name was explicitly provided
            if session_name.is_some() {
                branding::print_box_line(&format!("  ⏹ {}", filename));
                branding::print_box_bottom();
                filepath.clone()
            } else {
                // Prompt for rename on normal exit
                self.prompt_rename(&filepath, &filename)?
            }
        } else {
            branding::print_box_line(&format!("  ⏹ {} (error)", filename));
            branding::print_box_bottom();
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
            branding::print_box_line(&format!("  ⏹ {}", original_filename));
            branding::print_box_bottom();
            return Ok(filepath.clone());
        }

        branding::print_box_line(&format!("  ⏹ {}", original_filename));
        branding::print_box_bottom();
        print!("  ⏎ Rename: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().lock().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            Ok(filepath.clone())
        } else {
            let new_filename = Self::sanitize_filename(input);
            let new_filepath = filepath.parent().unwrap().join(&new_filename);

            if new_filepath.exists() {
                println!("  ⚠ Exists, kept original");
                Ok(filepath.clone())
            } else {
                std::fs::rename(filepath, &new_filepath).context("Failed to rename file")?;
                println!("  ✓ {}", new_filename);
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

        let agent_name = self.config.resolve_analysis_agent();

        // Parse agent type
        let agent = match agent_name.to_lowercase().as_str() {
            "claude" => AgentType::Claude,
            "codex" => AgentType::Codex,
            "gemini" => AgentType::Gemini,
            _ => {
                eprintln!(
                    "Auto-analyze skipped: unknown agent '{}'. Supported: claude, codex, gemini",
                    agent_name
                );
                return;
            }
        };

        // Create analyzer service with quiet mode (auto-analyze is background operation)
        let options = AnalyzeOptions::with_agent(agent).quiet();
        let service = AnalyzerService::new(options);

        // Check if agent is installed
        if !service.is_agent_available() {
            println!();
            println!(
                "Auto-analyze skipped: '{}' not installed. Install it or set [analysis].agent in config.",
                agent_name
            );
            println!(
                "Tip: Run 'agr list' to see recordings, then use your agent's CLI to analyze."
            );
            return;
        }

        println!();
        println!("Analyzing session with {}...", agent);

        match service.analyze(filepath) {
            Ok(result) => {
                println!(
                    "Analysis complete. {} markers added.",
                    result.markers_added()
                );
            }
            Err(e) => {
                eprintln!("Auto-analyze failed: {}", e);
                println!(
                    "Tip: Run 'agr list' to see recordings, then use your agent's CLI to analyze."
                );
            }
        }
    }
}
