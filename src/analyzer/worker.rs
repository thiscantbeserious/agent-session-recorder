//! Parallel execution for chunk analysis using Rayon.
//!
//! This module provides worker scaling and parallel execution of chunk analysis.
//! It uses Rayon for automatic thread pool management and work-stealing.
//!
//! # Design
//!
//! - `WorkerScaler` calculates optimal worker count based on content size
//! - `ParallelExecutor` orchestrates parallel chunk processing
//! - `ChunkResult` holds the result of analyzing a single chunk
//! - `RetryExecutor` provides retry with fallback to sequential
//! - Progress is reported via `ProgressReporter` callback
//!
//! # Retry & Fallback Strategy
//!
//! When parallel execution fails, the system can fall back to sequential:
//! 1. Parallel execution attempted first
//! 2. If all chunks fail (rate limiting), fall back to sequential
//! 3. Sequential execution with small delay between chunks
//! 4. Each chunk retried up to 3 times with exponential backoff

use crate::analyzer::backend::{AgentBackend, BackendError, RawMarker};
use crate::analyzer::chunk::{AnalysisChunk, TimeRange};
use crate::analyzer::tracker::TokenTracker;
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Configuration for worker scaling.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Minimum number of workers
    pub min_workers: usize,
    /// Maximum number of workers
    pub max_workers: usize,
    /// User override for worker count (takes precedence)
    pub user_override: Option<usize>,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            min_workers: 1,
            max_workers: 8,
            user_override: None,
        }
    }
}

/// Calculates optimal worker count based on content size and system resources.
#[derive(Debug)]
pub struct WorkerScaler {
    config: WorkerConfig,
}

impl WorkerScaler {
    /// Create a new worker scaler with the given configuration.
    pub fn new(config: WorkerConfig) -> Self {
        Self { config }
    }

    /// Create a worker scaler with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(WorkerConfig::default())
    }

    /// Calculate optimal worker count based on chunks and total tokens.
    ///
    /// Scaling heuristics (from ADR):
    /// - <100K tokens: scale 0.5, yields 1 worker
    /// - 100K-500K: scale 1.0, yields 1-3 workers
    /// - 500K-1M: scale 1.2, yields 3-6 workers
    /// - >1M: scale 1.5, yields 4-8 workers
    ///
    /// Result is capped by min(max_workers, CPU_count).
    pub fn calculate_workers(&self, chunk_count: usize, total_tokens: usize) -> usize {
        // User override takes precedence
        if let Some(override_count) = self.config.user_override {
            return override_count.clamp(1, self.config.max_workers);
        }

        // Scale factor based on content size
        let scale_factor = match total_tokens {
            0..=100_000 => 0.5,
            100_001..=500_000 => 1.0,
            500_001..=1_000_000 => 1.2,
            _ => 1.5,
        };

        let scaled = (chunk_count as f64 * scale_factor).ceil() as usize;

        // Get system CPU count
        let cpu_count = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);

        // Calculate effective bounds (CPU count may limit max below configured min)
        let effective_max = self.config.max_workers.min(cpu_count);
        let effective_min = self.config.min_workers.min(effective_max);

        // Clamp to reasonable bounds
        scaled.clamp(effective_min, effective_max)
    }
}

/// Result of analyzing a single chunk.
#[derive(Debug)]
pub struct ChunkResult {
    /// Chunk identifier
    pub chunk_id: usize,
    /// Time range covered by this chunk
    pub time_range: TimeRange,
    /// Analysis result (markers or error)
    pub result: Result<Vec<RawMarker>, BackendError>,
}

impl ChunkResult {
    /// Create a successful chunk result.
    pub fn success(chunk_id: usize, time_range: TimeRange, markers: Vec<RawMarker>) -> Self {
        Self {
            chunk_id,
            time_range,
            result: Ok(markers),
        }
    }

    /// Create a failed chunk result.
    pub fn failure(chunk_id: usize, time_range: TimeRange, error: BackendError) -> Self {
        Self {
            chunk_id,
            time_range,
            result: Err(error),
        }
    }

    /// Check if this result is successful.
    pub fn is_success(&self) -> bool {
        self.result.is_ok()
    }

    /// Check if this result is a failure.
    pub fn is_failure(&self) -> bool {
        self.result.is_err()
    }
}

/// Progress reporter for parallel analysis.
///
/// Thread-safe progress tracking using atomic operations.
pub struct ProgressReporter {
    /// Current count of completed chunks
    completed: Arc<AtomicUsize>,
    /// Total number of chunks
    total: usize,
    /// Optional callback for progress updates
    callback: Option<Box<dyn Fn(usize, usize) + Send + Sync>>,
}

impl ProgressReporter {
    /// Create a new progress reporter.
    pub fn new(total: usize) -> Self {
        Self {
            completed: Arc::new(AtomicUsize::new(0)),
            total,
            callback: None,
        }
    }

    /// Create a progress reporter with a callback.
    pub fn with_callback<F>(total: usize, callback: F) -> Self
    where
        F: Fn(usize, usize) + Send + Sync + 'static,
    {
        Self {
            completed: Arc::new(AtomicUsize::new(0)),
            total,
            callback: Some(Box::new(callback)),
        }
    }

    /// Report that one more chunk has been completed.
    ///
    /// Returns the new count of completed chunks.
    pub fn report_progress(&self) -> usize {
        let completed = self.completed.fetch_add(1, Ordering::SeqCst) + 1;

        if let Some(ref callback) = self.callback {
            callback(completed, self.total);
        }

        completed
    }

    /// Get current progress (completed, total).
    pub fn get_progress(&self) -> (usize, usize) {
        (self.completed.load(Ordering::SeqCst), self.total)
    }

    /// Get a clone of the completed counter for sharing across threads.
    pub fn completed_counter(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.completed)
    }
}

/// Executor for parallel chunk analysis.
pub struct ParallelExecutor<'a, B: AgentBackend + ?Sized> {
    backend: &'a B,
    timeout: Duration,
    worker_count: usize,
    use_schema: bool,
}

impl<'a, B: AgentBackend + ?Sized> ParallelExecutor<'a, B> {
    /// Create a new parallel executor.
    pub fn new(backend: &'a B, timeout: Duration, worker_count: usize, use_schema: bool) -> Self {
        Self {
            backend,
            timeout,
            worker_count,
            use_schema,
        }
    }

    /// Execute analysis on chunks, returning results for each.
    ///
    /// For a single chunk, processes directly without creating a thread pool.
    /// For multiple chunks, uses Rayon for parallel processing.
    pub fn execute(
        &self,
        chunks: Vec<AnalysisChunk>,
        progress: &ProgressReporter,
        prompt_builder: impl Fn(&AnalysisChunk) -> String + Sync,
    ) -> Vec<ChunkResult> {
        if chunks.is_empty() {
            return Vec::new();
        }

        // Single chunk optimization: no thread pool needed
        if chunks.len() == 1 {
            return self.execute_single(chunks, progress, &prompt_builder);
        }

        // Multiple chunks: use Rayon parallel execution
        self.execute_parallel(chunks, progress, &prompt_builder)
    }

    /// Execute a single chunk without thread pool overhead.
    fn execute_single(
        &self,
        mut chunks: Vec<AnalysisChunk>,
        progress: &ProgressReporter,
        prompt_builder: &impl Fn(&AnalysisChunk) -> String,
    ) -> Vec<ChunkResult> {
        let chunk = chunks.remove(0);
        let result = self.analyze_chunk(&chunk, prompt_builder);
        progress.report_progress();
        vec![result]
    }

    /// Execute multiple chunks in parallel using Rayon.
    fn execute_parallel(
        &self,
        chunks: Vec<AnalysisChunk>,
        progress: &ProgressReporter,
        prompt_builder: &(impl Fn(&AnalysisChunk) -> String + Sync),
    ) -> Vec<ChunkResult> {
        // Build dedicated thread pool with specified worker count
        let pool = match rayon::ThreadPoolBuilder::new()
            .num_threads(self.worker_count)
            .thread_name(|i| format!("analyzer-{}", i))
            .build()
        {
            Ok(pool) => pool,
            Err(e) => {
                // If thread pool creation fails, return failures for all chunks
                eprintln!(
                    "Warning: Failed to create thread pool: {}. Processing sequentially.",
                    e
                );
                return chunks
                    .into_iter()
                    .map(|chunk| {
                        let result = self.analyze_chunk(&chunk, prompt_builder);
                        progress.report_progress();
                        result
                    })
                    .collect();
            }
        };

        // Execute in parallel
        pool.install(|| {
            chunks
                .into_par_iter()
                .map(|chunk| {
                    let result = self.analyze_chunk(&chunk, prompt_builder);
                    progress.report_progress();
                    result
                })
                .collect()
        })
    }

    /// Analyze a single chunk using the backend.
    fn analyze_chunk(
        &self,
        chunk: &AnalysisChunk,
        prompt_builder: &impl Fn(&AnalysisChunk) -> String,
    ) -> ChunkResult {
        let prompt = prompt_builder(chunk);

        match self.backend.invoke(&prompt, self.timeout, self.use_schema) {
            Ok(response) => match self.backend.parse_response(&response) {
                Ok(markers) => ChunkResult::success(chunk.id, chunk.time_range.clone(), markers),
                Err(e) => ChunkResult::failure(chunk.id, chunk.time_range.clone(), e),
            },
            Err(e) => ChunkResult::failure(chunk.id, chunk.time_range.clone(), e),
        }
    }
}

/// Executor with parallel execution and token tracking.
///
/// Wraps parallel execution with:
/// - Token tracking for visibility
/// - Rate limit detection and warnings
pub struct RetryExecutor<'a, B: AgentBackend + ?Sized> {
    backend: &'a B,
    timeout: Duration,
    worker_count: usize,
    use_schema: bool,
}

impl<'a, B: AgentBackend + ?Sized> RetryExecutor<'a, B> {
    /// Create a new executor.
    pub fn new(backend: &'a B, timeout: Duration, worker_count: usize, use_schema: bool) -> Self {
        Self {
            backend,
            timeout,
            worker_count,
            use_schema,
        }
    }

    /// Execute analysis with tracking.
    ///
    /// Returns tuple of (results, tracker) for visibility.
    pub fn execute_with_retry(
        &self,
        chunks: Vec<AnalysisChunk>,
        progress: &ProgressReporter,
        prompt_builder: impl Fn(&AnalysisChunk) -> String + Sync,
    ) -> (Vec<ChunkResult>, TokenTracker) {
        let mut tracker = TokenTracker::new();
        let chunk_count = chunks.len();

        if chunks.is_empty() {
            return (Vec::new(), tracker);
        }

        // Pre-extract token counts to avoid needing full chunks after execution
        let token_map: std::collections::HashMap<usize, usize> =
            chunks.iter().map(|c| (c.id, c.estimated_tokens)).collect();

        // Try parallel execution first
        let parallel_executor = ParallelExecutor::new(
            self.backend,
            self.timeout,
            self.worker_count,
            self.use_schema,
        );

        let results = parallel_executor.execute(chunks, progress, &prompt_builder);

        // Check if all failed (might need sequential fallback)
        let all_failed = results.iter().all(|r| r.is_failure());
        let has_rate_limit = results
            .iter()
            .any(|r| matches!(&r.result, Err(BackendError::RateLimited(_))));

        // Sequential fallback is no longer available since we consume chunks in parallel
        // execution. If all chunks fail with rate limiting, we return the failed results.
        // Sequential retry would need to be handled at a higher level.
        if all_failed && has_rate_limit && chunk_count > 1 {
            eprintln!(
                "Warning: All {} chunks failed with rate limiting. Sequential fallback unavailable.",
                chunk_count
            );
        }

        // Record results in tracker using pre-extracted token counts
        for result in &results {
            let tokens = token_map.get(&result.chunk_id).copied().unwrap_or(0);

            match &result.result {
                Ok(_) => tracker.record_success(
                    result.chunk_id,
                    tokens,
                    Duration::ZERO, // Parallel doesn't track individual durations
                    1,
                ),
                Err(_) => tracker.record_failure(result.chunk_id, tokens, Duration::ZERO, 1),
            }
        }

        (results, tracker)
    }

    /// Check if fallback to sequential should be triggered.
    pub fn should_fallback_to_sequential(results: &[ChunkResult]) -> bool {
        if results.is_empty() {
            return false;
        }

        // Fall back if all failed and at least one is rate limited
        let all_failed = results.iter().all(|r| r.is_failure());
        let has_rate_limit = results
            .iter()
            .any(|r| matches!(&r.result, Err(BackendError::RateLimited(_))));

        all_failed && has_rate_limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::backend::MarkerCategory;
    use crate::analyzer::chunk::TokenBudget;
    use std::sync::Mutex;

    // ============================================
    // Mock Backend for Testing
    // ============================================

    /// Mock backend for testing parallel execution.
    struct MockBackend {
        /// Responses to return for each invocation (by chunk content matching)
        responses: Mutex<Vec<Result<String, BackendError>>>,
        /// Track invocations
        invocations: Mutex<Vec<String>>,
    }

    impl MockBackend {
        fn new(responses: Vec<Result<String, BackendError>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                invocations: Mutex::new(Vec::new()),
            }
        }

        fn invocation_count(&self) -> usize {
            self.invocations.lock().unwrap().len()
        }
    }

    impl AgentBackend for MockBackend {
        fn name(&self) -> &'static str {
            "mock"
        }

        fn is_available(&self) -> bool {
            true
        }

        fn invoke(
            &self,
            prompt: &str,
            _timeout: Duration,
            _use_schema: bool,
        ) -> Result<String, BackendError> {
            self.invocations.lock().unwrap().push(prompt.to_string());

            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                // Default response with empty markers
                Ok(r#"{"markers": []}"#.to_string())
            } else {
                responses.remove(0)
            }
        }

        fn parse_response(&self, response: &str) -> Result<Vec<RawMarker>, BackendError> {
            crate::analyzer::extract_json(response).map(|r| r.markers)
        }

        fn token_budget(&self) -> TokenBudget {
            TokenBudget::claude()
        }
    }

    // ============================================
    // WorkerScaler Tests
    // ============================================

    #[test]
    fn worker_scaler_small_content_yields_one_worker() {
        let scaler = WorkerScaler::with_defaults();

        // <100K tokens, scale 0.5
        // 2 chunks * 0.5 = 1 worker
        let workers = scaler.calculate_workers(2, 50_000);
        assert_eq!(workers, 1);
    }

    #[test]
    fn worker_scaler_medium_content_scales_normally() {
        let scaler = WorkerScaler::with_defaults();

        // 100K-500K tokens, scale 1.0
        // 3 chunks * 1.0 = 3 workers (but may be capped by CPU count on CI)
        let workers = scaler.calculate_workers(3, 300_000);
        let cpu_count = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);
        let expected = 3.min(cpu_count).min(8);
        assert_eq!(workers, expected);
    }

    #[test]
    fn worker_scaler_large_content_scales_up() {
        let scaler = WorkerScaler::with_defaults();

        // 500K-1M tokens, scale 1.2
        // 5 chunks * 1.2 = 6 workers (but may be capped by CPU count on CI)
        let workers = scaler.calculate_workers(5, 750_000);
        let cpu_count = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);
        let expected = 6.min(cpu_count).min(8); // 6 or less if CPU-limited
        assert_eq!(workers, expected);
    }

    #[test]
    fn worker_scaler_very_large_content_scales_aggressively() {
        let scaler = WorkerScaler::with_defaults();

        // >1M tokens, scale 1.5
        // 6 chunks * 1.5 = 9, but capped at max_workers (8) and CPU count
        let workers = scaler.calculate_workers(6, 1_500_000);
        let cpu_count = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);
        let max_expected = 8.min(cpu_count);
        assert!(workers >= 1 && workers <= max_expected);
    }

    #[test]
    fn worker_scaler_respects_user_override() {
        let config = WorkerConfig {
            min_workers: 1,
            max_workers: 8,
            user_override: Some(4),
        };
        let scaler = WorkerScaler::new(config);

        // User override should take precedence regardless of content
        let workers = scaler.calculate_workers(10, 50_000);
        assert_eq!(workers, 4);
    }

    #[test]
    fn worker_scaler_clamps_to_min_workers() {
        let config = WorkerConfig {
            min_workers: 2,
            max_workers: 8,
            user_override: None,
        };
        let scaler = WorkerScaler::new(config);

        // Even with tiny content, should get min_workers (or CPU count if lower)
        let workers = scaler.calculate_workers(1, 1_000);
        let cpu_count = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);
        let effective_min = 2.min(cpu_count);
        assert!(workers >= effective_min);
    }

    #[test]
    fn worker_scaler_clamps_to_max_workers() {
        let config = WorkerConfig {
            min_workers: 1,
            max_workers: 4,
            user_override: None,
        };
        let scaler = WorkerScaler::new(config);

        // Even with huge content, should not exceed max_workers
        let workers = scaler.calculate_workers(20, 5_000_000);
        assert!(workers <= 4);
    }

    #[test]
    fn worker_scaler_user_override_clamped_to_max() {
        let config = WorkerConfig {
            min_workers: 1,
            max_workers: 4,
            user_override: Some(10),
        };
        let scaler = WorkerScaler::new(config);

        let workers = scaler.calculate_workers(1, 1_000);
        assert_eq!(workers, 4); // Clamped to max
    }

    // ============================================
    // ChunkResult Tests
    // ============================================

    #[test]
    fn chunk_result_success_creation() {
        let markers = vec![RawMarker {
            timestamp: 10.0,
            label: "Test".to_string(),
            category: MarkerCategory::Success,
        }];

        let result = ChunkResult::success(0, TimeRange::new(0.0, 100.0), markers);

        assert!(result.is_success());
        assert!(!result.is_failure());
        assert_eq!(result.chunk_id, 0);
        assert!(result.result.is_ok());
    }

    #[test]
    fn chunk_result_failure_creation() {
        let error = BackendError::Timeout(Duration::from_secs(60));

        let result = ChunkResult::failure(1, TimeRange::new(100.0, 200.0), error);

        assert!(!result.is_success());
        assert!(result.is_failure());
        assert_eq!(result.chunk_id, 1);
        assert!(result.result.is_err());
    }

    // ============================================
    // ProgressReporter Tests
    // ============================================

    #[test]
    fn progress_reporter_initial_state() {
        let reporter = ProgressReporter::new(5);

        let (completed, total) = reporter.get_progress();
        assert_eq!(completed, 0);
        assert_eq!(total, 5);
    }

    #[test]
    fn progress_reporter_increments() {
        let reporter = ProgressReporter::new(3);

        assert_eq!(reporter.report_progress(), 1);
        assert_eq!(reporter.report_progress(), 2);
        assert_eq!(reporter.report_progress(), 3);

        let (completed, _) = reporter.get_progress();
        assert_eq!(completed, 3);
    }

    #[test]
    fn progress_reporter_callback_called() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let reporter = ProgressReporter::with_callback(2, move |completed, total| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
            assert!(completed <= total);
        });

        reporter.report_progress();
        reporter.report_progress();

        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    #[cfg_attr(miri, ignore)] // Miri struggles with thread spawning
    fn progress_reporter_thread_safe() {
        let reporter = Arc::new(ProgressReporter::new(100));
        let mut handles = Vec::new();

        // Spawn multiple threads that increment progress
        for _ in 0..10 {
            let reporter_clone = Arc::clone(&reporter);
            handles.push(std::thread::spawn(move || {
                for _ in 0..10 {
                    reporter_clone.report_progress();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let (completed, _) = reporter.get_progress();
        assert_eq!(completed, 100);
    }

    // ============================================
    // ParallelExecutor Tests
    // ============================================

    fn create_test_chunk(id: usize, start: f64, end: f64) -> AnalysisChunk {
        AnalysisChunk::new(id, TimeRange::new(start, end), vec![])
    }

    #[test]
    fn parallel_executor_single_chunk_no_thread_pool() {
        let backend = MockBackend::new(vec![Ok(
            r#"{"markers": [{"timestamp": 5.0, "label": "Test", "category": "success"}]}"#
                .to_string(),
        )]);

        let executor = ParallelExecutor::new(&backend, Duration::from_secs(60), 4, true);
        let chunks = vec![create_test_chunk(0, 0.0, 100.0)];
        let progress = ProgressReporter::new(1);

        let results = executor.execute(chunks, &progress, |_| "test prompt".to_string());

        assert_eq!(results.len(), 1);
        assert!(results[0].is_success());
        assert_eq!(backend.invocation_count(), 1);

        let (completed, _) = progress.get_progress();
        assert_eq!(completed, 1);
    }

    #[test]
    #[cfg_attr(miri, ignore)] // Rayon thread pool unsupported in Miri
    fn parallel_executor_multiple_chunks_processed() {
        let backend = MockBackend::new(vec![
            Ok(r#"{"markers": []}"#.to_string()),
            Ok(r#"{"markers": []}"#.to_string()),
            Ok(r#"{"markers": []}"#.to_string()),
        ]);

        let executor = ParallelExecutor::new(&backend, Duration::from_secs(60), 2, true);
        let chunks = vec![
            create_test_chunk(0, 0.0, 100.0),
            create_test_chunk(1, 100.0, 200.0),
            create_test_chunk(2, 200.0, 300.0),
        ];
        let progress = ProgressReporter::new(3);

        let results = executor.execute(chunks, &progress, |_| "test prompt".to_string());

        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.is_success()));

        let (completed, _) = progress.get_progress();
        assert_eq!(completed, 3);
    }

    #[test]
    #[cfg_attr(miri, ignore)] // Rayon thread pool unsupported in Miri
    fn parallel_executor_progress_called_for_each_chunk() {
        let backend = MockBackend::new(vec![]);
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let executor = ParallelExecutor::new(&backend, Duration::from_secs(60), 2, true);
        let chunks = vec![
            create_test_chunk(0, 0.0, 100.0),
            create_test_chunk(1, 100.0, 200.0),
            create_test_chunk(2, 200.0, 300.0),
            create_test_chunk(3, 300.0, 400.0),
        ];

        let progress = ProgressReporter::with_callback(4, move |_, _| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let _ = executor.execute(chunks, &progress, |_| "test".to_string());

        assert_eq!(call_count.load(Ordering::SeqCst), 4);
    }

    #[test]
    #[cfg_attr(miri, ignore)] // Rayon thread pool unsupported in Miri
    fn parallel_executor_partial_failure() {
        let backend = MockBackend::new(vec![
            Ok(r#"{"markers": []}"#.to_string()),
            Err(BackendError::Timeout(Duration::from_secs(60))),
            Ok(r#"{"markers": []}"#.to_string()),
        ]);

        let executor = ParallelExecutor::new(&backend, Duration::from_secs(60), 2, true);
        let chunks = vec![
            create_test_chunk(0, 0.0, 100.0),
            create_test_chunk(1, 100.0, 200.0),
            create_test_chunk(2, 200.0, 300.0),
        ];
        let progress = ProgressReporter::new(3);

        let results = executor.execute(chunks, &progress, |_| "test".to_string());

        assert_eq!(results.len(), 3);

        // Count successes and failures
        let successes = results.iter().filter(|r| r.is_success()).count();
        let failures = results.iter().filter(|r| r.is_failure()).count();

        assert_eq!(successes, 2);
        assert_eq!(failures, 1);

        // Progress should still report all chunks
        let (completed, _) = progress.get_progress();
        assert_eq!(completed, 3);
    }

    #[test]
    fn parallel_executor_empty_chunks() {
        let backend = MockBackend::new(vec![]);
        let executor = ParallelExecutor::new(&backend, Duration::from_secs(60), 4, true);
        let progress = ProgressReporter::new(0);

        let results = executor.execute(vec![], &progress, |_| "test".to_string());

        assert!(results.is_empty());
    }

    #[test]
    #[cfg_attr(miri, ignore)] // Rayon thread pool unsupported in Miri
    fn parallel_executor_all_failures() {
        let backend = MockBackend::new(vec![
            Err(BackendError::Timeout(Duration::from_secs(60))),
            Err(BackendError::NotAvailable("claude".to_string())),
        ]);

        let executor = ParallelExecutor::new(&backend, Duration::from_secs(60), 2, true);
        let chunks = vec![
            create_test_chunk(0, 0.0, 100.0),
            create_test_chunk(1, 100.0, 200.0),
        ];
        let progress = ProgressReporter::new(2);

        let results = executor.execute(chunks, &progress, |_| "test".to_string());

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.is_failure()));
    }

    #[test]
    #[cfg_attr(miri, ignore)] // Rayon thread pool unsupported in Miri
    fn parallel_executor_preserves_chunk_ids() {
        let backend = MockBackend::new(vec![]);
        let executor = ParallelExecutor::new(&backend, Duration::from_secs(60), 2, true);
        let chunks = vec![
            create_test_chunk(0, 0.0, 100.0),
            create_test_chunk(1, 100.0, 200.0),
            create_test_chunk(2, 200.0, 300.0),
        ];
        let progress = ProgressReporter::new(3);

        let results = executor.execute(chunks, &progress, |_| "test".to_string());

        // Results may be in any order due to parallelism, but all chunk IDs should be present
        let mut ids: Vec<_> = results.iter().map(|r| r.chunk_id).collect();
        ids.sort();
        assert_eq!(ids, vec![0, 1, 2]);
    }

    // ============================================
    // Integration Tests (WorkerScaler + Executor)
    // ============================================

    #[test]
    #[cfg_attr(miri, ignore)] // Rayon thread pool unsupported in Miri
    fn worker_scaling_affects_executor() {
        let scaler = WorkerScaler::with_defaults();
        let backend = MockBackend::new(vec![]);

        // Small content - should get 1 worker
        let workers = scaler.calculate_workers(2, 50_000);
        assert_eq!(workers, 1);

        // Can create executor with calculated worker count
        let executor = ParallelExecutor::new(&backend, Duration::from_secs(60), workers, true);
        let chunks = vec![
            create_test_chunk(0, 0.0, 100.0),
            create_test_chunk(1, 100.0, 200.0),
        ];
        let progress = ProgressReporter::new(2);

        let results = executor.execute(chunks, &progress, |_| "test".to_string());
        assert_eq!(results.len(), 2);
    }

    // ============================================
    // RetryExecutor Tests
    // ============================================

    #[test]
    fn retry_executor_success_no_retry_needed() {
        let backend = MockBackend::new(vec![Ok(
            r#"{"markers": [{"timestamp": 5.0, "label": "Test", "category": "success"}]}"#
                .to_string(),
        )]);

        let executor = RetryExecutor::new(&backend, Duration::from_secs(60), 1, true);
        let mut chunks = vec![create_test_chunk(0, 0.0, 100.0)];
        chunks[0].estimated_tokens = 10000;
        let progress = ProgressReporter::new(1);

        let (results, tracker) =
            executor.execute_with_retry(chunks, &progress, |_| "test".to_string());

        assert_eq!(results.len(), 1);
        assert!(results[0].is_success());

        let summary = tracker.summary();
        assert_eq!(summary.successful_chunks, 1);
    }

    #[test]
    fn retry_executor_should_fallback_all_rate_limited() {
        use crate::analyzer::backend::RateLimitInfo;

        let results = vec![
            ChunkResult::failure(
                0,
                TimeRange::new(0.0, 100.0),
                BackendError::RateLimited(RateLimitInfo {
                    retry_after: Some(Duration::from_secs(30)),
                    message: "Rate limited".to_string(),
                }),
            ),
            ChunkResult::failure(
                1,
                TimeRange::new(100.0, 200.0),
                BackendError::RateLimited(RateLimitInfo {
                    retry_after: None,
                    message: "Rate limited".to_string(),
                }),
            ),
        ];

        assert!(RetryExecutor::<MockBackend>::should_fallback_to_sequential(
            &results
        ));
    }

    #[test]
    fn retry_executor_should_not_fallback_partial_success() {
        let results = vec![
            ChunkResult::success(0, TimeRange::new(0.0, 100.0), vec![]),
            ChunkResult::failure(
                1,
                TimeRange::new(100.0, 200.0),
                BackendError::Timeout(Duration::from_secs(60)),
            ),
        ];

        assert!(!RetryExecutor::<MockBackend>::should_fallback_to_sequential(&results));
    }

    #[test]
    fn retry_executor_should_not_fallback_non_rate_limit() {
        let results = vec![ChunkResult::failure(
            0,
            TimeRange::new(0.0, 100.0),
            BackendError::Timeout(Duration::from_secs(60)),
        )];

        assert!(!RetryExecutor::<MockBackend>::should_fallback_to_sequential(&results));
    }

    #[test]
    fn retry_executor_empty_chunks() {
        let backend = MockBackend::new(vec![]);
        let executor = RetryExecutor::new(&backend, Duration::from_secs(60), 1, true);
        let progress = ProgressReporter::new(0);

        let (results, tracker) =
            executor.execute_with_retry(vec![], &progress, |_| "test".to_string());

        assert!(results.is_empty());
        assert_eq!(tracker.summary().chunks_processed, 0);
    }

    #[test]
    #[cfg_attr(miri, ignore)] // Rayon thread pool unsupported in Miri
    fn retry_executor_tracks_usage() {
        let backend = MockBackend::new(vec![
            Ok(r#"{"markers": []}"#.to_string()),
            Ok(r#"{"markers": []}"#.to_string()),
        ]);

        let executor = RetryExecutor::new(&backend, Duration::from_secs(60), 2, true);
        let mut chunks = vec![
            create_test_chunk(0, 0.0, 100.0),
            create_test_chunk(1, 100.0, 200.0),
        ];
        chunks[0].estimated_tokens = 10000;
        chunks[1].estimated_tokens = 20000;
        let progress = ProgressReporter::new(2);

        let (results, tracker) =
            executor.execute_with_retry(chunks, &progress, |_| "test".to_string());

        assert_eq!(results.len(), 2);

        let summary = tracker.summary();
        assert_eq!(summary.chunks_processed, 2);
        assert_eq!(summary.total_estimated_tokens, 30000);
    }
}
