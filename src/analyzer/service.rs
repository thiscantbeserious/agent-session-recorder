//! AnalyzerService facade for orchestrating analysis operations.
//!
//! This module provides the main entry point for analyzing cast files.
//! The `AnalyzerService` orchestrates all components from Stages 1-6.
//!
//! # Workflow
//!
//! 1. Parse cast file
//! 2. Check for existing markers (warn if present)
//! 3. Extract content (Stage 1)
//! 4. Calculate chunks (Stage 2)
//! 5. Execute parallel analysis (Stage 3+4)
//! 6. Aggregate results (Stage 5)
//! 7. Write markers to file
//! 8. Report summary (Stage 6)

use std::path::Path;
use std::time::Duration;

use crate::asciicast::AsciicastFile;

use super::backend::{AgentBackend, AgentType};
use super::chunk::{ChunkCalculator, ChunkConfig};
use super::config::ExtractionConfig;
use super::error::AnalysisError;
use super::extractor::ContentExtractor;
use super::progress::DefaultProgressReporter;
use super::prompt::{
    build_analyze_prompt, build_curation_prompt, build_rename_prompt, extract_rename_response,
};
use super::result::{MarkerWriter, ResultAggregator, ValidatedMarker, WriteReport};
use super::tracker::UsageSummary;
use super::worker::{ProgressReporter, RetryExecutor, WorkerConfig, WorkerScaler};

/// Default timeout for agent invocations in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// Configuration options for analysis.
#[derive(Debug, Clone)]
pub struct AnalyzeOptions {
    /// Agent to use for analysis
    pub agent: AgentType,
    /// Number of workers (None = auto-scale)
    pub workers: Option<usize>,
    /// Timeout per chunk in seconds
    pub timeout_secs: u64,
    /// Disable parallel processing
    pub no_parallel: bool,
    /// Quiet mode (suppress progress output)
    pub quiet: bool,
    /// Debug mode (save cleaned content and stop)
    pub debug: bool,
    /// Output path for cleaned content (optional)
    pub output_path: Option<String>,
    /// Fast mode (skip JSON schema enforcement)
    pub fast: bool,
    /// Extra CLI arguments to pass to the agent backend (for analysis)
    pub extra_args: Vec<String>,
    /// Extra CLI arguments for curation (falls back to `extra_args` if empty)
    pub curate_extra_args: Vec<String>,
    /// Extra CLI arguments for rename (falls back to `extra_args` if empty)
    pub rename_extra_args: Vec<String>,
    /// Override the token budget for chunk calculation
    pub token_budget_override: Option<usize>,
}

impl Default for AnalyzeOptions {
    fn default() -> Self {
        Self {
            agent: AgentType::Claude,
            workers: None,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            no_parallel: false,
            quiet: false,
            debug: false,
            output_path: None,
            fast: false,
            extra_args: Vec::new(),
            curate_extra_args: Vec::new(),
            rename_extra_args: Vec::new(),
            token_budget_override: None,
        }
    }
}

impl AnalyzeOptions {
    /// Create options for a specific agent.
    pub fn with_agent(agent: AgentType) -> Self {
        Self {
            agent,
            ..Default::default()
        }
    }

    /// Set worker count override.
    pub fn workers(mut self, count: usize) -> Self {
        self.workers = Some(count);
        self
    }

    /// Set timeout per chunk.
    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Disable parallel processing.
    pub fn sequential(mut self) -> Self {
        self.no_parallel = true;
        self
    }

    /// Enable quiet mode.
    pub fn quiet(mut self) -> Self {
        self.quiet = true;
        self
    }

    /// Enable debug mode.
    pub fn debug(mut self, enabled: bool) -> Self {
        self.debug = enabled;
        self
    }

    /// Set output path for cleaned content.
    pub fn output(mut self, path: String) -> Self {
        self.output_path = Some(path);
        self
    }

    /// Enable fast mode (skip JSON schema enforcement).
    pub fn fast(mut self, enabled: bool) -> Self {
        self.fast = enabled;
        self
    }

    /// Set extra CLI arguments to pass to the agent backend.
    pub fn extra_args(mut self, args: Vec<String>) -> Self {
        self.extra_args = args;
        self
    }

    /// Set extra CLI arguments for curation tasks.
    pub fn curate_extra_args(mut self, args: Vec<String>) -> Self {
        self.curate_extra_args = args;
        self
    }

    /// Set extra CLI arguments for rename tasks.
    pub fn rename_extra_args(mut self, args: Vec<String>) -> Self {
        self.rename_extra_args = args;
        self
    }

    /// Set token budget override for chunk calculation.
    pub fn token_budget_override(mut self, budget: usize) -> Self {
        self.token_budget_override = Some(budget);
        self
    }
}

/// Result of an analysis operation.
#[derive(Debug)]
pub struct AnalysisResult {
    /// Markers that were added to the file
    pub markers: Vec<ValidatedMarker>,
    /// Write report with details
    pub write_report: WriteReport,
    /// Usage summary for visibility
    pub usage_summary: UsageSummary,
    /// Whether existing markers were found
    pub had_existing_markers: bool,
    /// Number of existing markers before analysis
    pub existing_marker_count: usize,
    /// Total duration of the recording in seconds
    pub total_duration: f64,
}

impl AnalysisResult {
    /// Number of markers added.
    pub fn markers_added(&self) -> usize {
        self.write_report.markers_written
    }

    /// Check if analysis was successful.
    pub fn is_success(&self) -> bool {
        self.usage_summary.successful_chunks > 0
    }

    /// Check if analysis was partial (some chunks failed).
    pub fn is_partial(&self) -> bool {
        self.usage_summary.failed_chunks > 0 && self.usage_summary.successful_chunks > 0
    }
}

/// Main service for analyzing cast files.
///
/// Facade pattern - coordinates all analysis components.
pub struct AnalyzerService {
    options: AnalyzeOptions,
    backend: Box<dyn AgentBackend>,
}

impl AnalyzerService {
    /// Create a new analyzer service with options.
    pub fn new(options: AnalyzeOptions) -> Self {
        let backend = options.agent.create_backend(options.extra_args.clone());
        Self { options, backend }
    }

    /// Create with a custom backend (for testing).
    pub fn with_backend(options: AnalyzeOptions, backend: Box<dyn AgentBackend>) -> Self {
        Self { options, backend }
    }

    /// Create a backend with task-specific extra args.
    ///
    /// Falls back to the main `extra_args` if the task-specific args are empty.
    fn backend_for_args(&self, task_args: &[String]) -> Box<dyn AgentBackend> {
        let args = if task_args.is_empty() {
            self.options.extra_args.clone()
        } else {
            task_args.to_vec()
        };
        self.options.agent.create_backend(args)
    }

    /// Check if the configured agent is available.
    pub fn is_agent_available(&self) -> bool {
        self.backend.is_available()
    }

    /// Analyze a cast file and add markers.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the cast file
    ///
    /// # Returns
    ///
    /// Analysis result with markers and statistics.
    pub fn analyze<P: AsRef<Path>>(&self, path: P) -> Result<AnalysisResult, AnalysisError> {
        let path = path.as_ref();

        // 1. Parse cast file
        let mut cast = AsciicastFile::parse(path).map_err(|e| AnalysisError::IoError {
            operation: "reading cast file".to_string(),
            message: e.to_string(),
        })?;

        // 2. Check for existing markers
        let (had_existing_markers, existing_marker_count) =
            MarkerWriter::has_existing_markers(&cast);
        if had_existing_markers && !self.options.quiet {
            eprintln!(
                "Warning: File already contains {} marker(s). New markers will be added alongside existing ones.",
                existing_marker_count
            );
        }

        // 3. Extract content (Stage 1)
        let config = ExtractionConfig::default();
        let extractor = ContentExtractor::new(config);
        let (cols, rows) = cast.terminal_size();
        let content = extractor.extract(&mut cast.events, cols as usize, rows as usize);

        // Show extraction stats (before NoContent check so --debug always sees them)
        if !self.options.quiet {
            let stats = &content.stats;
            let compression = if stats.original_bytes > 0 {
                100.0 - (stats.extracted_bytes as f64 / stats.original_bytes as f64 * 100.0)
            } else {
                0.0
            };

            eprintln!("\nExtraction Summary:");
            eprintln!(
                "──────────────────────────────────────────────────────────────────────────────"
            );
            eprintln!(
                "  Size Reduction:    {:>8}KB → {:>8}KB ({:.1}%)",
                stats.original_bytes / 1024,
                stats.extracted_bytes / 1024,
                compression
            );
            eprintln!(
                "──────────────────────────────────────────────────────────────────────────────"
            );
            eprintln!(
                "  Redraw Cleanup:    {:>8} redraw frames coalesced",
                stats.events_coalesced
            );
            eprintln!(
                "                     {:>8} status lines deduped",
                stats.windowed_lines_deduped
            );
            eprintln!(
                "  Content Pruning:   {:>8} redundant lines removed",
                stats.global_lines_deduped
            );
            eprintln!(
                "                     {:>8} similar blocks collapsed",
                stats.lines_collapsed
            );
            eprintln!(
                "                     {:>8} large output bursts truncated",
                stats.bursts_collapsed
            );
            eprintln!(
                "                     {:>8} massive events truncated",
                stats.blocks_truncated
            );
            eprintln!(
                "  Sanitization:      {:>8} ANSI sequences stripped",
                stats.ansi_sequences_stripped
            );
            eprintln!(
                "                     {:>8} control characters removed",
                stats.control_chars_stripped
            );
            eprintln!(
                "──────────────────────────────────────────────────────────────────────────────\n"
            );
        }

        // Handle debug output if requested (--debug AND --output flags)
        // --debug is required, --output triggers the save-and-exit behavior
        let save_debug_output = self.options.debug && self.options.output_path.is_some();
        if save_debug_output {
            // Use provided path, or auto-derive from input if empty
            let output_path = match &self.options.output_path {
                Some(p) if !p.is_empty() => p.clone(),
                _ => {
                    let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
                        AnalysisError::IoError {
                            operation: "deriving debug output path".to_string(),
                            message: "Path does not have a valid filename".to_string(),
                        }
                    })?;
                    format!("/tmp/{}.txt", stem)
                }
            };

            std::fs::write(&output_path, content.text()).map_err(|e| AnalysisError::IoError {
                operation: "writing debug output".to_string(),
                message: e.to_string(),
            })?;

            if !self.options.quiet {
                eprintln!("Cleaned content written to: {}", output_path);
            }
        }

        if content.total_tokens == 0 || content.segments.is_empty() {
            return Err(AnalysisError::NoContent);
        }

        // 4. Calculate chunks (Stage 2)
        let calculator = if let Some(budget_tokens) = self.options.token_budget_override {
            if budget_tokens < 10000 {
                eprintln!(
                    "Warning: token_budget {} is below minimum (10000). Using default budget.",
                    budget_tokens
                );
                ChunkCalculator::for_agent(self.options.agent)
            } else {
                // Use overridden token budget from per-agent config
                let mut budget = self.options.agent.token_budget();
                budget.max_input_tokens = budget_tokens;
                ChunkCalculator::new(budget, ChunkConfig::default())
            }
        } else {
            ChunkCalculator::for_agent(self.options.agent)
        };
        let chunks = calculator.calculate_chunks(&content);

        // 5. Execute analysis (Stage 3+4)
        let timeout = Duration::from_secs(self.options.timeout_secs);
        let worker_count = self.calculate_worker_count(chunks.len(), content.total_tokens);

        // Return early if in debug output mode (after showing useful info)
        if save_debug_output {
            if !self.options.quiet {
                eprintln!(
                    "Analysis would use {} chunks, {} tokens, {} workers",
                    chunks.len(),
                    content.total_tokens,
                    worker_count
                );
            }
            return Ok(AnalysisResult {
                markers: vec![],
                write_report: WriteReport::default(),
                usage_summary: UsageSummary::default(),
                had_existing_markers,
                existing_marker_count,
                total_duration: content.total_duration,
            });
        }

        // Progress reporting
        let progress = if self.options.quiet {
            DefaultProgressReporter::quiet(chunks.len())
        } else {
            DefaultProgressReporter::new(chunks.len())
        };
        progress.start(chunks.len(), content.total_tokens);

        // Build prompt builder with template
        let total_duration = content.total_duration;
        let total_chunks = chunks.len();
        let prompt_builder = |chunk: &super::chunk::AnalysisChunk| -> String {
            build_analyze_prompt(chunk, total_duration, total_chunks)
        };

        // Execute with retry
        // use_schema = true unless --fast flag was passed
        let use_schema = !self.options.fast;
        let worker_progress = ProgressReporter::new(chunks.len());
        let executor = RetryExecutor::new(self.backend.as_ref(), timeout, worker_count, use_schema);
        let (results, tracker) =
            executor.execute_with_retry(chunks.clone(), &worker_progress, prompt_builder);

        // 6. Aggregate results (Stage 5)
        let aggregator = ResultAggregator::new(content.total_duration);
        let (markers, agg_report) = aggregator.aggregate(results);

        // 7. Write markers to file
        let write_report =
            MarkerWriter::write_markers(path, &markers).map_err(|e| AnalysisError::IoError {
                operation: "writing markers".to_string(),
                message: e.to_string(),
            })?;

        // 8. Report summary (Stage 6)
        let usage_summary = tracker.summary();

        if !self.options.quiet {
            if agg_report.failed_chunks.is_empty() {
                progress.finish(write_report.markers_written);
            } else {
                let failed_ranges: Vec<_> = chunks
                    .iter()
                    .filter(|c| agg_report.failed_chunks.contains(&c.id))
                    .map(|c| (c.time_range.start, c.time_range.end))
                    .collect();
                // Collect error messages in same order as failed_ranges
                let error_messages: Vec<_> = chunks
                    .iter()
                    .filter(|c| agg_report.failed_chunks.contains(&c.id))
                    .map(|c| {
                        agg_report
                            .failed_chunk_details
                            .iter()
                            .find(|f| f.chunk_id == c.id)
                            .map(|f| f.error.clone())
                            .unwrap_or_default()
                    })
                    .collect();
                progress.finish_partial_with_errors(
                    usage_summary.successful_chunks,
                    usage_summary.chunks_processed,
                    write_report.markers_written,
                    &failed_ranges,
                    &error_messages,
                );
            }
        }

        Ok(AnalysisResult {
            markers,
            write_report,
            usage_summary,
            had_existing_markers,
            existing_marker_count,
            total_duration,
        })
    }

    /// Calculate worker count based on options and content.
    fn calculate_worker_count(&self, chunk_count: usize, total_tokens: usize) -> usize {
        if self.options.no_parallel {
            return 1;
        }

        let config = WorkerConfig {
            min_workers: 1,
            max_workers: 8,
            user_override: self.options.workers,
        };
        let scaler = WorkerScaler::new(config);
        scaler.calculate_workers(chunk_count, total_tokens)
    }

    /// Curate markers using LLM to select the most significant ones.
    ///
    /// Call this after analyze() if the marker count is too high.
    /// Returns a reduced set of 8-12 most significant markers.
    pub fn curate_markers(
        &self,
        markers: &[ValidatedMarker],
        total_duration: f64,
        timeout: Duration,
    ) -> Result<Vec<ValidatedMarker>, AnalysisError> {
        let prompt = build_curation_prompt(markers, total_duration);
        let backend = self.backend_for_args(&self.options.curate_extra_args);

        // Never use schema for curation — it's a small prompt where
        // schema enforcement adds overhead without reliability benefit.
        let response =
            backend
                .invoke(&prompt, timeout, false)
                .map_err(|e| AnalysisError::IoError {
                    operation: "curation".to_string(),
                    message: format!("{}", e),
                })?;

        let parsed = backend
            .parse_response(&response)
            .map_err(|e| AnalysisError::IoError {
                operation: "parsing curation response".to_string(),
                message: format!("{}", e),
            })?;

        // Convert RawMarkers back to ValidatedMarkers
        let curated: Vec<ValidatedMarker> = parsed
            .into_iter()
            .map(|raw| ValidatedMarker::new(raw.timestamp, raw.label, raw.category))
            .collect();

        Ok(curated)
    }

    /// Suggest a better filename for the recording based on markers.
    ///
    /// Uses the LLM to generate a descriptive filename from the analysis markers.
    /// Passes the current filename so the LLM can see what the file is called now.
    /// Returns the suggested filename (without extension) or None on failure.
    pub fn suggest_rename(
        &self,
        markers: &[ValidatedMarker],
        total_duration: f64,
        timeout: Duration,
        current_filename: &str,
    ) -> Option<String> {
        let prompt = build_rename_prompt(markers, total_duration, current_filename);
        let backend = self.backend_for_args(&self.options.rename_extra_args);

        let response = backend
            .invoke(&prompt, timeout, false) // Never use schema for rename (plain text response)
            .ok()?;

        // Extract the filename from the response (strip wrapper if present)
        let filename = extract_rename_response(&response)?;

        // Validate the filename
        let filename = filename.trim().trim_matches('"').trim();
        if filename.is_empty() || filename.len() < 3 || filename.len() > 60 {
            return None;
        }

        // Ensure it's valid as a filename (kebab-case, no weird chars)
        let sanitized: String = filename
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect();

        // Clean up double dashes
        let mut result = String::new();
        let mut prev_dash = false;
        for c in sanitized.chars() {
            if c == '-' {
                if !prev_dash {
                    result.push(c);
                }
                prev_dash = true;
            } else {
                result.push(c);
                prev_dash = false;
            }
        }

        let result = result.trim_matches('-').to_lowercase();
        if result.len() < 3 {
            return None;
        }

        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::backend::{BackendError, RawMarker};
    use crate::analyzer::chunk::TokenBudget;
    use crate::asciicast::{Event, Header};
    use std::io::Write;
    use std::sync::Mutex;
    use tempfile::NamedTempFile;

    // ============================================
    // Mock Backend for Testing
    // ============================================

    struct MockBackend {
        responses: Mutex<Vec<Result<String, BackendError>>>,
        available: bool,
    }

    impl MockBackend {
        fn new(responses: Vec<Result<String, BackendError>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                available: true,
            }
        }

        fn unavailable() -> Self {
            Self {
                responses: Mutex::new(vec![]),
                available: false,
            }
        }
    }

    impl AgentBackend for MockBackend {
        fn name(&self) -> &'static str {
            "mock"
        }

        fn is_available(&self) -> bool {
            self.available
        }

        fn invoke(
            &self,
            _prompt: &str,
            _timeout: Duration,
            _use_schema: bool,
        ) -> Result<String, BackendError> {
            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                Ok(r#"{"markers": []}"#.to_string())
            } else {
                responses.remove(0)
            }
        }

        fn parse_response(&self, response: &str) -> Result<Vec<RawMarker>, BackendError> {
            super::super::extract_json(response).map(|r| r.markers)
        }

        fn token_budget(&self) -> TokenBudget {
            TokenBudget::claude()
        }
    }

    // ============================================
    // Test Helpers
    // ============================================

    /// Create a realistic test cast file with diverse, multi-line content
    /// that survives the full extraction pipeline (TerminalTransform + dedup).
    fn create_test_cast_file() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        let header = Header {
            version: 3,
            width: Some(120),
            height: Some(10),
            timestamp: None,
            duration: None,
            title: None,
            command: None,
            term: None,
            env: None,
            idle_time_limit: None,
        };
        let mut cast = AsciicastFile::new(header);

        // Simulate a realistic multi-phase agent session with natural timing.
        // Content is varied and phases are separated by meaningful time gaps
        // to survive the full extraction pipeline (coalescing, dedup, burst filter).
        let phases: &[(&[&str], f64)] = &[
            // Phase 1: Build (t=0)
            (
                &[
                    "$ cargo build --release\n",
                    "   Compiling serde v1.0.200\n",
                    "   Compiling agr v0.1.0 (/home/user/project)\n",
                    "    Finished release [optimized] target(s) in 14.32s\n",
                ],
                0.0,
            ),
            // Phase 2: Tests (t=5)
            (
                &[
                    "$ cargo test --lib\n",
                    "running 42 tests\n",
                    "test config::tests::load_default_config ... ok\n",
                    "test parser::tests::parse_asciicast_header ... ok\n",
                    "test result: ok. 42 passed; 0 failed; 0 ignored\n",
                ],
                5.0,
            ),
            // Phase 3: Git operations (t=12)
            (
                &[
                    "$ git add -A && git commit -m 'feat: add clipboard support'\n",
                    "[main abc1234] feat: add clipboard support\n",
                    " 3 files changed, 150 insertions(+), 12 deletions(-)\n",
                ],
                12.0,
            ),
            // Phase 4: Deploy (t=20)
            (
                &[
                    "$ git push origin main\n",
                    "Enumerating objects: 8, done.\n",
                    "To github.com:user/project.git\n",
                    "   def5678..abc1234  main -> main\n",
                ],
                20.0,
            ),
            // Phase 5: Verification (t=30)
            (
                &[
                    "$ curl -s https://api.example.com/health | jq .\n",
                    "{\n",
                    "  \"status\": \"healthy\",\n",
                    "  \"version\": \"1.2.3\",\n",
                    "  \"uptime\": \"2h 15m\"\n",
                    "}\n",
                ],
                30.0,
            ),
        ];

        for (lines, phase_start) in phases {
            for (i, line) in lines.iter().enumerate() {
                let time = if i == 0 { *phase_start } else { 0.1 };
                cast.events.push(Event::output(time, *line));
            }
        }

        let content = cast.to_string().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    fn mock_response_with_markers() -> String {
        // Timestamps are relative to the chunk start. Use 0.0 so they resolve
        // to exactly time_range.start, guaranteed to be within recording duration.
        r#"{"markers": [
            {"timestamp": 0.0, "label": "Started build process", "category": "implementation"},
            {"timestamp": 0.01, "label": "Build completed successfully", "category": "success"}
        ]}"#
        .to_string()
    }

    // ============================================
    // AnalyzeOptions Tests
    // ============================================

    #[test]
    fn analyze_options_default() {
        let opts = AnalyzeOptions::default();
        assert_eq!(opts.agent, AgentType::Claude);
        assert_eq!(opts.workers, None);
        assert_eq!(opts.timeout_secs, DEFAULT_TIMEOUT_SECS);
        assert!(!opts.no_parallel);
        assert!(!opts.quiet);
    }

    #[test]
    fn analyze_options_with_agent() {
        let opts = AnalyzeOptions::with_agent(AgentType::Codex);
        assert_eq!(opts.agent, AgentType::Codex);
    }

    #[test]
    fn analyze_options_builder() {
        let opts = AnalyzeOptions::with_agent(AgentType::Gemini)
            .workers(4)
            .timeout(60)
            .sequential()
            .quiet();

        assert_eq!(opts.agent, AgentType::Gemini);
        assert_eq!(opts.workers, Some(4));
        assert_eq!(opts.timeout_secs, 60);
        assert!(opts.no_parallel);
        assert!(opts.quiet);
    }

    // ============================================
    // AnalyzerService Tests
    // ============================================

    #[test]
    fn analyzer_service_is_agent_available() {
        let opts = AnalyzeOptions::default().quiet();
        let backend = Box::new(MockBackend::new(vec![]));
        let service = AnalyzerService::with_backend(opts, backend);

        assert!(service.is_agent_available());
    }

    #[test]
    fn analyzer_service_unavailable_backend() {
        let opts = AnalyzeOptions::default().quiet();
        let backend = Box::new(MockBackend::unavailable());
        let service = AnalyzerService::with_backend(opts, backend);

        assert!(!service.is_agent_available());
    }

    #[test]
    fn analyzer_service_analyze_small_file() {
        let file = create_test_cast_file();
        let opts = AnalyzeOptions::default().quiet();
        let backend = Box::new(MockBackend::new(vec![Ok(mock_response_with_markers())]));
        let service = AnalyzerService::with_backend(opts, backend);

        let result = service.analyze(file.path());

        // The test fixture has enough diverse content to survive the full
        // extraction pipeline. Verify the mock backend's markers are returned.
        let analysis = result.unwrap_or_else(|e| {
            panic!(
                "Analysis should succeed with realistic test content, got: {:?}",
                e
            )
        });
        assert!(
            !analysis.markers.is_empty(),
            "Expected markers from mock backend, got none"
        );
    }

    #[test]
    fn analyzer_service_analyze_with_codex_agent() {
        let file = create_test_cast_file();
        let opts = AnalyzeOptions::with_agent(AgentType::Codex).quiet();
        let backend = Box::new(MockBackend::new(vec![Ok(mock_response_with_markers())]));
        let service = AnalyzerService::with_backend(opts, backend);

        let result = service.analyze(file.path()).unwrap();

        assert!(result.is_success());
    }

    #[test]
    fn analyzer_service_preserves_file_integrity() {
        let file = create_test_cast_file();

        // Read original content
        let original = std::fs::read_to_string(file.path()).unwrap();
        let original_lines: Vec<_> = original.lines().collect();

        let opts = AnalyzeOptions::default().quiet();
        let backend = Box::new(MockBackend::new(vec![
            Ok(mock_response_with_markers()),
            Ok(mock_response_with_markers()),
            Ok(mock_response_with_markers()),
        ]));
        let service = AnalyzerService::with_backend(opts, backend);

        let result = service.analyze(file.path());

        match result {
            Ok(_result) => {
                // Read modified content
                let modified = std::fs::read_to_string(file.path()).unwrap();
                let modified_lines: Vec<_> = modified.lines().collect();

                // Header should be preserved
                assert_eq!(original_lines[0], modified_lines[0]);

                // File should be valid NDJSON
                for line in modified_lines {
                    assert!(
                        serde_json::from_str::<serde_json::Value>(line).is_ok(),
                        "Invalid JSON line: {}",
                        line
                    );
                }
            }
            Err(AnalysisError::NoContent) => {
                // Acceptable: TerminalTransform can reduce small synthetic content to nothing
                // Verify file was NOT modified
                let after = std::fs::read_to_string(file.path()).unwrap();
                assert_eq!(original, after, "File should not be modified on NoContent");
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn analyzer_service_detects_existing_markers() {
        // Create a file with an existing marker
        let mut file = NamedTempFile::new().unwrap();
        let header = Header {
            version: 3,
            width: Some(80),
            height: Some(24),
            timestamp: None,
            duration: None,
            title: None,
            command: None,
            term: None,
            env: None,
            idle_time_limit: None,
        };
        let mut cast = AsciicastFile::new(header);
        cast.events.push(Event::output(0.1, "Hello\n"));
        cast.events.push(Event::marker(0.2, "Existing marker"));
        cast.events.push(Event::output(0.5, "World\n"));

        let content = cast.to_string().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let opts = AnalyzeOptions::default().quiet();
        let backend = Box::new(MockBackend::new(vec![Ok(r#"{"markers": []}"#.to_string())]));
        let service = AnalyzerService::with_backend(opts, backend);

        let result = service.analyze(file.path()).unwrap();

        assert!(result.had_existing_markers);
        assert_eq!(result.existing_marker_count, 1);
    }

    #[test]
    fn analyzer_service_sequential_mode() {
        let file = create_test_cast_file();
        let opts = AnalyzeOptions::default().sequential().quiet();
        let backend = Box::new(MockBackend::new(vec![Ok(mock_response_with_markers())]));
        let service = AnalyzerService::with_backend(opts, backend);

        let worker_count = service.calculate_worker_count(4, 100_000);
        assert_eq!(worker_count, 1);

        let result = service.analyze(file.path()).unwrap();
        assert!(result.is_success());
    }

    #[test]
    fn analyzer_service_empty_content_error() {
        // Create a file with only header (no output events)
        let mut file = NamedTempFile::new().unwrap();
        let header = Header {
            version: 3,
            width: Some(80),
            height: Some(24),
            timestamp: None,
            duration: None,
            title: None,
            command: None,
            term: None,
            env: None,
            idle_time_limit: None,
        };
        let cast = AsciicastFile::new(header);

        let content = cast.to_string().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let opts = AnalyzeOptions::default().quiet();
        let backend = Box::new(MockBackend::new(vec![]));
        let service = AnalyzerService::with_backend(opts, backend);

        let result = service.analyze(file.path());

        assert!(matches!(result, Err(AnalysisError::NoContent)));
    }

    #[test]
    fn analyzer_service_file_not_found_error() {
        let opts = AnalyzeOptions::default().quiet();
        let backend = Box::new(MockBackend::new(vec![]));
        let service = AnalyzerService::with_backend(opts, backend);

        let result = service.analyze("/nonexistent/path/file.cast");

        assert!(matches!(result, Err(AnalysisError::IoError { .. })));
    }

    // ============================================
    // AnalysisResult Tests
    // ============================================

    #[test]
    fn analysis_result_is_success() {
        let result = AnalysisResult {
            markers: vec![],
            write_report: WriteReport {
                markers_written: 5,
                had_existing_markers: false,
                existing_marker_count: 0,
            },
            usage_summary: UsageSummary {
                chunks_processed: 2,
                successful_chunks: 2,
                failed_chunks: 0,
                total_estimated_tokens: 10000,
                total_duration: Duration::from_secs(30),
                avg_tokens_per_chunk: 5000,
                avg_duration_per_chunk: Duration::from_secs(15),
                success_rate: 1.0,
                total_retries: 0,
            },
            had_existing_markers: false,
            existing_marker_count: 0,
            total_duration: 120.0,
        };

        assert!(result.is_success());
        assert!(!result.is_partial());
        assert_eq!(result.markers_added(), 5);
    }

    #[test]
    fn analysis_result_is_partial() {
        let result = AnalysisResult {
            markers: vec![],
            write_report: WriteReport {
                markers_written: 3,
                had_existing_markers: false,
                existing_marker_count: 0,
            },
            usage_summary: UsageSummary {
                chunks_processed: 4,
                successful_chunks: 2,
                failed_chunks: 2,
                total_estimated_tokens: 20000,
                total_duration: Duration::from_secs(60),
                avg_tokens_per_chunk: 5000,
                avg_duration_per_chunk: Duration::from_secs(15),
                success_rate: 0.5,
                total_retries: 4,
            },
            had_existing_markers: false,
            existing_marker_count: 0,
            total_duration: 180.0,
        };

        assert!(result.is_success());
        assert!(result.is_partial());
    }
}
