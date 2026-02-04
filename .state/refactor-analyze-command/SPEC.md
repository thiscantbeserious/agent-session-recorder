# Specification: Analyze Command

This document contains implementation specifications derived from real-world analysis.

## 1. Noise Patterns (From Real Cast Files)

Analysis of 73MB+ Claude sessions and Codex sessions reveals:

### 1.1 ANSI Escape Sequences

| Pattern | Description | Frequency in 73MB Claude file |
|---------|-------------|-------------------------------|
| `\x1b[?2026h/l` | Synchronized output mode | ~20K occurrences |
| `\x1b[2K` | Erase entire line | High |
| `\x1b[1A` | Cursor up one line | High |
| `\x1b[G` | Cursor to column 1 | High |
| `\x1b[38;5;NNNm` | 256-color foreground | High |
| `\x1b[48;5;NNNm` | 256-color background | High |
| `\x1b[2m` | Dim text | High |
| `\x1b[22m` | Normal intensity | High |
| `\x1b[7m` | Reverse video | Medium |
| `\x1b[27m` | Reverse video off | Medium |
| `\x1b[?25h/l` | Show/hide cursor | Low |
| `\x1b[?2004h/l` | Bracketed paste mode | Low |
| `\x1b[?1004h/l` | Focus tracking | Low |
| `\x1b[J` | Erase display | Medium |
| `\x1b[H` | Cursor home | Medium |
| `\x1b[Nr` | Set scroll region | Low |
| `\x1bM` | Reverse index | Low |

### 1.2 Claude-Specific Indicators

| Character | Meaning | Occurrences |
|-----------|---------|-------------|
| `⎿` | Command result/continuation | 17,703 |
| `⏺` | Activity indicator (tool call) | 17,533 |
| `⏸` | Plan mode indicator | 5,148 |
| `▖▗▘▝` | Claude logo animation blocks | 676 each |
| `❯` | Prompt indicator | High |

**Claude "Cerebrating" Spinner Characters:**
```
· ✢ ✳ ✶ ✻ ✽
```
These cycle with color-wave animation on the word "Cerebrating..."

### 1.3 Codex-Specific Indicators

| Character | Meaning |
|-----------|---------|
| `›` | Prompt/selection indicator |
| `■` | Error/interrupt indicator |
| `•` | Bullet/section marker |

### 1.4 Box Drawing Characters

```
Corners: ╭ ╮ ╰ ╯
Lines:   ─ │ ┌ ┐ └ ┘
Joins:   ├ ┤ ┬ ┴ ┼
Tables:  ┌───┬───┐
         │   │   │
         └───┴───┘
```

### 1.5 Progress/Status Patterns

These entire lines should be deduplicated (keep final state):

```
Pattern: \r followed by repeated content
Example: \r⠋ Building... \r⠙ Building... \r✓ Done

Claude thinking animation:
  "· Cerebrating..." → "✢ Cerebrating..." → "✳ Cerebrating..." (repeats)
```

---

## 2. JSON Response Schema

### 2.1 LLM Prompt Template

```
You are analyzing a terminal session recording from an AI coding agent.
Your task is to identify key engineering workflow moments and return them as markers.

## Session Content

Time range: {chunk_start_time}s - {chunk_end_time}s
Recording duration: {total_duration}s

<session_content>
{cleaned_content}
</session_content>

## Output Format

Return a JSON array of markers. Each marker must have:
- timestamp: Relative timestamp in seconds from chunk start (float)
- label: Brief description of what happened (string, max 80 chars)
- category: One of: "planning", "design", "implementation", "success", "failure"

Example:
```json
{
  "markers": [
    {"timestamp": 12.5, "label": "Started planning feature implementation", "category": "planning"},
    {"timestamp": 45.2, "label": "Build failed - missing dependency", "category": "failure"},
    {"timestamp": 78.9, "label": "Tests passing after fix", "category": "success"}
  ]
}
```

## Categories

- **planning**: Task breakdown, approach decisions, strategy discussion
- **design**: Architecture decisions, API design, data model choices
- **implementation**: Code writing, file modifications, command execution
- **success**: Tests passing, builds working, feature complete
- **failure**: Errors, test failures, failed approaches, issues encountered

Return ONLY the JSON. No explanation, no markdown, just the JSON object.
```

### 2.2 Response Schema (JSON Schema)

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["markers"],
  "properties": {
    "markers": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["timestamp", "label", "category"],
        "properties": {
          "timestamp": {
            "type": "number",
            "minimum": 0,
            "description": "Relative timestamp in seconds from chunk start"
          },
          "label": {
            "type": "string",
            "maxLength": 80,
            "description": "Brief description of the engineering moment"
          },
          "category": {
            "type": "string",
            "enum": ["planning", "design", "implementation", "success", "failure"]
          }
        }
      }
    }
  }
}
```

### 2.3 Rust Types for Parsing

```rust
use serde::{Deserialize, Serialize};

/// Raw marker from LLM response (before timestamp resolution)
#[derive(Debug, Clone, Deserialize)]
pub struct RawMarker {
    /// Relative timestamp (seconds from chunk start)
    pub timestamp: f64,
    /// Description of the moment
    pub label: String,
    /// Engineering category
    pub category: MarkerCategory,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MarkerCategory {
    Planning,
    Design,
    Implementation,
    Success,
    Failure,
}

/// LLM response wrapper
#[derive(Debug, Deserialize)]
pub struct AnalysisResponse {
    pub markers: Vec<RawMarker>,
}

/// Validated marker with absolute timestamp
#[derive(Debug, Clone)]
pub struct ValidatedMarker {
    /// Absolute timestamp in recording
    pub timestamp: f64,
    /// Marker label (may include category prefix)
    pub label: String,
    /// Original category
    pub category: MarkerCategory,
}

impl ValidatedMarker {
    /// Format for asciicast marker event
    pub fn to_marker_text(&self) -> String {
        let prefix = match self.category {
            MarkerCategory::Planning => "PLAN",
            MarkerCategory::Design => "DESIGN",
            MarkerCategory::Implementation => "IMPL",
            MarkerCategory::Success => "SUCCESS",
            MarkerCategory::Failure => "FAILURE",
        };
        format!("[{}] {}", prefix, self.label)
    }
}
```

### 2.4 JSON Extraction from Codex

Codex doesn't support JSON output mode. Extract JSON from text response:

```rust
/// Extract JSON from potentially wrapped text response
pub fn extract_json(response: &str) -> Result<AnalysisResponse, JsonExtractionError> {
    // Try direct parse first
    if let Ok(parsed) = serde_json::from_str(response) {
        return Ok(parsed);
    }

    // Look for JSON object in response
    let trimmed = response.trim();

    // Try to find { ... } boundaries
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        let json_str = &trimmed[start..=end];
        if let Ok(parsed) = serde_json::from_str(json_str) {
            return Ok(parsed);
        }
    }

    // Try code block extraction
    if let Some(json_str) = extract_from_code_block(trimmed) {
        if let Ok(parsed) = serde_json::from_str(json_str) {
            return Ok(parsed);
        }
    }

    Err(JsonExtractionError::NoValidJson {
        response: response.to_string(),
    })
}

fn extract_from_code_block(text: &str) -> Option<&str> {
    // Match ```json ... ``` or ``` ... ```
    let patterns = ["```json\n", "```\n"];
    for pattern in patterns {
        if let Some(start) = text.find(pattern) {
            let json_start = start + pattern.len();
            if let Some(end) = text[json_start..].find("```") {
                return Some(&text[json_start..json_start + end]);
            }
        }
    }
    None
}
```

---

## 3. Parallelization Logic

### 3.1 Chunk Count Calculation

```rust
/// Calculate number of chunks based on content size and agent token limit
pub fn calculate_chunk_count(
    total_tokens: usize,
    agent_budget: &TokenBudget,
) -> usize {
    let available = agent_budget.available_for_content();

    if total_tokens <= available {
        return 1; // Single chunk, no splitting needed
    }

    // Ceiling division to ensure all content fits
    let chunk_count = (total_tokens + available - 1) / available;

    // Cap at reasonable maximum
    chunk_count.min(16)
}

impl TokenBudget {
    /// Tokens available for actual content (after reservations)
    pub fn available_for_content(&self) -> usize {
        let reserved = self.reserved_for_prompt + self.reserved_for_output;
        let usable = self.max_input_tokens.saturating_sub(reserved);

        // Apply safety margin (10%)
        (usable as f64 * 0.90) as usize
    }
}
```

### 3.2 Worker Count Scaling

```rust
/// Calculate optimal worker count based on chunks and system resources
pub fn calculate_worker_count(
    chunk_count: usize,
    total_tokens: usize,
    config: &WorkerConfig,
) -> usize {
    // User override takes precedence
    if let Some(override_count) = config.user_override {
        return override_count.clamp(1, config.max_workers);
    }

    // Scale factor based on content size
    let scale_factor = match total_tokens {
        0..=100_000 => 0.5,        // Small: conservative
        100_001..=500_000 => 1.0,  // Medium: normal
        500_001..=1_000_000 => 1.2, // Large: slightly aggressive
        _ => 1.5,                   // Very large: aggressive
    };

    let scaled = (chunk_count as f64 * scale_factor).ceil() as usize;

    // Get system CPU count
    let cpu_count = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4);

    // Clamp to reasonable bounds
    scaled.clamp(
        config.min_workers,
        config.max_workers.min(cpu_count),
    )
}

pub struct WorkerConfig {
    pub min_workers: usize,      // Default: 1
    pub max_workers: usize,      // Default: 8
    pub user_override: Option<usize>,
}
```

### 3.3 Parallel Execution Flow

```rust
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub struct ParallelAnalyzer {
    backend: Box<dyn AgentBackend>,
    config: AnalyzerConfig,
}

impl ParallelAnalyzer {
    pub fn analyze(&self, chunks: Vec<AnalysisChunk>) -> AnalysisResult {
        let worker_count = calculate_worker_count(
            chunks.len(),
            chunks.iter().map(|c| c.estimated_tokens).sum(),
            &self.config.worker,
        );

        // Build dedicated thread pool
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count)
            .thread_name(|i| format!("analyzer-{}", i))
            .build()
            .expect("Failed to create thread pool");

        // Progress tracking
        let completed = Arc::new(AtomicUsize::new(0));
        let total = chunks.len();
        let progress = self.config.progress_reporter.clone();

        // Execute in parallel
        let results: Vec<ChunkResult> = pool.install(|| {
            chunks
                .into_par_iter()
                .map(|chunk| {
                    let result = self.analyze_chunk(&chunk);

                    // Update progress
                    let done = completed.fetch_add(1, Ordering::SeqCst) + 1;
                    progress.report(done, total, &chunk);

                    ChunkResult {
                        chunk_id: chunk.id,
                        time_range: chunk.time_range.clone(),
                        result,
                    }
                })
                .collect()
        });

        // Thread pool is dropped here - Rayon handles cleanup automatically

        self.aggregate_results(results)
    }

    fn analyze_chunk(&self, chunk: &AnalysisChunk) -> Result<Vec<RawMarker>, ChunkError> {
        // Build prompt
        let prompt = self.build_prompt(chunk);

        // Call agent with timeout
        let response = self.backend.invoke(&prompt, self.config.timeout)?;

        // Parse response
        let analysis: AnalysisResponse = extract_json(&response)?;

        Ok(analysis.markers)
    }
}
```

### 3.4 Fallback Logic

```rust
pub fn analyze_with_fallback(
    &self,
    chunks: Vec<AnalysisChunk>,
) -> AnalysisResult {
    // Try parallel first
    let parallel_result = self.analyze_parallel(&chunks);

    match parallel_result {
        Ok(result) if result.successful_chunks > 0 => {
            // At least partial success
            return Ok(result);
        }
        Err(e) if self.should_retry_sequential(&e) => {
            // Parallel failed, try sequential
            eprintln!("Parallel analysis failed, falling back to sequential...");
            return self.analyze_sequential(&chunks);
        }
        other => other,
    }
}

fn should_retry_sequential(&self, error: &AnalysisError) -> bool {
    matches!(error,
        AnalysisError::RateLimited { .. } |
        AnalysisError::AllChunksFailed { .. }
    )
}

fn analyze_sequential(&self, chunks: &[AnalysisChunk]) -> AnalysisResult {
    let mut all_markers = Vec::new();
    let mut errors = Vec::new();

    for chunk in chunks {
        match self.analyze_chunk(chunk) {
            Ok(markers) => all_markers.extend(markers),
            Err(e) => errors.push((chunk.id, e)),
        }

        // Small delay between sequential calls
        std::thread::sleep(Duration::from_millis(500));
    }

    // ...
}
```

### 3.5 Scaling Table

| Content Size | Est. Tokens | Chunks (160K budget) | Workers | Est. Time |
|-------------|-------------|---------------------|---------|-----------|
| 10 MB | ~50K | 1 | 1 | 30-60s |
| 25 MB | ~125K | 1 | 1 | 60-90s |
| 50 MB | ~250K | 2 | 2 | 60-90s |
| 75 MB | ~375K | 3 | 3 | 60-90s |
| 100 MB | ~500K | 4 | 4 | 60-90s |
| 150 MB | ~750K | 5 | 5 | 60-90s |
| 200 MB | ~1M | 7 | 6-7 | 90-120s |

Note: With parallel processing, wall-clock time stays relatively flat as content grows.

---

## 4. Agent CLI Invocation

### 4.1 Claude

```bash
# With JSON schema enforcement
claude --print \
  --output-format json \
  --json-schema '{"type":"object","properties":{"markers":{"type":"array"}}}' \
  --dangerously-skip-permissions \
  -p "$PROMPT"

# Or via stdin
echo "$PROMPT" | claude --print --output-format json --dangerously-skip-permissions
```

### 4.2 Codex

```bash
# Codex doesn't support JSON output - need text extraction
codex exec \
  --dangerously-bypass-approvals-and-sandbox \
  "$PROMPT"

# Via stdin
echo "$PROMPT" | codex exec --dangerously-bypass-approvals-and-sandbox
```

### 4.3 Gemini

```bash
# With JSON output
gemini \
  --output-format json \
  --yolo \
  "$PROMPT"

# Via stdin
echo "$PROMPT" | gemini --output-format json --yolo
```

---

## 5. Chunk Overlap Strategy

To maintain context continuity between chunks:

```rust
pub struct ChunkConfig {
    /// Overlap percentage (0.0 - 0.2 recommended)
    pub overlap_pct: f64,
    /// Minimum overlap in tokens
    pub min_overlap_tokens: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            overlap_pct: 0.10,  // 10% overlap
            min_overlap_tokens: 500,
        }
    }
}

pub fn create_chunks_with_overlap(
    content: &AnalysisContent,
    budget: &TokenBudget,
    config: &ChunkConfig,
) -> Vec<AnalysisChunk> {
    let available = budget.available_for_content();
    let overlap = (available as f64 * config.overlap_pct) as usize;
    let overlap = overlap.max(config.min_overlap_tokens);
    let step = available - overlap;

    // Create overlapping windows
    let mut chunks = Vec::new();
    let mut start_token = 0;
    let mut chunk_id = 0;

    while start_token < content.total_tokens {
        let end_token = (start_token + available).min(content.total_tokens);

        // Find segment boundaries
        let time_range = content.token_range_to_time_range(start_token, end_token);
        let chunk = AnalysisChunk::from_content(chunk_id, content, time_range);

        chunks.push(chunk);
        chunk_id += 1;
        start_token += step;

        if end_token >= content.total_tokens {
            break;
        }
    }

    chunks
}
```

### 5.1 Deduplication After Overlap

Chunks may report the same marker due to overlap. Deduplicate:

```rust
pub fn deduplicate_markers(
    markers: Vec<ValidatedMarker>,
    time_window: f64,  // e.g., 2.0 seconds
) -> Vec<ValidatedMarker> {
    if markers.is_empty() {
        return markers;
    }

    let mut sorted = markers;
    sorted.sort_by(|a, b| a.timestamp.partial_cmp(&b.timestamp).unwrap());

    let mut deduplicated = Vec::with_capacity(sorted.len());
    deduplicated.push(sorted.remove(0));

    for marker in sorted {
        let last = deduplicated.last().unwrap();

        // Check if within time window and similar category
        if (marker.timestamp - last.timestamp).abs() < time_window
            && marker.category == last.category
        {
            // Skip duplicate
            continue;
        }

        deduplicated.push(marker);
    }

    deduplicated
}
```
