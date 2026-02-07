//! Prompt building and response parsing for LLM analysis.
//!
//! Contains all template rendering, token math, and response extraction
//! for the analyze, rename, and curate prompts.

use super::chunk::AnalysisChunk;
use super::result::ValidatedMarker;

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
pub fn build_analyze_prompt(
    chunk: &AnalysisChunk,
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
    if total_chunks <= 1 {
        // Single chunk = entire session, use full target range
        return (TARGET_TOTAL_MARKERS_MIN, TARGET_TOTAL_MARKERS_MAX);
    }

    // Distribute target markers across chunks
    let min_per_chunk = (TARGET_TOTAL_MARKERS_MIN / total_chunks).max(1);
    let max_per_chunk = (TARGET_TOTAL_MARKERS_MAX / total_chunks).max(min_per_chunk + 1);

    // Cap at reasonable per-chunk limits to avoid flooding
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

/// Build the rename prompt for filename suggestion.
pub(super) fn build_rename_prompt(
    markers: &[ValidatedMarker],
    total_duration: f64,
    current_filename: &str,
) -> String {
    const TEMPLATE: &str = include_str!("prompts/rename.txt");

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
        .replace(
            "{duration_minutes}",
            &format!("{:.1}", total_duration / 60.0),
        )
        .replace("{marker_count}", &markers.len().to_string())
        .replace("{current_filename}", current_filename)
        .replace("{markers_json}", &markers_json_str)
}

/// Extract the filename from an LLM rename response.
///
/// Handles Claude wrapper format and plain text.
pub(super) fn extract_rename_response(response: &str) -> Option<String> {
    let trimmed = response.trim();

    // Try Claude wrapper format
    if let Ok(wrapper) = serde_json::from_str::<serde_json::Value>(trimmed) {
        // Claude wrapper: {"type":"result","result":"the-filename",...}
        if wrapper.get("type").and_then(|t| t.as_str()) == Some("result") {
            if let Some(result) = wrapper.get("result").and_then(|r| r.as_str()) {
                let name = result.trim();
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
    }

    // Plain text response - take first line
    let first_line = trimmed.lines().next()?.trim();
    if !first_line.is_empty() {
        Some(first_line.to_string())
    } else {
        None
    }
}

/// Build the curation prompt for marker selection.
pub(super) fn build_curation_prompt(markers: &[ValidatedMarker], total_duration: f64) -> String {
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
        .replace(
            "{duration_minutes}",
            &format!("{:.1}", total_duration / 60.0),
        )
        .replace("{marker_count}", &markers.len().to_string())
        .replace("{markers_json}", &markers_json_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::chunk::TimeRange;

    #[test]
    fn build_analyze_prompt_substitutes_values() {
        let chunk = AnalysisChunk::new(
            0,
            TimeRange::new(10.0, 50.0),
            vec![crate::analyzer::types::AnalysisSegment {
                start_time: 10.0,
                end_time: 50.0,
                content: "Test content here".to_string(),
                estimated_tokens: 100,
                event_range: (0, 10),
            }],
        );

        let prompt = build_analyze_prompt(&chunk, 120.0, 3); // 3 chunks total

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
}
