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

use crate::analyzer::{AgentType, AnalyzeOptions, AnalyzerService};
use crate::config::Config;
use crate::files::{backup, filename, lock};
use crate::storage::StorageManager;
use crate::theme;
use crate::utils::process_guard::ProcessGuard;

/// Session recorder that wraps asciinema
pub struct Recorder {
    #[allow(dead_code)]
    config: Config,
    storage: StorageManager,
    guard: ProcessGuard,
}

impl Recorder {
    /// Create a new recorder with the given config
    pub fn new(config: Config) -> Self {
        let storage = StorageManager::new(config.clone());
        Self {
            config,
            storage,
            guard: ProcessGuard::new(),
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

        // Lock the file before recording starts
        lock::create_lock(&filepath)?;

        // Build the command to run
        let command = if args.is_empty() {
            agent.to_string()
        } else {
            format!("{} {}", agent, args.join(" "))
        };

        // Set up signal handlers for clean shutdown (SIGINT + SIGHUP)
        self.guard.register_signal_handlers();

        theme::print_start_banner();
        theme::print_box_line(&format!("  ⏺ {}/{}", agent, filename));
        theme::print_box_bottom();
        println!();

        // Spawn asciinema rec (spawn + poll so we can react to signals)
        let mut child = match Command::new("asciinema")
            .arg("rec")
            .arg(&filepath)
            .arg("--title")
            .arg(format!("{} session", agent))
            .arg("-c")
            .arg(&command)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                lock::remove_lock(&filepath);
                return Err(anyhow::Error::new(e).context("Failed to start asciinema"));
            }
        };

        let status = match self.guard.wait_or_kill(&mut child) {
            Ok(s) => s,
            Err(e) => {
                lock::remove_lock(&filepath);
                return Err(e);
            }
        };

        println!();
        theme::print_done_banner();

        // Capture file identity for recovery if file gets moved
        let inode = Self::capture_inode(&filepath);
        let header = Self::read_header_line(&filepath);

        // Recording is done - remove lock after capturing identity
        lock::remove_lock(&filepath);

        // Handle exit and get final filepath (may have been renamed)
        let final_filepath = if self.guard.is_interrupted() {
            theme::print_box_line(&format!("  ⏹ {}", filename));
            theme::print_box_bottom();
            filepath.clone()
        } else if status.success() {
            // Skip rename prompt if name was explicitly provided
            if session_name.is_some() {
                theme::print_box_line(&format!("  ⏹ {}", filename));
                theme::print_box_bottom();
                filepath.clone()
            } else {
                // Prompt for rename on normal exit (non-fatal)
                match self.prompt_rename(&filepath, &filename, inode, &header, &agent_dir) {
                    Ok(path) => path,
                    Err(e) => {
                        eprintln!("  \u{26a0} Rename failed: {}", e);
                        filepath.clone()
                    }
                }
            }
        } else {
            theme::print_box_line(&format!("  ⏹ {} (error)", filename));
            theme::print_box_bottom();
            filepath.clone()
        };

        // Run auto-analyze if enabled
        self.maybe_auto_analyze(&final_filepath);

        // Show storage warning if threshold exceeded
        self.show_storage_warning()?;

        Ok(())
    }

    /// Prompt user to rename the session file, returns final filepath.
    ///
    /// Performs recovery if the file was moved during recording.
    fn prompt_rename(
        &self,
        filepath: &Path,
        original_filename: &str,
        inode: Option<u64>,
        header: &Option<String>,
        agent_dir: &Path,
    ) -> Result<PathBuf> {
        // Resolve actual file path - may have been moved during recording
        let actual_path = Self::resolve_actual_path(filepath, inode, header, agent_dir);

        // Skip prompt if stdin is not a TTY (non-interactive)
        if !atty::is(atty::Stream::Stdin) {
            theme::print_box_line(&format!("  \u{23f9} {}", original_filename));
            theme::print_box_bottom();
            return Ok(actual_path);
        }

        // Show current filename (might differ from original if file was moved)
        let display_name = actual_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(original_filename);

        theme::print_box_line(&format!("  \u{23f9} {}", display_name));
        theme::print_box_bottom();
        print!("  \u{23ce} Rename: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().lock().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            Ok(actual_path)
        } else {
            let new_filename = Self::sanitize_filename(input);
            let new_filepath = actual_path.parent().unwrap().join(&new_filename);

            if new_filepath.exists() {
                println!("  \u{26a0} Exists, kept original");
                Ok(actual_path)
            } else {
                std::fs::rename(&actual_path, &new_filepath).context("Failed to rename file")?;
                println!("  \u{2713} {}", new_filename);
                Ok(new_filepath)
            }
        }
    }

    /// Capture the inode of a file for later recovery if it gets renamed.
    #[cfg(unix)]
    fn capture_inode(path: &Path) -> Option<u64> {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path).ok().map(|m| m.ino())
    }

    /// Capture the inode of a file (stub for non-Unix platforms).
    #[cfg(not(unix))]
    fn capture_inode(_path: &Path) -> Option<u64> {
        None
    }

    /// Read the first line of a cast file for header fingerprint recovery.
    fn read_header_line(path: &Path) -> Option<String> {
        use std::io::BufReader;
        let file = std::fs::File::open(path).ok()?;
        let mut reader = BufReader::new(file);
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let trimmed = line.trim_end().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    /// Resolve the actual file path, recovering from renames during recording.
    ///
    /// Tries: original path -> inode scan -> header fingerprint -> original (with warning).
    fn resolve_actual_path(
        filepath: &Path,
        inode: Option<u64>,
        header: &Option<String>,
        agent_dir: &Path,
    ) -> PathBuf {
        if filepath.exists() {
            return filepath.to_path_buf();
        }

        eprintln!("  \u{26a0} Recording file was moved during session");

        // Try inode-based recovery
        #[cfg(unix)]
        if let Some(ino) = inode {
            if let Some(found) = lock::find_by_inode(agent_dir, ino) {
                eprintln!("  \u{2713} Found at: {}", found.display());
                return found;
            }
        }

        // Try header fingerprint recovery
        if let Some(ref hdr) = header {
            if let Some(found) = lock::find_by_header(agent_dir, hdr) {
                eprintln!("  \u{2713} Found at: {}", found.display());
                return found;
            }
        }

        // File not found - warn and suggest backup
        let bak = backup::backup_path_for(filepath);
        if bak.exists() {
            eprintln!("  Backup available at: {}", bak.display());
        }
        filepath.to_path_buf()
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
