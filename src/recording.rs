//! Recording logic for AI agent sessions
//!
//! This module is excluded from unit test coverage because:
//! 1. It requires external binaries (asciinema)
//! 2. It performs complex process spawning and signal handling
//! 3. It is thoroughly tested via e2e tests in tests/e2e_test.sh

mod finalize;
pub use finalize::finalize_recording;

use anyhow::{bail, Context, Result};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

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
    /// Uses the `filename_template` from config with tags like `{directory}`,
    /// `{date}`, `{time}`, `{branch}`, `{id}`.
    /// Falls back to a timestamp-based name if template generation fails.
    pub fn generate_filename(&self) -> String {
        let dir_name = env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| "recording".to_string());

        let branch = detect_git_branch();
        let filename_config = filename::Config::new(self.config.recording.directory_max_length);
        let ctx = filename::RenderContext {
            directory: &dir_name,
            branch: branch.as_deref(),
        };

        filename::generate(
            &ctx,
            &self.config.recording.filename_template,
            &filename_config,
        )
        .unwrap_or_else(|_| {
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
        let agent_dir = self.storage.ensure_agent_dir(agent)?;
        let filename = self.unique_filename(&agent_dir, session_name)?;
        let filepath = agent_dir.join(&filename);
        lock::create_lock(&filepath)?;
        self.guard.register_signal_handlers();
        Self::print_start(agent, &filename);
        let status = self.run_asciinema(agent, args, &filepath)?;
        println!();
        theme::print_done_banner();
        let final_path = self.finish(&filepath, &agent_dir, &filename, status);
        self.maybe_auto_analyze(&final_path);
        self.show_storage_warning()
    }

    fn print_start(agent: &str, filename: &str) {
        theme::print_start_banner();
        theme::print_box_line(&format!("  ⏺ {}/{}", agent, filename));
        theme::print_box_bottom();
        println!();
    }

    /// Provided or template-based filename, with same-second collisions resolved.
    fn unique_filename(&self, agent_dir: &Path, session_name: Option<&str>) -> Result<String> {
        let base_filename = match session_name {
            Some(name) => Self::sanitize_filename(name),
            None => self.generate_filename(),
        };
        filename::resolve_collision(agent_dir, &base_filename).ok_or_else(|| {
            anyhow::anyhow!("All collision suffixes exhausted for {}", base_filename)
        })
    }

    fn asciinema_command(agent: &str, args: &[String], filepath: &Path) -> Command {
        let command = [agent].into_iter().chain(args.iter().map(String::as_str));
        let mut cmd = Command::new("asciinema");
        cmd.args(["rec", "--quiet"]).arg(filepath);
        cmd.arg("--title").arg(format!("{} session", agent));
        cmd.arg("-c").arg(command.collect::<Vec<_>>().join(" "));
        cmd.stdin(Stdio::inherit()).stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());
        cmd
    }

    /// Spawn `asciinema rec` and wait (poll, so signals are handled). Removes the lock on error.
    fn run_asciinema(&self, agent: &str, args: &[String], filepath: &Path) -> Result<ExitStatus> {
        let mut child = Self::asciinema_command(agent, args, filepath)
            .spawn()
            .map_err(|e| {
                lock::remove_lock(filepath);
                anyhow::Error::new(e).context("Failed to start asciinema")
            })?;
        self.guard.wait_or_kill(&mut child).inspect_err(|_| {
            lock::remove_lock(filepath);
        })
    }

    /// Remove the lock, recover a moved file, compress it and print the final line.
    fn finish(
        &self,
        filepath: &Path,
        agent_dir: &Path,
        filename: &str,
        status: ExitStatus,
    ) -> PathBuf {
        // Capture file identity for recovery if file gets moved
        let inode = Self::capture_inode(filepath);
        let header = Self::read_header_line(filepath);
        lock::remove_lock(filepath);
        let actual_path = Self::resolve_actual_path(filepath, inode, &header, agent_dir);
        let final_path = finalize_recording(&actual_path, &self.config);
        let failed = !status.success() && !self.guard.is_interrupted();
        Self::print_stop(&final_path, filename, failed);
        final_path
    }

    fn print_stop(final_path: &Path, filename: &str, failed: bool) {
        let name = final_path.file_name().and_then(std::ffi::OsStr::to_str);
        let suffix = if failed { " (error)" } else { "" };
        theme::print_box_line(&format!(
            "  \u{23f9} {}{}",
            name.unwrap_or(filename),
            suffix
        ));
        theme::print_box_bottom();
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
        use std::io::{BufRead, BufReader};
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
        #[cfg(not(unix))]
        let _ = inode; // inode recovery is unix only

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
            eprintln!("   Run 'agr cleanup' to free space.");
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

/// Detects the current git branch name, returning None if not in a git repo.
///
/// Returns None if: git is not installed, not inside a git repo, or HEAD is detached.
pub fn detect_git_branch() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if branch.is_empty() || branch == "HEAD" {
        return None;
    }

    Some(branch)
}
