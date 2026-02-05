//! Token tracking and retry coordination for analysis.
//!
//! This module provides:
//! - `RetryPolicy` - configuration for retry behavior
//! - `RetryCoordinator` - manages retry logic with exponential backoff
//! - `TokenTracker` - Observer pattern for usage metrics
//! - `ChunkUsage` - per-chunk usage information
//!
//! # Retry Strategy
//!
//! - Max 3 attempts per chunk
//! - Exponential backoff: 1s -> 2s -> 4s, capped at 60s
//! - Respects agent-provided retry-after duration for rate limits
//!
//! # Token Tracking (R6)
//!
//! Tracks usage across analysis for visibility:
//! - Estimated tokens per chunk
//! - Duration per chunk
//! - Success/failure rates
//! - Summary report at end

use std::time::{Duration, Instant};

/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts (default: 3)
    pub max_attempts: usize,
    /// Initial delay in milliseconds (default: 1000)
    pub initial_delay_ms: u64,
    /// Backoff multiplier (default: 2.0)
    pub backoff_multiplier: f64,
    /// Maximum delay in milliseconds (default: 60000)
    pub max_delay_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 60000,
        }
    }
}

impl RetryPolicy {
    /// Create a new retry policy with custom settings.
    pub fn new(
        max_attempts: usize,
        initial_delay_ms: u64,
        backoff_multiplier: f64,
        max_delay_ms: u64,
    ) -> Self {
        Self {
            max_attempts,
            initial_delay_ms,
            backoff_multiplier,
            max_delay_ms,
        }
    }

    /// Calculate delay for a given attempt number (0-indexed).
    ///
    /// Uses exponential backoff: delay = initial * (multiplier ^ attempt)
    /// Capped at max_delay_ms.
    pub fn delay_for_attempt(&self, attempt: usize) -> Duration {
        let delay_ms = self.initial_delay_ms as f64 * self.backoff_multiplier.powi(attempt as i32);
        let capped = delay_ms.min(self.max_delay_ms as f64) as u64;
        Duration::from_millis(capped)
    }

    /// Check if more retries are allowed.
    pub fn should_retry(&self, attempt: usize) -> bool {
        attempt < self.max_attempts
    }
}

/// Coordinates retry logic for chunk analysis.
pub struct RetryCoordinator {
    policy: RetryPolicy,
}

impl RetryCoordinator {
    /// Create a new retry coordinator with the given policy.
    pub fn new(policy: RetryPolicy) -> Self {
        Self { policy }
    }

    /// Create with default policy.
    pub fn with_defaults() -> Self {
        Self::new(RetryPolicy::default())
    }

    /// Get the retry policy.
    pub fn policy(&self) -> &RetryPolicy {
        &self.policy
    }

    /// Calculate wait duration for a retry.
    ///
    /// If `agent_retry_after` is provided (from rate limit response),
    /// uses that. Otherwise, uses exponential backoff based on attempt.
    pub fn wait_duration(&self, attempt: usize, agent_retry_after: Option<Duration>) -> Duration {
        // Agent-provided retry_after takes precedence
        if let Some(retry_after) = agent_retry_after {
            // Still cap at max_delay
            let capped = retry_after.min(Duration::from_millis(self.policy.max_delay_ms));
            return capped;
        }

        // Use exponential backoff
        self.policy.delay_for_attempt(attempt)
    }

    /// Check if we should retry based on attempt count.
    pub fn should_retry(&self, attempt: usize) -> bool {
        self.policy.should_retry(attempt)
    }

    /// Get the maximum number of attempts.
    pub fn max_attempts(&self) -> usize {
        self.policy.max_attempts
    }
}

/// Usage information for a single chunk.
#[derive(Debug, Clone)]
pub struct ChunkUsage {
    /// Chunk identifier
    pub chunk_id: usize,
    /// Estimated tokens for this chunk
    pub estimated_tokens: usize,
    /// Time spent analyzing this chunk
    pub duration: Duration,
    /// Whether analysis succeeded
    pub success: bool,
    /// Number of retry attempts
    pub attempts: usize,
}

impl ChunkUsage {
    /// Create a new chunk usage record.
    pub fn new(
        chunk_id: usize,
        estimated_tokens: usize,
        duration: Duration,
        success: bool,
        attempts: usize,
    ) -> Self {
        Self {
            chunk_id,
            estimated_tokens,
            duration,
            success,
            attempts,
        }
    }
}

/// Summary report of analysis usage.
#[derive(Debug, Clone)]
pub struct UsageSummary {
    /// Number of chunks processed
    pub chunks_processed: usize,
    /// Number of successful chunks
    pub successful_chunks: usize,
    /// Number of failed chunks
    pub failed_chunks: usize,
    /// Total estimated tokens
    pub total_estimated_tokens: usize,
    /// Total duration of analysis
    pub total_duration: Duration,
    /// Average tokens per chunk
    pub avg_tokens_per_chunk: usize,
    /// Average duration per chunk
    pub avg_duration_per_chunk: Duration,
    /// Success rate (0.0 - 1.0)
    pub success_rate: f64,
    /// Total retry attempts
    pub total_retries: usize,
}

/// Tracks token usage and analysis metrics.
///
/// Implements Observer pattern - receives updates from workers
/// and maintains aggregate statistics.
pub struct TokenTracker {
    /// Usage records for each chunk
    chunk_usage: Vec<ChunkUsage>,
    /// Start time of analysis
    start_time: Instant,
}

impl TokenTracker {
    /// Create a new token tracker.
    pub fn new() -> Self {
        Self {
            chunk_usage: Vec::new(),
            start_time: Instant::now(),
        }
    }

    /// Record usage for a chunk.
    pub fn record_chunk(
        &mut self,
        chunk_id: usize,
        estimated_tokens: usize,
        duration: Duration,
        success: bool,
        attempts: usize,
    ) {
        self.chunk_usage.push(ChunkUsage::new(
            chunk_id,
            estimated_tokens,
            duration,
            success,
            attempts,
        ));
    }

    /// Record a successful chunk analysis.
    pub fn record_success(
        &mut self,
        chunk_id: usize,
        estimated_tokens: usize,
        duration: Duration,
        attempts: usize,
    ) {
        self.record_chunk(chunk_id, estimated_tokens, duration, true, attempts);
    }

    /// Record a failed chunk analysis.
    pub fn record_failure(
        &mut self,
        chunk_id: usize,
        estimated_tokens: usize,
        duration: Duration,
        attempts: usize,
    ) {
        self.record_chunk(chunk_id, estimated_tokens, duration, false, attempts);
    }

    /// Get usage for a specific chunk.
    pub fn get_chunk_usage(&self, chunk_id: usize) -> Option<&ChunkUsage> {
        self.chunk_usage.iter().find(|u| u.chunk_id == chunk_id)
    }

    /// Get all chunk usages.
    pub fn all_chunks(&self) -> &[ChunkUsage] {
        &self.chunk_usage
    }

    /// Get total elapsed time since tracking started.
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Generate usage summary.
    pub fn summary(&self) -> UsageSummary {
        let chunks_processed = self.chunk_usage.len();
        let successful_chunks = self.chunk_usage.iter().filter(|u| u.success).count();
        let failed_chunks = chunks_processed - successful_chunks;

        let total_estimated_tokens: usize =
            self.chunk_usage.iter().map(|u| u.estimated_tokens).sum();

        let total_duration = self.elapsed();

        let avg_tokens_per_chunk = if chunks_processed > 0 {
            total_estimated_tokens / chunks_processed
        } else {
            0
        };

        let avg_duration_per_chunk = if chunks_processed > 0 {
            total_duration / chunks_processed as u32
        } else {
            Duration::ZERO
        };

        let success_rate = if chunks_processed > 0 {
            successful_chunks as f64 / chunks_processed as f64
        } else {
            0.0
        };

        let total_retries: usize = self
            .chunk_usage
            .iter()
            .map(|u| u.attempts.saturating_sub(1)) // Subtract 1 since first attempt isn't a retry
            .sum();

        UsageSummary {
            chunks_processed,
            successful_chunks,
            failed_chunks,
            total_estimated_tokens,
            total_duration,
            avg_tokens_per_chunk,
            avg_duration_per_chunk,
            success_rate,
            total_retries,
        }
    }

    /// Format summary for display.
    pub fn format_summary(&self) -> String {
        let summary = self.summary();
        let mut output = String::new();

        output.push_str("\nAnalysis Summary:\n");
        output.push_str(&format!(
            "   Chunks processed: {}\n",
            summary.chunks_processed
        ));
        output.push_str(&format!(
            "   Estimated tokens: ~{}\n",
            format_number(summary.total_estimated_tokens)
        ));
        output.push_str(&format!(
            "   Total duration: {}\n",
            format_duration(summary.total_duration)
        ));
        output.push_str(&format!(
            "   Success rate: {:.0}%\n",
            summary.success_rate * 100.0
        ));

        if summary.total_retries > 0 {
            output.push_str(&format!("   Retries: {}\n", summary.total_retries));
        }

        output
    }

    /// Check if analysis should prefer small chunks for retry.
    ///
    /// Used for smart retry ordering (R8).
    pub fn should_retry_small_chunks_first(&self, threshold_tokens: usize) -> bool {
        // If we've had failures, prefer retrying smaller chunks first
        let failures: Vec<_> = self.chunk_usage.iter().filter(|u| !u.success).collect();

        if failures.is_empty() {
            return false;
        }

        // Check if any failed chunk is below threshold
        failures
            .iter()
            .any(|u| u.estimated_tokens < threshold_tokens)
    }
}

impl Default for TokenTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Format a number with comma separators.
fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format a duration for display.
fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 60 {
        let mins = secs / 60;
        let remaining_secs = secs % 60;
        format!("{}m {}s", mins, remaining_secs)
    } else {
        format!("{}s", secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================
    // RetryPolicy Tests
    // ============================================

    #[test]
    fn retry_policy_default() {
        let policy = RetryPolicy::default();

        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.initial_delay_ms, 1000);
        assert!((policy.backoff_multiplier - 2.0).abs() < f64::EPSILON);
        assert_eq!(policy.max_delay_ms, 60000);
    }

    #[test]
    fn retry_policy_delay_for_attempt_0() {
        let policy = RetryPolicy::default();

        // First attempt: 1000ms
        let delay = policy.delay_for_attempt(0);
        assert_eq!(delay, Duration::from_millis(1000));
    }

    #[test]
    fn retry_policy_delay_for_attempt_1() {
        let policy = RetryPolicy::default();

        // Second attempt: 1000 * 2 = 2000ms
        let delay = policy.delay_for_attempt(1);
        assert_eq!(delay, Duration::from_millis(2000));
    }

    #[test]
    fn retry_policy_delay_for_attempt_2() {
        let policy = RetryPolicy::default();

        // Third attempt: 1000 * 4 = 4000ms
        let delay = policy.delay_for_attempt(2);
        assert_eq!(delay, Duration::from_millis(4000));
    }

    #[test]
    fn retry_policy_delay_capped_at_max() {
        let policy = RetryPolicy::default();

        // Many attempts: should be capped at 60s
        let delay = policy.delay_for_attempt(10);
        assert_eq!(delay, Duration::from_millis(60000));
    }

    #[test]
    fn retry_policy_should_retry_within_attempts() {
        let policy = RetryPolicy::default();

        assert!(policy.should_retry(0));
        assert!(policy.should_retry(1));
        assert!(policy.should_retry(2));
    }

    #[test]
    fn retry_policy_should_not_retry_at_max() {
        let policy = RetryPolicy::default();

        assert!(!policy.should_retry(3));
        assert!(!policy.should_retry(4));
    }

    #[test]
    fn retry_policy_custom() {
        let policy = RetryPolicy::new(5, 500, 1.5, 30000);

        assert_eq!(policy.max_attempts, 5);
        assert_eq!(policy.initial_delay_ms, 500);
        assert!((policy.backoff_multiplier - 1.5).abs() < f64::EPSILON);
        assert_eq!(policy.max_delay_ms, 30000);
    }

    // ============================================
    // RetryCoordinator Tests
    // ============================================

    #[test]
    fn retry_coordinator_uses_agent_retry_after() {
        let coordinator = RetryCoordinator::with_defaults();

        // Agent says wait 45 seconds
        let wait = coordinator.wait_duration(0, Some(Duration::from_secs(45)));
        assert_eq!(wait, Duration::from_secs(45));
    }

    #[test]
    fn retry_coordinator_caps_agent_retry_after() {
        let coordinator = RetryCoordinator::with_defaults();

        // Agent says wait 120 seconds, but max is 60
        let wait = coordinator.wait_duration(0, Some(Duration::from_secs(120)));
        assert_eq!(wait, Duration::from_secs(60));
    }

    #[test]
    fn retry_coordinator_uses_backoff_without_agent_retry() {
        let coordinator = RetryCoordinator::with_defaults();

        // No agent retry_after, use backoff
        let wait0 = coordinator.wait_duration(0, None);
        let wait1 = coordinator.wait_duration(1, None);
        let wait2 = coordinator.wait_duration(2, None);

        assert_eq!(wait0, Duration::from_millis(1000));
        assert_eq!(wait1, Duration::from_millis(2000));
        assert_eq!(wait2, Duration::from_millis(4000));
    }

    #[test]
    fn retry_coordinator_should_retry() {
        let coordinator = RetryCoordinator::with_defaults();

        assert!(coordinator.should_retry(0));
        assert!(coordinator.should_retry(2));
        assert!(!coordinator.should_retry(3));
    }

    #[test]
    fn retry_coordinator_max_attempts() {
        let coordinator = RetryCoordinator::with_defaults();
        assert_eq!(coordinator.max_attempts(), 3);
    }

    // ============================================
    // ChunkUsage Tests
    // ============================================

    #[test]
    fn chunk_usage_creation() {
        let usage = ChunkUsage::new(0, 50000, Duration::from_secs(30), true, 1);

        assert_eq!(usage.chunk_id, 0);
        assert_eq!(usage.estimated_tokens, 50000);
        assert_eq!(usage.duration, Duration::from_secs(30));
        assert!(usage.success);
        assert_eq!(usage.attempts, 1);
    }

    // ============================================
    // TokenTracker Tests
    // ============================================

    #[test]
    fn token_tracker_records_success() {
        let mut tracker = TokenTracker::new();

        tracker.record_success(0, 10000, Duration::from_secs(10), 1);

        let usage = tracker.get_chunk_usage(0).unwrap();
        assert_eq!(usage.chunk_id, 0);
        assert!(usage.success);
    }

    #[test]
    fn token_tracker_records_failure() {
        let mut tracker = TokenTracker::new();

        tracker.record_failure(1, 20000, Duration::from_secs(60), 3);

        let usage = tracker.get_chunk_usage(1).unwrap();
        assert_eq!(usage.chunk_id, 1);
        assert!(!usage.success);
        assert_eq!(usage.attempts, 3);
    }

    #[test]
    fn token_tracker_all_chunks() {
        let mut tracker = TokenTracker::new();

        tracker.record_success(0, 10000, Duration::from_secs(10), 1);
        tracker.record_success(1, 20000, Duration::from_secs(20), 1);
        tracker.record_failure(2, 15000, Duration::from_secs(30), 3);

        assert_eq!(tracker.all_chunks().len(), 3);
    }

    #[test]
    fn token_tracker_summary_all_success() {
        let mut tracker = TokenTracker::new();

        tracker.record_success(0, 10000, Duration::from_secs(10), 1);
        tracker.record_success(1, 20000, Duration::from_secs(20), 1);
        tracker.record_success(2, 15000, Duration::from_secs(15), 1);

        let summary = tracker.summary();

        assert_eq!(summary.chunks_processed, 3);
        assert_eq!(summary.successful_chunks, 3);
        assert_eq!(summary.failed_chunks, 0);
        assert_eq!(summary.total_estimated_tokens, 45000);
        assert!((summary.success_rate - 1.0).abs() < f64::EPSILON);
        assert_eq!(summary.total_retries, 0);
    }

    #[test]
    fn token_tracker_summary_with_failures() {
        let mut tracker = TokenTracker::new();

        tracker.record_success(0, 10000, Duration::from_secs(10), 1);
        tracker.record_failure(1, 20000, Duration::from_secs(30), 3);

        let summary = tracker.summary();

        assert_eq!(summary.chunks_processed, 2);
        assert_eq!(summary.successful_chunks, 1);
        assert_eq!(summary.failed_chunks, 1);
        assert!((summary.success_rate - 0.5).abs() < f64::EPSILON);
        assert_eq!(summary.total_retries, 2); // 3 attempts - 1 = 2 retries
    }

    #[test]
    fn token_tracker_summary_empty() {
        let tracker = TokenTracker::new();
        let summary = tracker.summary();

        assert_eq!(summary.chunks_processed, 0);
        assert_eq!(summary.successful_chunks, 0);
        assert_eq!(summary.total_estimated_tokens, 0);
        assert!((summary.success_rate - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn token_tracker_format_summary() {
        let mut tracker = TokenTracker::new();

        tracker.record_success(0, 100000, Duration::from_secs(30), 1);
        tracker.record_success(1, 150000, Duration::from_secs(45), 2);

        let formatted = tracker.format_summary();

        assert!(formatted.contains("Chunks processed: 2"));
        assert!(formatted.contains("250,000")); // Formatted with comma
        assert!(formatted.contains("Success rate: 100%"));
    }

    #[test]
    fn token_tracker_should_retry_small_chunks() {
        let mut tracker = TokenTracker::new();

        tracker.record_success(0, 100000, Duration::from_secs(30), 1);
        tracker.record_failure(1, 20000, Duration::from_secs(60), 3); // Small failed chunk

        // Should prefer retrying small chunks (< 50000 tokens)
        assert!(tracker.should_retry_small_chunks_first(50000));
    }

    #[test]
    fn token_tracker_no_small_chunks_to_retry() {
        let mut tracker = TokenTracker::new();

        tracker.record_success(0, 100000, Duration::from_secs(30), 1);
        tracker.record_failure(1, 80000, Duration::from_secs(60), 3); // Large failed chunk

        // No small chunks failed
        assert!(!tracker.should_retry_small_chunks_first(50000));
    }

    // ============================================
    // Format Helper Tests
    // ============================================

    #[test]
    fn format_number_small() {
        assert_eq!(format_number(123), "123");
    }

    #[test]
    fn format_number_thousands() {
        assert_eq!(format_number(1234), "1,234");
    }

    #[test]
    fn format_number_large() {
        assert_eq!(format_number(1234567), "1,234,567");
    }

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration(Duration::from_secs(45)), "45s");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(Duration::from_secs(90)), "1m 30s");
    }

    #[test]
    fn format_duration_many_minutes() {
        assert_eq!(format_duration(Duration::from_secs(125)), "2m 5s");
    }
}
