# Specification: Analyze Command

This document contains implementation specifications derived from real-world analysis.

## 1. Noise Patterns (From Real Cast Files)

Analysis performed on:
- Claude: 73MB session (`20260114-152743-6732.cast`)
- Codex: 4.3MB session (`codex-pattern-analysis.cast`)
- Gemini: 176MB session (`gemini-pattern-analysis.cast`)

### 1.1 ANSI Escape Sequences (Common to All)

| Pattern | Description | Notes |
|---------|-------------|-------|
| `\x1b[?2026h/l` | Synchronized output mode | All agents use heavily |
| `\x1b[2K` | Erase entire line | High frequency |
| `\x1b[1A` | Cursor up one line | High frequency |
| `\x1b[G` | Cursor to column 1 | High frequency |
| `\x1b[38;5;NNNm` | 256-color foreground | Gemini uses extensively |
| `\x1b[48;5;NNNm` | 256-color background | Codex: `234` (dark bg) |
| `\x1b[2m` / `\x1b[22m` | Dim / Normal intensity | All agents |
| `\x1b[?25h/l` | Show/hide cursor | All agents |
| `\x1b[J` | Erase display | All agents |

### 1.2 Claude-Specific Patterns

**Indicators (from 73MB session):**

| Char | Unicode | Count | Meaning |
|------|---------|-------|---------|
| `─` | U+2500 | 3,381,205 | Horizontal line |
| `│` | U+2502 | 121,244 | Vertical line |
| `╌` | U+254C | 69,680 | Dashed line |
| `→` | U+2192 | 27,449 | Arrow |
| `·` | U+00B7 | 27,630 | Middle dot |
| `┼` | U+253C | 25,675 | Cross |
| `⎿` | U+23BF | 17,703 | Result continuation |
| `⏺` | U+23FA | 17,533 | Activity indicator (tool call) |
| `❯` | U+276F | 15,030 | Prompt indicator |
| `…` | U+2026 | 9,604 | Ellipsis |
| `├` `┤` | U+251C/2524 | 9,125 | T-junctions |
| `┬` `┴` | U+252C/2534 | 6,755 | T-junctions |
| `⏵` | U+23F5 | 5,822 | Play indicator |
| `⏸` | U+23F8 | 5,148 | Plan mode indicator |
| `↓` | U+2193 | 3,240 | Down arrow |
| `┌┐└┘` | U+250C etc | 2,365 | Corners |
| `╭╮╰╯` | U+256D etc | 955 | Rounded corners |

**Claude "Cerebrating" Spinner (cycles with color-wave):**

| Char | Unicode | Count |
|------|---------|-------|
| `✻` | U+273B | 1,772 |
| `✳` | U+2733 | 1,575 |
| `✢` | U+2722 | 1,436 |
| `✶` | U+2736 | 1,428 |
| `✽` | U+273D | 1,067 |

**Logo Animation Blocks:**

| Char | Unicode | Count |
|------|---------|-------|
| `▖▗▘▝` | U+2596-259D | 676 each |

### 1.3 Codex-Specific Patterns

**Indicators (from 4.3MB session):**

| Char | Unicode | Count | Meaning |
|------|---------|-------|---------|
| `─` | U+2500 | 1,060 | Horizontal line |
| `·` | U+00B7 | 719 | Middle dot |
| `•` | U+2022 | 544 | Bullet point |
| `›` | U+203A | 286 | Prompt/selection |
| `◦` | U+25E6 | 256 | Open bullet |
| `━` | U+2501 | 80 | Heavy horizontal |
| `│` | U+2502 | 76 | Vertical line |
| `└` | U+2514 | 49 | Corner |
| `…` | U+2026 | 39 | Ellipsis |
| `✔` | U+2714 | 15 | Checkmark |
| `⋮` | U+22EE | 4 | Vertical ellipsis |
| `╭╮╰╯` | U+256D etc | 2 | Rounded corners |
| `⚠` | U+26A0 | 2 | Warning |

### 1.4 Gemini-Specific Patterns

**Indicators (from 176MB session):**

| Char | Unicode | Count | Meaning |
|------|---------|-------|---------|
| `─` | U+2500 | 12,722,194 | Horizontal line (MASSIVE) |
| `█` | U+2588 | 779,841 | Full block (progress bars) |
| `│` | U+2502 | 688,101 | Vertical line |
| `░` | U+2591 | 390,874 | Light shade (progress) |
| `✦` | U+2726 | 73,083 | Four-pointed star |
| `╭╰` | U+256D/2570 | 66,103 | Rounded corners |
| `╮╯` | U+256E/256F | 65,965 | Rounded corners |
| `═` | U+2550 | 62,419 | Double horizontal |
| `✓` | U+2713 | 61,394 | Check mark |
| `…` | U+2026 | 24,856 | Ellipsis |
| `ℹ` | U+2139 | 3,191 | Info symbol |
| `☐` | U+2610 | 2,973 | Unchecked box |
| `»` | U+00BB | 2,925 | Double angle quote |
| `▼` | U+25BC | 632 | Down triangle |
| `→` | U+2192 | 406 | Arrow |
| `●` | U+25CF | 334 | Filled circle |
| `▲` | U+25B2 | 317 | Up triangle |
| `✕` | U+2715 | 164 | X mark |
| `┌┐└┘` | U+250C etc | 136 | Sharp corners |

**Gemini Braille Spinner (standard Braille pattern):**

| Char | Unicode | Count |
|------|---------|-------|
| `⠋` | U+280B | 588 |
| `⠙` | U+2819 | 574 |
| `⠹` | U+2839 | 575 |
| `⠸` | U+2838 | 574 |
| `⠼` | U+283C | 600 |
| `⠴` | U+2834 | 581 |
| `⠦` | U+2826 | 560 |
| `⠧` | U+2827 | 563 |
| `⠇` | U+2807 | 592 |
| `⠏` | U+280F | 613 |

### 1.5 Summary: What to Strip vs Keep

#### STRIP (Visual Only - No Semantic Value)

**All Agents:**
- ANSI escape sequences (colors, cursor movement, erase)
- Box drawing characters (`─│┌┐└┘├┤┬┴┼╭╮╰╯═`)
- Block characters for progress bars (`█░▒▓`)

**Claude-specific:**
- Cerebrating spinner: `✢ ✳ ✶ ✻ ✽`
- Logo blocks: `▖▗▘▝`

**Codex-specific:**
- Visual bullets: `› • ◦ ⋮`

**Gemini-specific:**
- Braille spinner: `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`

#### KEEP (Semantic Meaning - Helps LLM Understand)

| Character | Meaning | Why Keep |
|-----------|---------|----------|
| `✓` `✔` | Success/Pass | LLM needs to identify success moments |
| `✕` | Failure/Error | LLM needs to identify failure moments |
| `⚠` | Warning | Indicates issues worth noting |
| `ℹ` | Information | Contextual info marker |
| `☐` `☑` | Unchecked/Checked | Task completion state |
| `❯` | Prompt | May indicate user input context |

**Rationale**: The LLM uses these semantic indicators to understand success/failure workflow moments (R3, AC3). Stripping them removes critical information for marker quality.

### 1.6 Progress/Status Patterns

These lines repeat heavily and should be deduplicated:

```
Claude: "· Cerebrating..." → "✢ Cerebrating..." → "✳ Cerebrating..." (repeats)
Gemini: "⠋ Waiting..." → "⠙ Waiting..." → "⠹ Waiting..." (Braille cycle)
Codex: Menu selection cycling with › indicator
```

### 1.7 Before/After Examples (Real Data)

These examples demonstrate the transformation from raw cast events to clean content.

#### Codex Example

**Raw Event (631 bytes):**
```
\x1b[?2026h\x1b[1;61H\x1b[0m\x1b[49m\x1b[K\x1b[?25l\x1b[1;61H\x1b[48;5;234m\x1b[38;5;7m
\x1b[2m\x1b[38;5;8m \x1b[22m› 1. Allow Codex to work in this folder without asking for approval
\x1b[2m\x1b[38;5;8m \x1b[22m  2. Require approval of edits and commands
\x1b[?25h\x1b[?2026l
```

**Clean Output (110 bytes):**
```
› 1. Allow Codex to work in this folder without asking for approval  2. Require approval of edits and commands
```

**Compression: 82% reduction**

#### Gemini Example

**Raw Event (290 bytes):**
```
\x1b[2K\x1b[1A\x1b[2K\x1b[1A\x1b[2K\x1b[G\x1b[38;5;35m?\x1b[39m \x1b[1mEnter your
message\x1b[22m\x1b[38;5;239m (Ctrl+C to quit)\x1b[39m\x1b[57G\x1b[38;5;239m\x1b[39m
\x1b[G\x1b[2K\x1b[1A\x1b[2K\x1b[G\x1b[38;5;6m>\x1b[39m can you understand the
current project? i want to have a detailed session to\n  ahve all kind of weird
output that you can produce
```

**Clean Output (131 bytes):**
```
> can you understand the current project? i want to have a detailed session to
  ahve all kind of weird output that you can produce
```

**Compression: 55% reduction**

#### Claude Example

**Raw Event (847 bytes):**
```
\x1b[?2026h\x1b[16;3H\x1b[0m\x1b[38;5;174m\x1b[1m  ╭─\x1b[0m\x1b[38;5;174m───────
\x1b[48;5;174m\x1b[38;5;16m API Request \x1b[0m\x1b[38;5;174m───────────────────────
\x1b[1m─╮\x1b[0m\x1b[17;3H\x1b[38;5;174m\x1b[1m  │\x1b[0m\x1b[38;5;174m
This tool call will make an API request   \x1b[1m │\x1b[0m\x1b[18;3H\x1b[38;5;174m
\x1b[1m  │\x1b[0m\x1b[38;5;174m  POST https://api.anthropic.com/v1/messages
\x1b[1m │\x1b[0m\x1b[19;3H\x1b[38;5;174m\x1b[1m  ╰─\x1b[0m\x1b[38;5;174m──────────
───────────────────────────────────\x1b[1m─╯\x1b[0m
```

**Clean Output (89 bytes):**
```
API Request
This tool call will make an API request
POST https://api.anthropic.com/v1/messages
```

**Compression: 89% reduction**

#### Key Observations

1. **ANSI sequences dominate file size**: Raw events are 55-89% escape sequences
2. **Semantic content is small**: The actual text is a fraction of the raw data
3. **Box drawing adds noise**: Claude's UI elements (╭╮╰╯│─) add visual structure but no semantic value
4. **Timestamps preserved**: The transformation keeps timestamp associations intact for marker positioning

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

---

## 6. Test-Driven Development (TDD) Approach

### 6.1 Philosophy: RED → GREEN → REFACTOR

All transforms and extraction logic MUST be developed test-first using the classic TDD cycle:

```
    ┌─────────────────────────────────────────────────────────┐
    │                                                         │
    │   ┌─────────┐      ┌─────────┐      ┌──────────────┐   │
    │   │  RED    │ ───▶ │  GREEN  │ ───▶ │   REFACTOR   │   │
    │   │ (test   │      │ (make   │      │ (clean up,   │   │
    │   │  fails) │      │  pass)  │      │  tests stay  │   │
    │   └─────────┘      └─────────┘      │  green)      │   │
    │        ▲                            └──────┬───────┘   │
    │        │                                   │           │
    │        └───────────────────────────────────┘           │
    │                    (next test)                         │
    └─────────────────────────────────────────────────────────┘
```

| Phase | What | Why |
|-------|------|-----|
| 🔴 **RED** | Write test that fails | Proves test actually checks something |
| 🟢 **GREEN** | Write minimal code to pass | No premature optimization, just make it work |
| 🔵 **REFACTOR** | Clean up code, tests stay green | Improve design without changing behavior |

**The cycle repeats for each new behavior.**

**Key principles:**
1. **Never write production code without a failing test first**
2. **Write the simplest code that makes the test pass**
3. **Refactor only when tests are green**
4. **Run tests after every change**
5. **Snapshot tests** capture complex outputs for regression detection

### 6.2 Snapshot Testing Strategy

Snapshot tests capture the "before/after" of transformations for regression detection.

```rust
// In tests/snapshots/transform_tests.rs

use insta::assert_snapshot;

#[test]
fn snapshot_claude_ansi_stripping() {
    let raw = include_str!("fixtures/claude_raw_event.txt");
    let clean = strip_ansi_codes(raw);
    assert_snapshot!("claude_ansi_stripped", clean);
}

#[test]
fn snapshot_gemini_progress_dedupe() {
    let events = load_test_events("fixtures/gemini_progress_sequence.json");
    let mut deduped = events.clone();
    DeduplicateProgressLines.transform(&mut deduped);
    assert_snapshot!("gemini_progress_deduped", format_events(&deduped));
}

#[test]
fn snapshot_full_pipeline_codex() {
    let cast = load_cast_file("fixtures/codex_sample.cast");
    let config = ExtractionConfig::default();
    let content = ContentExtractor::new(config).extract(&cast);
    assert_snapshot!("codex_full_pipeline", content.text);
}
```

### 6.3 Test Fixtures

Create minimal but representative test fixtures from real files:

```
tests/
├── fixtures/
│   ├── claude/
│   │   ├── raw_spinner_event.txt       # Single spinner animation
│   │   ├── raw_box_drawing.txt         # Box-drawn dialog
│   │   ├── sample_session_100kb.cast   # Small representative sample
│   │   └── expected_clean_output.txt   # Expected transformation result
│   ├── codex/
│   │   ├── raw_menu_selection.txt      # Menu with indicators
│   │   ├── raw_progress_output.txt     # Build progress
│   │   └── sample_session_50kb.cast
│   └── gemini/
│       ├── raw_braille_spinner.txt     # Braille animation sequence
│       ├── raw_progress_bar.txt        # Progress bar sequence
│       └── sample_session_100kb.cast
├── snapshots/
│   └── transform_tests/                # insta snapshot files
│       ├── claude_ansi_stripped.snap
│       ├── gemini_progress_deduped.snap
│       └── codex_full_pipeline.snap
```

### 6.4 Test Categories

#### Unit Tests (per transform)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Each transform has dedicated unit tests

    mod strip_ansi {
        #[test]
        fn removes_color_codes() { ... }

        #[test]
        fn removes_cursor_movement() { ... }

        #[test]
        fn preserves_plain_text() { ... }

        #[test]
        fn handles_malformed_sequences() { ... }

        #[test]
        fn handles_utf8_content() { ... }
    }

    mod deduplicate_progress {
        #[test]
        fn collapses_spinner_frames() { ... }

        #[test]
        fn keeps_non_progress_lines() { ... }

        #[test]
        fn handles_empty_input() { ... }
    }
}
```

#### Integration Tests (full pipeline)

```rust
// tests/integration/content_extraction.rs

#[test]
fn extracts_content_from_claude_fixture() {
    // Use fixture extracted from real session, NOT the actual 73MB file
    let cast = load_fixture("claude/sample_session_100kb.cast");
    let extractor = ContentExtractor::default();

    let content = extractor.extract(&cast);

    // Verify significant size reduction
    assert!(content.text.len() < cast.raw_size() / 5);

    // Verify no ANSI codes remain
    assert!(!content.text.contains("\x1b["));

    // Verify timestamps preserved
    assert!(content.segments.iter().all(|s| s.end_time >= s.start_time));
}
```

#### Property-Based Tests

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn strip_ansi_never_adds_bytes(input in ".*") {
        let result = strip_ansi_codes(&input);
        prop_assert!(result.len() <= input.len());
    }

    #[test]
    fn strip_ansi_is_idempotent(input in ".*") {
        let once = strip_ansi_codes(&input);
        let twice = strip_ansi_codes(&once);
        prop_assert_eq!(once, twice);
    }

    #[test]
    fn timestamp_resolution_in_bounds(
        chunk_start in 0.0f64..1000.0,
        chunk_end in 0.0f64..1000.0,
        relative in 0.0f64..100.0,
    ) {
        let chunk_end = chunk_start + chunk_end.abs();
        let chunk = AnalysisChunk {
            time_range: TimeRange { start: chunk_start, end: chunk_end },
            ..Default::default()
        };

        let absolute = chunk.resolve_timestamp(relative);

        prop_assert!(absolute >= chunk_start);
        prop_assert!(absolute <= chunk_end + relative);
    }
}
```

### 6.5 Test-First Implementation Order

For each stage, write tests BEFORE implementation:

#### Stage 1: Content Extraction

```
1. Write snapshot test for StripAnsiCodes → Run (fails) → Implement → Passes
2. Write unit tests for edge cases → Run (fails) → Implement → Passes
3. Write snapshot test for StripControlCharacters → ...
4. Write snapshot test for DeduplicateProgressLines → ...
5. Write integration test for full pipeline → ...
```

#### Stage 2: Chunking

```
1. Write test for single-chunk case → Implement
2. Write test for multi-chunk splitting → Implement
3. Write test for overlap calculation → Implement
4. Write property test for chunk boundaries → Implement
```

### 6.6 Snapshot Review Workflow

Using `cargo insta`:

```bash
# Run tests, snapshots that differ are marked as pending
cargo insta test

# Review pending snapshots
cargo insta review

# Accept or reject changes
# - Accept: New snapshot becomes the expected value
# - Reject: Test fails until code is fixed
```

### 6.7 CI Integration

```yaml
# In .github/workflows/ci.yml

test:
  steps:
    - name: Run tests
      run: cargo test

    - name: Check snapshots
      run: cargo insta test --check
      # Fails CI if snapshots don't match
```

### 6.8 Coverage Goals

| Component | Unit Test Coverage | Integration Coverage |
|-----------|-------------------|---------------------|
| Transforms | 90%+ | Via full pipeline |
| Chunking | 80%+ | Via large file tests |
| JSON parsing | 95%+ | Via mock responses |
| Backends | Mock-based | Manual (real CLIs) |
| Worker scaling | 70%+ | Via parallel tests |

### 6.9 Test Data Management

**DO:**
- Create small, focused fixtures extracted from real data
- Document what each fixture tests
- Keep fixtures in version control (< 100KB each)
- Update snapshots deliberately

**DON'T:**
- **NEVER reference real user cast files in tests** (e.g., `~/.local/share/asciinema/...`)
- Commit full 100MB cast files as fixtures
- Auto-accept snapshot changes without review
- Skip tests for "obvious" code

**Fixture Creation Process:**
1. Analyze real cast file to identify representative patterns
2. Extract minimal events that demonstrate the pattern (10-100 events)
3. Save as `tests/fixtures/<agent>/<pattern_name>.cast` or `.txt`
4. Document in fixture what real file it was derived from (as comment only)
