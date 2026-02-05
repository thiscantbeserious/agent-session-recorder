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
use super::chunk::ChunkCalculator;
use super::config::ExtractionConfig;
use super::error::AnalysisError;
use super::extractor::ContentExtractor;
use super::progress::DefaultProgressReporter;
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
        let backend = options.agent.create_backend();
        Self { options, backend }
    }

    /// Create with a custom backend (for testing).
    pub fn with_backend(options: AnalyzeOptions, backend: Box<dyn AgentBackend>) -> Self {
        Self { options, backend }
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
        let content = extractor.extract(&mut cast.events);

        if content.total_tokens == 0 || content.segments.is_empty() {
            return Err(AnalysisError::NoContent);
        }

        // Handle debug output if requested
        if self.options.debug || self.options.output_path.is_some() {
            let output_path = if let Some(path_str) = &self.options.output_path {
                path_str.clone()
            } else {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| format!("{}.txt", s))
                    .expect("Path must have a filename")
            };
            
            std::fs::write(&output_path, content.text())
                .map_err(|e| AnalysisError::IoError {
                    operation: "writing debug output".to_string(),
                    message: e.to_string(),
                })?;
            
            if !self.options.quiet {
                eprintln!("Cleaned content written to: {}", output_path);
            }

            // Return early if in debug mode
            if self.options.debug {
                return Ok(AnalysisResult {
                    markers: vec![],
                    write_report: WriteReport::default(),
                    usage_summary: UsageSummary::default(),
                    had_existing_markers,
                    existing_marker_count,
                    total_duration: content.total_duration,
                });
            }
        }

        // Show extraction stats
        if !self.options.quiet {
            let stats = &content.stats;
            let compression = if stats.original_bytes > 0 {
                100.0 - (stats.extracted_bytes as f64 / stats.original_bytes as f64 * 100.0)
            } else {
                0.0
            };
            eprintln!(
                "Extracted: {}KB → {}KB ({:.0}% reduction, {} ANSI stripped, {} deduped, {} coalesced, {} global, {} window, {} collapsed, {} truncated)",
                stats.original_bytes / 1024,
                stats.extracted_bytes / 1024,
                compression,
                stats.ansi_sequences_stripped,
                stats.progress_lines_deduplicated,
                stats.events_coalesced,
                stats.global_lines_deduped,
                stats.window_events_deduped,
                stats.lines_collapsed,
                stats.blocks_truncated
            );
        }

        // 4. Calculate chunks (Stage 2)
        let calculator = ChunkCalculator::for_agent(self.options.agent);
        let chunks = calculator.calculate_chunks(&content);

        // 5. Execute analysis (Stage 3+4)
        let timeout = Duration::from_secs(self.options.timeout_secs);
        let worker_count = self.calculate_worker_count(chunks.len(), content.total_tokens);

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
            build_prompt(chunk, total_duration, total_chunks)
        };

        // Execute with retry
        let worker_progress = ProgressReporter::new(chunks.len());
        let executor = RetryExecutor::new(self.backend.as_ref(), timeout, worker_count);
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

        let response = self
            .backend
            .invoke(&prompt, timeout)
            .map_err(|e| AnalysisError::IoError {
                operation: "curation".to_string(),
                message: format!("{}", e),
            })?;

        let parsed = self
            .backend
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
}

/// Maximum tokens for prompt content (safety net for edge cases).
/// This should be higher than the chunk calculator's available_for_content()
/// (161,500 for Claude) so truncation only triggers if chunking fails.
/// Setting to 170K gives a small buffer above Claude's limit while still
/// catching bugs where chunks are not properly sized.
const MAX_PROMPT_CONTENT_TOKENS: usize = 170_000;

/// Estimated characters per token for truncation calculation.
const CHARS_PER_TOKEN: usize = 4;

/// Target total markers for an entire session (regardless of size).
const TARGET_TOTAL_MARKERS_MIN: usize = 10;
const TARGET_TOTAL_MARKERS_MAX: usize = 20;

/// Build the analysis prompt for a chunk.
///
/// Uses the template from `src/analyzer/prompts/analyze.txt`.
/// If the resulting prompt exceeds token limits, the content is truncated
/// with a warning logged.
///
/// # Arguments
///
/// * `chunk` - The chunk to analyze
/// * `total_duration` - Total duration of the recording
/// * `total_chunks` - Total number of chunks (for calculating markers per chunk)
pub fn build_prompt(
    chunk: &super::chunk::AnalysisChunk,
    total_duration: f64,
    total_chunks: usize,
) -> String {
    // Include the template at compile time
    const TEMPLATE: &str = include_str!("prompts/analyze.txt");

    // Calculate markers per chunk to achieve target total
    let (min_markers, max_markers) = calculate_markers_per_chunk(total_chunks);

    // Validate and potentially truncate content if too large
    let content = truncate_content_if_needed(&chunk.text, chunk.estimated_tokens);

    TEMPLATE
        .replace(
            "{chunk_start_time}",
            &format!("{:.1}", chunk.time_range.start),
        )
        .replace("{chunk_end_time}", &format!("{:.1}", chunk.time_range.end))
        .replace("{total_duration}", &format!("{:.1}", total_duration))
        .replace("{min_markers}", &min_markers.to_string())
        .replace("{max_markers}", &max_markers.to_string())
        .replace("{cleaned_content}", &content)
}

/// Calculate how many markers to request per chunk.
///
/// Distributes the target total markers across chunks, ensuring
/// each chunk requests at least 1 marker.
fn calculate_markers_per_chunk(total_chunks: usize) -> (usize, usize) {
    if total_chunks == 0 {
        return (1, 3);
    }

    // Distribute target markers across chunks
    let min_per_chunk = (TARGET_TOTAL_MARKERS_MIN / total_chunks).max(1);
    let max_per_chunk = (TARGET_TOTAL_MARKERS_MAX / total_chunks).max(min_per_chunk + 1);

    // Cap at reasonable per-chunk limits
    (min_per_chunk.min(5), max_per_chunk.min(8))
}

/// Truncate content if it exceeds the maximum prompt token limit.
///
/// Returns the content as-is if within limits, otherwise truncates
/// and appends a truncation notice.
fn truncate_content_if_needed(content: &str, estimated_tokens: usize) -> String {
    if estimated_tokens <= MAX_PROMPT_CONTENT_TOKENS {
        return content.to_string();
    }

    eprintln!(
        "Warning: Content size ({} tokens) exceeds limit ({}). Truncating.",
        estimated_tokens, MAX_PROMPT_CONTENT_TOKENS
    );

    // Calculate safe character limit
    let max_chars = MAX_PROMPT_CONTENT_TOKENS * CHARS_PER_TOKEN;
    let truncated: String = content.chars().take(max_chars).collect();

    format!("{}\n\n[Content truncated due to size limits]", truncated)
}

/// Build the curation prompt for marker selection.
fn build_curation_prompt(markers: &[ValidatedMarker], total_duration: f64) -> String {
    const TEMPLATE: &str = include_str!("prompts/curate.txt");

    // Convert markers to JSON for the prompt
    let markers_json: Vec<serde_json::Value> = markers
        .iter()
        .map(|m| {
            serde_json::json!({
                "timestamp": m.timestamp,
                "label": m.label,
                "category": format!("{:?}", m.category).to_lowercase()
            })
        })
        .collect();

    let markers_json_str =
        serde_json::to_string_pretty(&markers_json).unwrap_or_else(|_| "[]".to_string());

    TEMPLATE
        .replace("{total_duration}", &format!("{:.1}", total_duration))
        .replace("{duration_minutes}", &format!("{:.1}", total_duration / 60.0))
        .replace("{marker_count}", &markers.len().to_string())
        .replace("{markers_json}", &markers_json_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::backend::{BackendError, RawMarker};
    use crate::analyzer::chunk::{TimeRange, TokenBudget};
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

        fn invoke(&self, _prompt: &str, _timeout: Duration) -> Result<String, BackendError> {
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

    fn create_test_cast_file() -> NamedTempFile {
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
        cast.events.push(Event::output(0.1, "Starting build...\n"));
        cast.events.push(Event::output(0.5, "Compiling code...\n"));
        cast.events.push(Event::output(1.0, "Build complete!\n"));

        let content = cast.to_string().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    fn mock_response_with_markers() -> String {
        r#"{"markers": [
            {"timestamp": 0.3, "label": "Started build process", "category": "implementation"},
            {"timestamp": 1.0, "label": "Build completed successfully", "category": "success"}
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
    // build_prompt Tests
    // ============================================

    #[test]
    fn build_prompt_substitutes_values() {
        let chunk = super::super::chunk::AnalysisChunk::new(
            0,
            TimeRange::new(10.0, 50.0),
            vec![super::super::types::AnalysisSegment {
                start_time: 10.0,
                end_time: 50.0,
                content: "Test content here".to_string(),
                estimated_tokens: 100,
                event_range: (0, 10),
            }],
        );

        let prompt = build_prompt(&chunk, 120.0, 3); // 3 chunks total

        assert!(prompt.contains("10.0s - 50.0s"));
        assert!(prompt.contains("120.0s"));
        assert!(prompt.contains("Test content here"));
        assert!(prompt.contains("planning"));
        assert!(prompt.contains("design"));
        assert!(prompt.contains("implementation"));
        assert!(prompt.contains("success"));
        assert!(prompt.contains("failure"));
        // With 3 chunks, target 10-20 markers total = 3-6 per chunk
        assert!(prompt.contains("3-6"));
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

        let result = service.analyze(file.path()).unwrap();

        assert!(result.is_success());
        assert_eq!(result.markers_added(), 2);
        assert!(!result.had_existing_markers);
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
        let backend = Box::new(MockBackend::new(vec![Ok(mock_response_with_markers())]));
        let service = AnalyzerService::with_backend(opts, backend);

        let result = service.analyze(file.path()).unwrap();

        // Read modified content
        let modified = std::fs::read_to_string(file.path()).unwrap();
        let modified_lines: Vec<_> = modified.lines().collect();

        // Header should be preserved
        assert_eq!(original_lines[0], modified_lines[0]);

        // Should have more lines (markers added)
        assert!(modified_lines.len() > original_lines.len());

        // Markers should be added
        assert!(result.markers_added() > 0);

        // File should be valid NDJSON
        for line in modified_lines {
            assert!(
                serde_json::from_str::<serde_json::Value>(line).is_ok(),
                "Invalid JSON line: {}",
                line
            );
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
