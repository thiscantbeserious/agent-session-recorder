//! Progress reporting for parallel analysis.
//!
//! This module provides thread-safe progress tracking for chunk analysis.
//! It uses atomic operations for lock-free updates from multiple threads.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// Default progress reporter that writes to stderr.
///
/// Provides visual feedback during analysis with a simple progress line.
pub struct DefaultProgressReporter {
    /// Current count of completed chunks
    completed: Arc<AtomicUsize>,
    /// Total number of chunks
    total: usize,
    /// Whether to show output (can be disabled for quiet mode)
    show_output: bool,
    /// Whether progress has started
    started: AtomicBool,
}

impl DefaultProgressReporter {
    /// Create a new progress reporter.
    pub fn new(total: usize) -> Self {
        Self {
            completed: Arc::new(AtomicUsize::new(0)),
            total,
            show_output: true,
            started: AtomicBool::new(false),
        }
    }

    /// Create a progress reporter with output disabled.
    pub fn quiet(total: usize) -> Self {
        Self {
            completed: Arc::new(AtomicUsize::new(0)),
            total,
            show_output: false,
            started: AtomicBool::new(false),
        }
    }

    /// Report that analysis is starting.
    pub fn start(&self, chunk_count: usize, estimated_tokens: usize) {
        if !self.show_output {
            return;
        }

        if self
            .started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let tokens_display = format_tokens(estimated_tokens);
            eprintln!(
                "Analyzing session... ({} chunk{}, ~{} tokens)",
                chunk_count,
                if chunk_count == 1 { "" } else { "s" },
                tokens_display
            );
        }
    }

    /// Report that one chunk has completed.
    pub fn chunk_completed(&self, _chunk_id: usize, _duration_secs: f64) {
        let completed = self.completed.fetch_add(1, Ordering::SeqCst) + 1;

        if self.show_output {
            eprint!("\r  [{}/{}] Processing chunks...", completed, self.total);
            let _ = io::stderr().flush();
        }
    }

    /// Report that all chunks have completed.
    pub fn finish(&self, markers_added: usize) {
        if self.show_output {
            // Clear the progress line
            eprint!("\r                                                    \r");
            eprintln!(
                "Added {} marker{} to session",
                markers_added,
                if markers_added == 1 { "" } else { "s" }
            );
        }
    }

    /// Report partial success with some failures.
    pub fn finish_partial(
        &self,
        successful_chunks: usize,
        total_chunks: usize,
        markers_added: usize,
        failed_ranges: &[(f64, f64)],
    ) {
        self.finish_partial_with_errors(
            successful_chunks,
            total_chunks,
            markers_added,
            failed_ranges,
            &[],
        )
    }

    /// Report partial success with failures and error details.
    pub fn finish_partial_with_errors(
        &self,
        successful_chunks: usize,
        total_chunks: usize,
        markers_added: usize,
        failed_ranges: &[(f64, f64)],
        error_messages: &[String],
    ) {
        if !self.show_output {
            return;
        }

        // Clear the progress line
        eprint!("\r                                                    \r");

        eprintln!("Analysis partially complete:");
        eprintln!("   {}/{} chunks analyzed", successful_chunks, total_chunks);
        eprintln!("   {} markers added", markers_added);

        if !failed_ranges.is_empty() {
            eprintln!("   Failed time ranges:");
            for (i, (start, end)) in failed_ranges.iter().enumerate() {
                if i < error_messages.len() && !error_messages[i].is_empty() {
                    eprintln!("     - {:.1}s - {:.1}s ({})", start, end, error_messages[i]);
                } else {
                    eprintln!("     - {:.1}s - {:.1}s", start, end);
                }
            }
        }
    }

    /// Get current progress.
    pub fn get_progress(&self) -> (usize, usize) {
        (self.completed.load(Ordering::SeqCst), self.total)
    }

    /// Get the completed counter for sharing.
    pub fn completed_counter(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.completed)
    }
}

/// Format token count for display.
fn format_tokens(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{}K", tokens / 1_000)
    } else {
        format!("{}", tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_tokens_small() {
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(999), "999");
    }

    #[test]
    fn format_tokens_thousands() {
        assert_eq!(format_tokens(1_000), "1K");
        assert_eq!(format_tokens(50_000), "50K");
        assert_eq!(format_tokens(999_999), "999K");
    }

    #[test]
    fn format_tokens_millions() {
        assert_eq!(format_tokens(1_000_000), "1.0M");
        assert_eq!(format_tokens(1_500_000), "1.5M");
        assert_eq!(format_tokens(2_000_000), "2.0M");
    }

    #[test]
    fn progress_reporter_get_progress() {
        let reporter = DefaultProgressReporter::quiet(5);

        let (completed, total) = reporter.get_progress();
        assert_eq!(completed, 0);
        assert_eq!(total, 5);
    }

    #[test]
    fn progress_reporter_completed_counter_shared() {
        let reporter = DefaultProgressReporter::quiet(3);
        let counter = reporter.completed_counter();

        // Simulate parallel updates
        counter.fetch_add(1, Ordering::SeqCst);
        counter.fetch_add(1, Ordering::SeqCst);

        let (completed, _) = reporter.get_progress();
        assert_eq!(completed, 2);
    }
}
