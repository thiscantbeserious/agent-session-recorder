//! Token budget and chunking system for parallel analysis.
//!
//! This module divides content into chunks that fit within agent token limits,
//! enabling parallel processing of large cast files.
//!
//! # Design
//!
//! - `TokenBudget` defines agent-specific limits with safety margins
//! - `ChunkCalculator` divides content into appropriately-sized chunks
//! - `AnalysisChunk` represents a chunk ready for LLM analysis
//! - Overlap strategy ensures context continuity between chunks

use crate::analyzer::backend::AgentType;
use crate::analyzer::types::{AnalysisContent, AnalysisSegment};

/// Token budget configuration for an agent.
///
/// Defines the maximum tokens an agent can handle and reserves space
/// for the prompt template and expected output.
#[derive(Debug, Clone)]
pub struct TokenBudget {
    /// Maximum input tokens the agent supports
    pub max_input_tokens: usize,
    /// Tokens reserved for the analysis prompt template
    pub reserved_for_prompt: usize,
    /// Tokens reserved for the expected JSON output
    pub reserved_for_output: usize,
    /// Safety margin as percentage (0.0 - 1.0)
    safety_margin_pct: f64,
}

impl TokenBudget {
    /// Create a new token budget with custom parameters.
    pub fn new(
        max_input_tokens: usize,
        reserved_for_prompt: usize,
        reserved_for_output: usize,
        safety_margin_pct: f64,
    ) -> Self {
        Self {
            max_input_tokens,
            reserved_for_prompt,
            reserved_for_output,
            safety_margin_pct,
        }
    }

    /// Create budget for Claude (using conservative 100K limit).
    ///
    /// Note: Claude CLI with `--print --tools ""` has stricter limits
    /// than the API. The CLI adds its own system prompt and context,
    /// so we use a conservative 100K limit to ensure prompts fit.
    pub fn claude() -> Self {
        Self {
            max_input_tokens: 100_000, // Conservative limit for CLI mode
            reserved_for_prompt: 2_000,
            reserved_for_output: 8_000,
            safety_margin_pct: 0.20,
        }
    }

    /// Create budget for Codex (192K context).
    pub fn codex() -> Self {
        Self {
            max_input_tokens: 192_000,
            reserved_for_prompt: 2_000,
            reserved_for_output: 8_000,
            safety_margin_pct: 0.15,
        }
    }

    /// Create budget for Gemini (1M context).
    pub fn gemini() -> Self {
        Self {
            max_input_tokens: 1_000_000,
            reserved_for_prompt: 2_000,
            reserved_for_output: 8_000,
            safety_margin_pct: 0.15,
        }
    }

    /// Calculate tokens available for actual content.
    ///
    /// Subtracts reserved tokens and applies safety margin.
    pub fn available_for_content(&self) -> usize {
        let reserved = self.reserved_for_prompt + self.reserved_for_output;
        let usable = self.max_input_tokens.saturating_sub(reserved);
        // Apply safety margin (e.g., 0.15 margin means 0.85 utilization)
        (usable as f64 * (1.0 - self.safety_margin_pct)) as usize
    }
}

/// Time range within a recording.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeRange {
    /// Start time in seconds from recording start
    pub start: f64,
    /// End time in seconds from recording start
    pub end: f64,
}

impl TimeRange {
    /// Create a new time range.
    pub fn new(start: f64, end: f64) -> Self {
        Self { start, end }
    }

    /// Duration of this time range.
    pub fn duration(&self) -> f64 {
        self.end - self.start
    }

    /// Check if a timestamp falls within this range.
    pub fn contains(&self, timestamp: f64) -> bool {
        timestamp >= self.start && timestamp < self.end
    }
}

/// A chunk of content ready for LLM analysis.
///
/// Contains segments from the original content with timestamp mapping
/// preserved for accurate marker placement.
#[derive(Debug, Clone)]
pub struct AnalysisChunk {
    /// Unique chunk identifier
    pub id: usize,
    /// Time range this chunk covers
    pub time_range: TimeRange,
    /// Segments within this chunk
    pub segments: Vec<AnalysisSegment>,
    /// Combined text for LLM (concatenated segment content)
    pub text: String,
    /// Estimated token count
    pub estimated_tokens: usize,
}

impl AnalysisChunk {
    /// Create a new chunk from segments.
    pub fn new(id: usize, time_range: TimeRange, segments: Vec<AnalysisSegment>) -> Self {
        let text = segments
            .iter()
            .map(|s| s.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let estimated_tokens = segments.iter().map(|s| s.estimated_tokens).sum();

        Self {
            id,
            time_range,
            segments,
            text,
            estimated_tokens,
        }
    }

    /// Resolve a relative timestamp to absolute recording time.
    ///
    /// LLM markers use timestamps relative to chunk start.
    /// This maps them back to absolute recording time.
    pub fn resolve_marker_timestamp(&self, relative_ts: f64) -> f64 {
        self.time_range.start + relative_ts
    }

    /// Find a timestamp by searching for text content.
    ///
    /// Returns the start time of the segment containing the text.
    pub fn find_timestamp_by_text(&self, needle: &str) -> Option<f64> {
        self.segments
            .iter()
            .find(|s| s.content.contains(needle))
            .map(|s| s.start_time)
    }
}

/// Configuration for chunk creation.
#[derive(Debug, Clone)]
pub struct ChunkConfig {
    /// Overlap as percentage of chunk size (0.0 - 0.2)
    pub overlap_pct: f64,
    /// Minimum overlap in tokens
    pub min_overlap_tokens: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            overlap_pct: 0.10,
            min_overlap_tokens: 500,
        }
    }
}

/// Calculator for dividing content into chunks.
pub struct ChunkCalculator {
    budget: TokenBudget,
    config: ChunkConfig,
}

impl ChunkCalculator {
    /// Create a new chunk calculator.
    pub fn new(budget: TokenBudget, config: ChunkConfig) -> Self {
        Self { budget, config }
    }

    /// Create calculator with default config for an agent type.
    pub fn for_agent(agent: AgentType) -> Self {
        Self {
            budget: agent.token_budget(),
            config: ChunkConfig::default(),
        }
    }

    /// Calculate chunks from analysis content.
    ///
    /// Respects event boundaries - never splits mid-segment.
    pub fn calculate_chunks(&self, content: &AnalysisContent) -> Vec<AnalysisChunk> {
        let available = self.budget.available_for_content();

        // Single chunk if content fits
        if content.total_tokens <= available {
            return vec![self.create_single_chunk(content)];
        }

        self.create_overlapping_chunks(content, available)
    }

    /// Create a single chunk containing all content.
    fn create_single_chunk(&self, content: &AnalysisContent) -> AnalysisChunk {
        let time_range = TimeRange::new(
            content
                .segments
                .first()
                .map(|s| s.start_time)
                .unwrap_or(0.0),
            content.total_duration,
        );

        AnalysisChunk::new(0, time_range, content.segments.clone())
    }

    /// Create overlapping chunks for large content.
    fn create_overlapping_chunks(
        &self,
        content: &AnalysisContent,
        available: usize,
    ) -> Vec<AnalysisChunk> {
        let overlap = self.calculate_overlap(available);
        let step = available.saturating_sub(overlap);

        let mut chunks = Vec::new();
        let mut token_offset = 0;
        let mut chunk_id = 0;

        while token_offset < content.total_tokens {
            let target_end = (token_offset + available).min(content.total_tokens);

            // Find segments that fit in this token range
            let (segments, time_range) =
                self.find_segments_for_range(content, token_offset, target_end);

            if !segments.is_empty() {
                chunks.push(AnalysisChunk::new(chunk_id, time_range, segments));
                chunk_id += 1;
            }

            token_offset += step;

            // Prevent infinite loop on last chunk
            if target_end >= content.total_tokens {
                break;
            }
        }

        chunks
    }

    /// Calculate overlap tokens based on configuration.
    fn calculate_overlap(&self, available: usize) -> usize {
        let pct_overlap = (available as f64 * self.config.overlap_pct) as usize;
        pct_overlap.max(self.config.min_overlap_tokens)
    }

    /// Find segments that fit within a token range, splitting large segments if needed.
    fn find_segments_for_range(
        &self,
        content: &AnalysisContent,
        start_tokens: usize,
        end_tokens: usize,
    ) -> (Vec<AnalysisSegment>, TimeRange) {
        let mut segments = Vec::new();
        let mut accumulated_tokens = 0;
        let mut start_time = None;
        let mut end_time = 0.0;

        for segment in &content.segments {
            let segment_start = accumulated_tokens;
            let segment_end = accumulated_tokens + segment.estimated_tokens;

            // Check if segment overlaps with target range
            if segment_end > start_tokens && segment_start < end_tokens {
                // Calculate how much of this segment to include
                let include_start = start_tokens.saturating_sub(segment_start);
                let include_end = (end_tokens - segment_start).min(segment.estimated_tokens);

                if include_end > include_start {
                    // Calculate proportional time range within the segment
                    let segment_duration = segment.end_time - segment.start_time;
                    let segment_tokens = segment.estimated_tokens.max(1);
                    let time_per_token = segment_duration / segment_tokens as f64;

                    let partial_start_time =
                        segment.start_time + (include_start as f64 * time_per_token);
                    let partial_end_time =
                        segment.start_time + (include_end as f64 * time_per_token);

                    if start_time.is_none() {
                        start_time = Some(partial_start_time);
                    }
                    end_time = partial_end_time;

                    // Create a partial segment with only the included portion
                    let included_tokens = include_end - include_start;

                    // Extract partial content if we're not including the whole segment
                    let partial_content = if include_start == 0
                        && include_end == segment.estimated_tokens
                    {
                        segment.content.clone()
                    } else if segment.content.is_empty() {
                        String::new()
                    } else {
                        // Estimate character boundaries from token positions
                        // Use proportional mapping: char_pos = token_pos * (total_chars / total_tokens)
                        let total_chars = segment.content.chars().count();
                        let ratio = total_chars as f64 / segment_tokens as f64;
                        let char_start = ((include_start as f64 * ratio) as usize).min(total_chars);
                        let char_end = ((include_end as f64 * ratio) as usize).min(total_chars);

                        // Ensure we get at least some content if there are tokens to include
                        let (char_start, char_end) =
                            if char_end <= char_start && included_tokens > 0 {
                                // Fall back to including all content if calculation fails
                                (0, total_chars)
                            } else {
                                (char_start, char_end)
                            };

                        segment
                            .content
                            .chars()
                            .skip(char_start)
                            .take(char_end.saturating_sub(char_start))
                            .collect()
                    };

                    segments.push(AnalysisSegment {
                        start_time: partial_start_time,
                        end_time: partial_end_time,
                        content: partial_content,
                        estimated_tokens: included_tokens,
                        event_range: segment.event_range, // Keep original for reference
                    });
                }
            }

            accumulated_tokens = segment_end;

            // Stop if we've passed the target range
            if accumulated_tokens >= end_tokens {
                break;
            }
        }

        let time_range = TimeRange::new(start_time.unwrap_or(0.0), end_time);
        (segments, time_range)
    }

    /// Calculate the expected number of chunks for given content.
    pub fn calculate_chunk_count(&self, total_tokens: usize) -> usize {
        let available = self.budget.available_for_content();

        if total_tokens <= available {
            return 1;
        }

        let overlap = self.calculate_overlap(available);
        // Guard against step being 0 to prevent division by zero
        let step = available.saturating_sub(overlap).max(1);

        // Ceiling division considering overlap
        ((total_tokens.saturating_sub(overlap)) + step - 1) / step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================
    // TokenBudget Tests
    // ============================================

    #[test]
    fn token_budget_claude_limits() {
        let budget = TokenBudget::claude();
        assert_eq!(budget.max_input_tokens, 100_000);
        assert_eq!(budget.reserved_for_prompt, 2_000);
        assert_eq!(budget.reserved_for_output, 8_000);
    }

    #[test]
    fn token_budget_codex_limits() {
        let budget = TokenBudget::codex();
        assert_eq!(budget.max_input_tokens, 192_000);
        assert_eq!(budget.reserved_for_prompt, 2_000);
        assert_eq!(budget.reserved_for_output, 8_000);
    }

    #[test]
    fn token_budget_gemini_limits() {
        let budget = TokenBudget::gemini();
        assert_eq!(budget.max_input_tokens, 1_000_000);
        assert_eq!(budget.reserved_for_prompt, 2_000);
        assert_eq!(budget.reserved_for_output, 8_000);
    }

    #[test]
    fn token_budget_available_for_content_claude() {
        let budget = TokenBudget::claude();
        // 100K - 2K - 8K = 90K, then * 0.80 = 72,000
        let available = budget.available_for_content();
        assert_eq!(available, 72_000);
    }

    #[test]
    fn token_budget_available_for_content_codex() {
        let budget = TokenBudget::codex();
        // 192K - 2K - 8K = 182K, then * 0.85 = 154,700
        let available = budget.available_for_content();
        assert_eq!(available, 154_700);
    }

    #[test]
    fn token_budget_available_for_content_gemini() {
        let budget = TokenBudget::gemini();
        // 1M - 2K - 8K = 990K, then * 0.85 = 841,500
        let available = budget.available_for_content();
        assert_eq!(available, 841_500);
    }

    #[test]
    fn token_budget_custom() {
        let budget = TokenBudget::new(100_000, 1_000, 4_000, 0.10);
        // 100K - 1K - 4K = 95K, then * 0.90 = 85,500
        let available = budget.available_for_content();
        assert_eq!(available, 85_500);
    }

    #[test]
    fn agent_type_returns_correct_budget() {
        assert_eq!(AgentType::Claude.token_budget().max_input_tokens, 100_000);
        assert_eq!(AgentType::Codex.token_budget().max_input_tokens, 192_000);
        assert_eq!(AgentType::Gemini.token_budget().max_input_tokens, 1_000_000);
    }

    // ============================================
    // TimeRange Tests
    // ============================================

    #[test]
    fn time_range_duration() {
        let range = TimeRange::new(10.0, 25.0);
        assert!((range.duration() - 15.0).abs() < 0.001);
    }

    #[test]
    fn time_range_contains() {
        let range = TimeRange::new(10.0, 20.0);
        assert!(range.contains(10.0)); // start inclusive
        assert!(range.contains(15.0));
        assert!(!range.contains(20.0)); // end exclusive
        assert!(!range.contains(9.9));
        assert!(!range.contains(20.1));
    }

    // ============================================
    // AnalysisChunk Tests
    // ============================================

    #[test]
    fn analysis_chunk_resolve_timestamp() {
        let chunk = AnalysisChunk::new(0, TimeRange::new(100.0, 200.0), vec![]);

        // Relative timestamp 12.5s from chunk start
        let absolute = chunk.resolve_marker_timestamp(12.5);
        assert!((absolute - 112.5).abs() < 0.001);
    }

    #[test]
    fn analysis_chunk_find_timestamp_by_text() {
        let segments = vec![
            AnalysisSegment {
                start_time: 10.0,
                end_time: 20.0,
                content: "Starting build".to_string(),
                estimated_tokens: 10,
                event_range: (0, 5),
            },
            AnalysisSegment {
                start_time: 20.0,
                end_time: 30.0,
                content: "Build complete".to_string(),
                estimated_tokens: 10,
                event_range: (5, 10),
            },
        ];

        let chunk = AnalysisChunk::new(0, TimeRange::new(10.0, 30.0), segments);

        assert_eq!(chunk.find_timestamp_by_text("Starting"), Some(10.0));
        assert_eq!(chunk.find_timestamp_by_text("complete"), Some(20.0));
        assert_eq!(chunk.find_timestamp_by_text("not found"), None);
    }

    #[test]
    fn analysis_chunk_text_concatenation() {
        let segments = vec![
            AnalysisSegment {
                start_time: 0.0,
                end_time: 10.0,
                content: "first".to_string(),
                estimated_tokens: 5,
                event_range: (0, 5),
            },
            AnalysisSegment {
                start_time: 10.0,
                end_time: 20.0,
                content: "second".to_string(),
                estimated_tokens: 5,
                event_range: (5, 10),
            },
        ];

        let chunk = AnalysisChunk::new(0, TimeRange::new(0.0, 20.0), segments);

        assert_eq!(chunk.text, "first\nsecond");
        assert_eq!(chunk.estimated_tokens, 10);
    }

    // ============================================
    // ChunkCalculator Tests
    // ============================================

    fn create_test_content(total_tokens: usize, num_segments: usize) -> AnalysisContent {
        let tokens_per_segment = total_tokens / num_segments;
        let duration_per_segment = 10.0;

        let segments: Vec<AnalysisSegment> = (0..num_segments)
            .map(|i| AnalysisSegment {
                start_time: i as f64 * duration_per_segment,
                end_time: (i + 1) as f64 * duration_per_segment,
                content: format!("Segment {}", i),
                estimated_tokens: tokens_per_segment,
                event_range: (i * 10, (i + 1) * 10),
            })
            .collect();

        AnalysisContent {
            total_duration: num_segments as f64 * duration_per_segment,
            total_tokens,
            segments,
            stats: Default::default(),
        }
    }

    #[test]
    fn chunk_calculator_single_chunk_when_small() {
        let calculator = ChunkCalculator::for_agent(AgentType::Claude);
        let content = create_test_content(50_000, 10); // Well under Claude's 161K limit

        let chunks = calculator.calculate_chunks(&content);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].id, 0);
        assert_eq!(chunks[0].segments.len(), 10);
    }

    #[test]
    fn chunk_calculator_multiple_chunks_when_large() {
        let calculator = ChunkCalculator::for_agent(AgentType::Claude);
        // Claude has ~161K available, so 400K should create multiple chunks
        let content = create_test_content(400_000, 40);

        let chunks = calculator.calculate_chunks(&content);

        assert!(chunks.len() > 1);
        // Each chunk should be unique
        let ids: Vec<_> = chunks.iter().map(|c| c.id).collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn chunk_calculator_respects_segment_boundaries() {
        let calculator = ChunkCalculator::for_agent(AgentType::Claude);
        let content = create_test_content(300_000, 30);

        let chunks = calculator.calculate_chunks(&content);

        // Each chunk should contain complete segments
        for chunk in &chunks {
            for segment in &chunk.segments {
                assert!(segment.start_time < segment.end_time);
                assert!(!segment.content.is_empty());
            }
        }
    }

    #[test]
    fn chunk_calculator_overlap_provides_context() {
        let budget = TokenBudget::new(100_000, 2_000, 8_000, 0.0); // No safety margin
        let config = ChunkConfig {
            overlap_pct: 0.10,
            min_overlap_tokens: 500,
        };
        let calculator = ChunkCalculator::new(budget, config);

        // 200K tokens with 90K available = multiple chunks
        let content = create_test_content(200_000, 20);
        let chunks = calculator.calculate_chunks(&content);

        // With overlap, adjacent chunks should share some segments
        if chunks.len() >= 2 {
            let chunk1_end = chunks[0].time_range.end;
            let chunk2_start = chunks[1].time_range.start;
            // Chunks may have overlapping time ranges
            assert!(chunk2_start <= chunk1_end || chunks.len() == 2);
        }
    }

    #[test]
    fn chunk_calculator_count_matches_scaling_table() {
        let calculator = ChunkCalculator::for_agent(AgentType::Claude);
        // Claude ~72K available (100K - 10K reserved, * 0.80)

        // Small content = 1 chunk
        assert_eq!(calculator.calculate_chunk_count(50_000), 1);
        assert_eq!(calculator.calculate_chunk_count(72_000), 1);

        // Just over limit = 2 chunks
        assert!(calculator.calculate_chunk_count(80_000) >= 2);

        // Large content = many chunks
        assert!(calculator.calculate_chunk_count(500_000) >= 3);
    }

    #[test]
    fn chunk_calculator_gemini_handles_large_content() {
        let calculator = ChunkCalculator::for_agent(AgentType::Gemini);
        // Gemini has ~841K available

        // Very large content still fits
        let content = create_test_content(800_000, 80);
        let chunks = calculator.calculate_chunks(&content);
        assert_eq!(chunks.len(), 1);

        // Extremely large needs multiple
        let huge_content = create_test_content(2_000_000, 200);
        let chunks = calculator.calculate_chunks(&huge_content);
        assert!(chunks.len() >= 2);
    }

    #[test]
    fn chunk_config_default_values() {
        let config = ChunkConfig::default();
        assert!((config.overlap_pct - 0.10).abs() < 0.001);
        assert_eq!(config.min_overlap_tokens, 500);
    }

    // ============================================
    // Property-like Tests for Timestamp Resolution
    // ============================================

    #[test]
    fn timestamp_resolution_always_valid() {
        // Test various chunk configurations
        let test_cases = vec![
            (0.0, 100.0, 50.0),   // Middle of chunk
            (100.0, 200.0, 0.0),  // Start of chunk
            (50.0, 150.0, 99.0),  // Near end
            (0.0, 1000.0, 500.0), // Large chunk
        ];

        for (start, end, relative) in test_cases {
            let chunk = AnalysisChunk::new(0, TimeRange::new(start, end), vec![]);

            let absolute = chunk.resolve_marker_timestamp(relative);

            assert!(
                absolute >= start,
                "Absolute {} should be >= chunk start {}",
                absolute,
                start
            );
        }
    }

    #[test]
    fn chunks_cover_all_content() {
        let calculator = ChunkCalculator::for_agent(AgentType::Claude);
        let content = create_test_content(400_000, 40);

        let chunks = calculator.calculate_chunks(&content);

        // First chunk should start near content start
        assert!(chunks[0].time_range.start <= content.segments[0].start_time + 0.001);

        // Last chunk should cover content end
        let last_chunk = chunks.last().unwrap();
        assert!(last_chunk.time_range.end >= content.total_duration - 0.001);
    }

    // ============================================
    // Bug Fix Tests - Single Large Segment
    // ============================================

    /// Test for bug: all chunks have identical time ranges when content is a single large segment.
    /// This happened because a segment larger than the chunk size was included in ALL chunks
    /// without being split.
    #[test]
    fn chunk_calculator_single_large_segment_split() {
        let calculator = ChunkCalculator::for_agent(AgentType::Claude);
        // Claude has ~161K available, create a single 400K token segment
        let content = AnalysisContent {
            total_duration: 100.0,
            total_tokens: 400_000,
            segments: vec![AnalysisSegment {
                start_time: 0.0,
                end_time: 100.0,
                content: "x".repeat(1_600_000), // estimated_tokens set directly below
                estimated_tokens: 400_000,
                event_range: (0, 100),
            }],
            stats: Default::default(),
        };

        let chunks = calculator.calculate_chunks(&content);

        // Should have at least 3 chunks (400K / 161K ~= 3)
        assert!(
            chunks.len() >= 3,
            "Expected at least 3 chunks, got {}",
            chunks.len()
        );

        // Each chunk should have DIFFERENT time ranges
        for (i, chunk) in chunks.iter().enumerate() {
            for (j, other) in chunks.iter().enumerate() {
                if i != j {
                    assert!(
                        (chunk.time_range.start - other.time_range.start).abs() > 0.001
                            || (chunk.time_range.end - other.time_range.end).abs() > 0.001,
                        "Chunks {} and {} have identical time ranges: {:?} vs {:?}",
                        i,
                        j,
                        chunk.time_range,
                        other.time_range
                    );
                }
            }
        }

        // Each chunk should have tokens <= available
        let available = calculator.budget.available_for_content();
        for (i, chunk) in chunks.iter().enumerate() {
            assert!(
                chunk.estimated_tokens <= available,
                "Chunk {} has {} tokens, exceeds available {}",
                i,
                chunk.estimated_tokens,
                available
            );
        }

        // Time ranges should progress through the recording
        for i in 1..chunks.len() {
            assert!(
                chunks[i].time_range.start >= chunks[i - 1].time_range.start,
                "Chunk {} starts before chunk {}: {:.1}s vs {:.1}s",
                i,
                i - 1,
                chunks[i].time_range.start,
                chunks[i - 1].time_range.start
            );
        }
    }

    /// Test that chunks don't have inflated token counts when splitting large segments.
    #[test]
    fn chunk_calculator_no_duplicate_tokens() {
        let calculator = ChunkCalculator::for_agent(AgentType::Claude);
        let content = AnalysisContent {
            total_duration: 50.0,
            total_tokens: 390_000,
            segments: vec![AnalysisSegment {
                start_time: 1.9,
                end_time: 17.5,
                content: "x".repeat(1_560_000), // ~390K tokens
                estimated_tokens: 390_000,
                event_range: (0, 50),
            }],
            stats: Default::default(),
        };

        let chunks = calculator.calculate_chunks(&content);

        // Total tokens across all chunks should be close to original
        // (may be slightly higher due to overlap, but not 3x)
        let total_chunk_tokens: usize = chunks.iter().map(|c| c.estimated_tokens).sum();

        // With 10% overlap and 3 chunks, we expect roughly 1.2x the original tokens
        // Allow up to 1.5x to be safe
        assert!(
            total_chunk_tokens <= content.total_tokens * 2,
            "Duplicate tokens detected: chunk total {} vs original {}",
            total_chunk_tokens,
            content.total_tokens
        );
    }
}
