# ADR: Refactor Analyze Command

## Status

**Accepted** - Implementation complete, merged via PR #112

## Related Documents

- **REQUIREMENTS.md**: Business requirements and acceptance criteria
- **SPEC.md**: Implementation specifications (JSON schema, parallelization logic, noise patterns)
- **PLAN.md**: Implementation stages and tasks

## Prior Art: Existing Infrastructure

This refactor leverages existing patterns already established in the codebase:

### Transform Trait Pattern (`src/asciicast/transform.rs`)

The codebase already has a well-designed transformation pipeline:

```rust
pub trait Transform {
    fn transform(&mut self, events: &mut Vec<Event>);
}

pub struct TransformChain {
    transforms: Vec<Box<dyn Transform>>,
}
```

**Key design principles (from existing code):**
- **In-place mutation**: Transforms modify `Vec<Event>` directly to avoid memory copies when processing millions of events
- **Stateful transforms**: The `&mut self` receiver allows transforms to track state across events (e.g., cumulative time offsets)
- **Composable**: Multiple transforms can be chained together
- **Infallible**: Returns `()`, handles errors internally

This pattern is the foundation for our content extraction pipeline.

### Research Document (`research/algorithm_for_asciicast_cutting_and_compression.md`)

Comprehensive prior research exists covering:
- ANSI stripping algorithms
- Silence removal and compression
- Spinner/progress bar detection
- Streaming pipeline architecture (Source-Transform-Sink)
- Memory efficiency O(1) relative to file size
- Unicode/grapheme cluster handling

The implementation should reference this document for edge cases and algorithmic details

## Context

The `agr analyze` command needs a complete architectural overhaul. The current implementation is a rudimentary prototype with two critical issues:

### Problem 1: File Size

Cast files regularly grow to 60-75MB and can exceed 100MB due to ANSI escape sequences (terminal control codes for cursor positioning, colors, styling). These codes:
- Are unnecessary for LLM analysis (LLMs need semantic content, not rendering instructions)
- Cause analysis to take extremely long or timeout
- May exceed LLM context windows

Example from a real 75MB cast file:
```
[0.164,"o","\u001b[?2026h...\u001b[38;5;174m...\u001b[48;5;174m\u001b[38;5;16m..."]
```

### Problem 2: Architecture & Permissions

The current implementation:
- Spawns agent CLIs (claude, codex, gemini) without required permission flags
- Expects agents to execute `agr marker add` shell commands (requires permissions)
- Has no abstraction layers for different agent types
- Cannot scale or evolve

### Core Requirements (from REQUIREMENTS.md)

| Requirement | Description |
|-------------|-------------|
| R1 | In-memory ANSI stripping (preserve timestamps, don't modify files) |
| R2 | Proper abstraction layers (agent interface trait, extensible) |
| R3 | Structured analysis results (JSON output, agr owns marker writing) |
| R4 | Playback compatibility (asciicast v3, markers work in player) |
| R5 | **Parallel analysis (CORE GOAL)** - split, analyze, merge |
| R6 | Token tracking for smart decisions (subscription-based, not billing) |
| R7 | Pattern-driven architecture (documented design patterns) |
| R8 | Error handling & smart retry (informed by token tracking) |
| R9 | Existing marker handling (warn but don't block re-analysis) |

### Research: LLM Context Window Limits

| Agent | Context Window | Safe Limit for Chunks |
|-------|---------------|----------------------|
| Claude (Sonnet/Opus) | 200K tokens (standard) | ~160K tokens |
| Codex | 192K-400K tokens | ~150K tokens |
| Gemini | 1M-2M tokens | ~800K tokens |

### Token Estimation Strategy

#### Options Considered

##### Option A: tiktoken-rs (Accurate, External Dependency)

```rust
// Using tiktoken-rs crate
use tiktoken_rs::cl100k_base;

pub fn count_tokens_accurate(text: &str) -> usize {
    let bpe = cl100k_base().unwrap();
    bpe.encode_with_special_tokens(text).len()
}
```

- **Pros**: Exact token count matching OpenAI tokenizers, battle-tested
- **Cons**:
  - Additional dependency (~5MB)
  - Only accurate for OpenAI models (Claude/Gemini may differ)
  - Slower than heuristic

See: [tiktoken-rs on crates.io](https://crates.io/crates/tiktoken-rs)

##### Option B: rs-bpe (Fastest, External Dependency)

```rust
// Using rs-bpe crate
use bpe::Tokenizer;

pub fn count_tokens_fast(text: &str) -> usize {
    let tokenizer = Tokenizer::cl100k();
    tokenizer.count(text)
}
```

- **Pros**: 15x faster than tiktoken, constant-time counting for substrings
- **Cons**: Newer library, additional dependency

See: [rs-bpe performance analysis](https://dev.to/gweidart/rs-bpe-outperforms-tiktoken-tokenizers-2h3j)

##### Option C: Character Heuristic (Simple, No Dependency) [SELECTED]

```rust
/// Estimate token count from text content.
/// Uses chars/4 heuristic - simple, fast, no dependencies.
pub struct TokenEstimator {
    /// Base ratio: characters per token (default: 4.0)
    chars_per_token: f64,
    /// Safety margin to avoid exceeding limits (default: 0.85 = 15% buffer)
    safety_factor: f64,
}

impl TokenEstimator {
    pub fn estimate(&self, text: &str) -> usize {
        let char_count = text.chars().count();
        let raw_estimate = (char_count as f64 / self.chars_per_token).ceil() as usize;
        (raw_estimate as f64 * self.safety_factor) as usize
    }

    /// Estimate with whitespace bonus (code has more tokens per char)
    pub fn estimate_code(&self, text: &str) -> usize {
        let char_count = text.chars().count();
        let whitespace_count = text.chars().filter(|c| c.is_whitespace()).count();
        let whitespace_ratio = whitespace_count as f64 / char_count.max(1) as f64;

        // Code typically has 3.0-3.5 chars per token due to short identifiers
        let adjusted_ratio = if whitespace_ratio > 0.15 {
            3.5 // Code-like content
        } else {
            self.chars_per_token // Prose-like content
        };

        let raw_estimate = (char_count as f64 / adjusted_ratio).ceil() as usize;
        (raw_estimate as f64 * self.safety_factor) as usize
    }
}

impl Default for TokenEstimator {
    fn default() -> Self {
        Self {
            chars_per_token: 4.0,
            safety_factor: 0.85, // 15% safety buffer (conservative)
        }
    }
}
```

**Decision**: Option C (character heuristic) selected because:
1. **No external dependency** - keeps the tool lightweight
2. **Agent-agnostic** - tiktoken is OpenAI-specific, but we support Claude/Codex/Gemini
3. **15% safety margin** - compensates for estimation errors
4. **Good enough** - we're chunking, not billing. Slight overestimate is fine.
5. **Fast** - O(n) character count vs tokenizer regex parsing

**When estimation happens (AFTER cleanup):**

```
Raw Cast File (100MB)
        │
        ▼
┌───────────────────┐
│ Transform Pipeline │  ← ANSI stripped, spinners removed, progress deduped
└───────────────────┘
        │
        ▼
  Cleaned Events (~15MB)
        │
        ▼
┌───────────────────┐
│  Segment Creation  │  ← Token estimation happens HERE on clean content
└───────────────────┘
        │
        ▼
  AnalysisContent with accurate token counts
        │
        ▼
┌───────────────────┐
│  Chunk Calculator  │  ← Uses token counts to split into chunks
└───────────────────┘
```

**Critical**: Token estimation MUST happen after cleanup. Raw content is 55-89% noise (ANSI codes, spinners, progress bars). Estimating on raw content would massively overcount tokens.

1. Transform pipeline runs → cleans events in-place
2. Segment creation → estimates tokens on `segment.content` (already clean)
3. Chunk calculation → sums segment tokens to determine boundaries

**Future consideration**: If estimation proves too inaccurate, upgrade to tiktoken-rs or rs-bpe.

### Research: Agent CLI Capabilities

| Feature | Claude | Codex | Gemini |
|---------|--------|-------|--------|
| Non-interactive mode | `--print` | `exec --full-auto` | positional prompt |
| JSON output | `--output-format json` | N/A (text only) | `--output-format json` |
| Structured schema | `--json-schema <schema>` | N/A | N/A |
| Stdin input | Yes (pipe) | Yes (pipe) | Yes (pipe) |

**Permission Philosophy:** Agents receive content IN the prompt and return JSON. They never need to read files, write files, or execute commands. Therefore, **no permission bypass flags are required** (`--dangerously-skip-permissions`, `--dangerously-bypass-approvals-and-sandbox`, `--yolo`).

> **See SPEC.md Section 4** for complete CLI invocation examples.

### Prompt Design

The analysis prompt is designed to work without file access:
- Cast content is embedded directly in the prompt
- Agent returns JSON with marker positions
- No file reads, writes, or shell commands needed

Key prompt elements:
1. **Context**: Explains the content is cleaned terminal output
2. **Semantic indicators**: Notes that ✓✔✕⚠ are preserved for outcome detection
3. **Timestamp guidance**: Explains relative timestamps within chunk
4. **Output format**: JSON-only, no markdown wrapping
5. **Category definitions**: Clear guidance on when to use each

> **See SPEC.md Section 2.1** for the complete prompt template.

The prompt is stored as `src/analyzer/prompts/analyze.txt` for easy iteration without recompiling.

### AgentBackend Trait Definition

```rust
use std::time::Duration;

/// Result type for agent backend operations
pub type BackendResult<T> = Result<T, BackendError>;

/// Trait for AI agent backends (Strategy pattern)
pub trait AgentBackend: Send + Sync {
    /// Human-readable name for logging
    fn name(&self) -> &'static str;

    /// Check if the agent CLI is available on the system
    fn is_available(&self) -> bool;

    /// Invoke the agent with a prompt and return raw response
    fn invoke(&self, prompt: &str, timeout: Duration) -> BackendResult<String>;

    /// Parse raw response into markers (handles JSON extraction)
    fn parse_response(&self, response: &str) -> BackendResult<Vec<RawMarker>>;

    /// Get the token budget for this agent
    fn token_budget(&self) -> TokenBudget;
}

/// Agent types supported
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentType {
    Claude,
    Codex,
    Gemini,
}

impl AgentType {
    /// Create the appropriate backend for this agent type
    pub fn create_backend(&self) -> Box<dyn AgentBackend> {
        match self {
            AgentType::Claude => Box::new(ClaudeBackend::new()),
            AgentType::Codex => Box::new(CodexBackend::new()),
            AgentType::Gemini => Box::new(GeminiBackend::new()),
        }
    }
}

/// Errors from agent backends
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("Agent CLI not found: {0}")]
    NotAvailable(String),

    #[error("Agent timed out after {0:?}")]
    Timeout(Duration),

    #[error("Agent returned non-zero exit code: {code}")]
    ExitCode { code: i32, stderr: String },

    #[error("Failed to parse response as JSON: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("Failed to extract JSON from response")]
    JsonExtraction { response: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

---

## Options Considered

### Content Extraction: Transform Pipeline Approach

Building on the existing `Transform` trait, content extraction is implemented as a pipeline of composable transforms that strip noise while preserving timestamp mappings.

#### Transform Pipeline Design

```rust
use crate::asciicast::{Transform, TransformChain, Event};

/// Content extraction pipeline using existing Transform infrastructure
pub fn build_extraction_pipeline() -> TransformChain {
    TransformChain::new()
        .with(StripAnsiCodes::new())           // Remove escape sequences
        .with(StripControlCharacters::new())   // Remove BEL, BS, NUL, etc.
        .with(StripBoxDrawing::new())          // Remove ─│┌┐└┘ etc.
        .with(StripSpinnerChars::new())        // Remove ⠋⠙⠹ etc.
        .with(StripProgressBlocks::new())      // Remove █░ etc.
        .with(DeduplicateProgressLines::new()) // Keep only final state of \r lines
        .with(NormalizeWhitespace::new())      // Collapse excessive newlines
        .with(FilterEmptyEvents)               // Remove events with no content
}

/// Each transform implements the existing Transform trait.
/// Transforms MUST preserve non-output events (markers, input).

struct StripAnsiCodes {
    // Regex compiled once, reused for all events
    ansi_regex: regex::Regex,
}

impl StripAnsiCodes {
    pub fn new() -> Self {
        // Matches all ANSI escape sequences:
        // - CSI sequences: \x1b[ ... (params) ... (final byte)
        // - OSC sequences: \x1b] ... \x07 or \x1b\\
        // - Simple escapes: \x1b followed by single char
        let pattern = concat!(
            r"\x1b\[[0-9;?]*[A-Za-z]",      // CSI sequences (most common)
            r"|\x1b\][^\x07]*(?:\x07|\x1b\\)", // OSC sequences (hyperlinks, titles)
            r"|\x1b[PX^_][^\x1b]*\x1b\\",   // DCS, SOS, PM, APC sequences
            r"|\x1b[@-Z\\-_]",              // Simple escape sequences
            r"|\x1b\([0-9A-Za-z]",          // Character set selection
        );
        Self {
            ansi_regex: regex::Regex::new(pattern).unwrap(),
        }
    }
}

impl Transform for StripAnsiCodes {
    fn transform(&mut self, events: &mut Vec<Event>) {
        for event in events.iter_mut() {
            // Only modify output events, preserve markers/input
            if event.is_output() {
                // Note: Event.data is a public field - direct access is used
                // Implementation should add data()/data_mut() accessors for consistency
                event.data = self.ansi_regex.replace_all(&event.data, "").into_owned();
            }
        }
    }
}
```

**Key insight**: The Transform trait works on `Vec<Event>`, preserving timestamps naturally. We don't need a separate mapping system - the events retain their original timestamps throughout the pipeline.

#### What is "Useless" for LLMs?

It's not just ANSI codes. Terminal output contains many categories of noise.

> **See SPEC.md Section 1** for comprehensive noise patterns discovered from real cast file analysis (Claude 73MB, Codex 4.3MB, Gemini 176MB sessions).

| Category | Examples | Why Useless |
|----------|----------|-------------|
| **ANSI Escape Sequences** | `\x1b[38;5;174m`, `\x1b[H`, `\x1b[2J` | Rendering instructions, no semantic value |
| **Control Characters** | `\x07` (BEL), `\x08` (BS), `\x00`-`\x06` | Audio/visual signals, no content |
| **Progress Bar Spam** | Same line rewritten 1000x via `\r` | Only final state matters |
| **Spinner Characters** | `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` cycling | Visual animation, no content |
| **Box Drawing** | `┌─┐│└─┘├┤┬┴┼` | Decorative framing |
| **Excessive Whitespace** | `\n\n\n\n\n\n` | Adds nothing |
| **Binary Garbage** | Non-UTF8, image data, base64 blobs | Corrupted or embedded data |

> **See SPEC.md Section 1.7** for before/after examples showing 55-89% compression ratios.

#### The Timestamp Preservation Problem

When we strip content, we must maintain the connection to timestamps for accurate marker placement.

**Example Problem:**
```
Original events:
  [10.0, "o", "Starting build..."]
  [10.5, "o", "\r⠋ Building"]
  [10.6, "o", "\r⠙ Building"]
  ... 500 more spinner updates ...
  [15.2, "o", "\r✓ Build complete\n"]
  [15.3, "o", "Running tests..."]

After naive string concatenation:
  "Starting build...✓ Build completeRunning tests..."

Problem: If LLM says "marker at character 30", which timestamp is that?
```

**Solution: Transform Pipeline Preserves Events**

Using the Transform trait, we modify events in-place rather than concatenating into a string. Each event retains its original timestamp:

```rust
// After transform pipeline, events still have timestamps:
  [10.0, "o", "Starting build..."]
  [15.2, "o", "✓ Build complete\n"]    // Spinner events removed
  [15.3, "o", "Running tests..."]
```

For chunking, we create `AnalysisSegment` structs that group transformed events by time range while preserving the mapping back to original timestamps.

#### Core Data Structures

```rust
/// A segment of analysis content with time range mapping.
/// Created from transformed events for chunking and LLM analysis.
#[derive(Debug, Clone)]
pub struct AnalysisSegment {
    /// Start timestamp (absolute, from recording start)
    pub start_time: f64,

    /// End timestamp (absolute)
    pub end_time: f64,

    /// Cleaned text content for this segment
    pub content: String,

    /// Estimated token count for this segment
    pub estimated_tokens: usize,

    /// Range of event indices in original cast file (for reverse mapping)
    pub event_range: (usize, usize),
}

/// Complete analysis content extracted from a cast file.
#[derive(Debug)]
pub struct AnalysisContent {
    /// Segments with time ranges and content
    pub segments: Vec<AnalysisSegment>,

    /// Total recording duration
    pub total_duration: f64,

    /// Total estimated tokens across all segments
    pub total_tokens: usize,

    /// Extraction statistics for transparency
    pub stats: ExtractionStats,
}

impl AnalysisContent {
    /// Find the segment containing a given timestamp.
    pub fn segment_at_time(&self, timestamp: f64) -> Option<&AnalysisSegment> {
        self.segments.iter().find(|s|
            s.start_time <= timestamp && timestamp < s.end_time
        )
    }

    /// Get segments within a time range (for chunking).
    pub fn segments_in_range(&self, start: f64, end: f64) -> Vec<&AnalysisSegment> {
        self.segments.iter()
            .filter(|s| s.end_time > start && s.start_time < end)
            .collect()
    }
}

/// Extraction statistics for transparency
#[derive(Debug, Default, Clone)]
pub struct ExtractionStats {
    pub original_bytes: usize,
    pub extracted_bytes: usize,
    pub ansi_sequences_stripped: usize,
    pub control_chars_stripped: usize,
    pub progress_lines_deduplicated: usize,
    pub events_processed: usize,
    pub events_retained: usize,
}

/// Stats are collected via a shared accumulator passed to transforms
pub struct StatsCollector {
    stats: std::cell::RefCell<ExtractionStats>,
}

impl StatsCollector {
    pub fn new() -> Self {
        Self { stats: RefCell::new(ExtractionStats::default()) }
    }

    pub fn record_ansi_stripped(&self, count: usize) {
        self.stats.borrow_mut().ansi_sequences_stripped += count;
    }

    pub fn record_control_stripped(&self, count: usize) {
        self.stats.borrow_mut().control_chars_stripped += count;
    }

    pub fn record_progress_deduped(&self, count: usize) {
        self.stats.borrow_mut().progress_lines_deduplicated += count;
    }

    pub fn finalize(self, original_bytes: usize, extracted_bytes: usize) -> ExtractionStats {
        let mut stats = self.stats.into_inner();
        stats.original_bytes = original_bytes;
        stats.extracted_bytes = extracted_bytes;
        stats
    }
}
```

#### LLM Response Mapping

When the LLM returns markers, they include timestamps relative to the chunk. The `AnalysisChunk` handles mapping back to absolute timestamps:

```rust
/// Marker position types the LLM can return
/// See SPEC.md Section 2 for JSON response schema and Rust types
pub enum MarkerPosition {
    /// Relative timestamp within chunk (seconds from chunk start)
    RelativeTimestamp(f64),
    /// Search for text and use its timestamp
    TextSearch(String),
}

impl AnalysisChunk {
    /// Map LLM marker to absolute timestamp in original recording.
    pub fn resolve_marker_timestamp(&self, position: &MarkerPosition) -> Option<f64> {
        match position {
            MarkerPosition::RelativeTimestamp(rel_ts) => {
                Some(self.time_range.start + rel_ts)
            }
            MarkerPosition::TextSearch(needle) => {
                // Find the segment containing the text
                self.segments.iter()
                    .find(|s| s.content.contains(needle))
                    .map(|s| s.start_time)
            }
        }
    }
}
```

#### Chunking with Segment Preservation

**Event Boundary Alignment (NDJSON)**

Asciicast files are NDJSON - each line is one complete event:
```json
[0.1, "o", "hello"]
[0.5, "o", " world\n"]
[1.2, "m", "marker label"]
```

Chunking **always respects event boundaries**:
- Segments group whole events (never split mid-event)
- Chunks contain whole segments (never split mid-segment)
- This ensures timestamp integrity and valid JSON structure

```
Events:  [E1] [E2] [E3] [E4] [E5] [E6] [E7] [E8] [E9]
           └────┬────┘  └────┬────┘  └─────┬─────┘
Segments:     Seg 1        Seg 2         Seg 3
              └─────┬──────┘              │
Chunks:          Chunk 1              Chunk 2
```

```rust
/// A chunk ready for parallel analysis.
#[derive(Debug)]
pub struct AnalysisChunk {
    /// Unique chunk identifier
    pub id: usize,

    /// Time range this chunk covers
    pub time_range: TimeRange,

    /// Segments within this chunk (preserves timestamp mapping)
    pub segments: Vec<AnalysisSegment>,

    /// Combined text for LLM (concatenated segment content)
    pub text: String,

    /// Estimated token count
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone)]
pub struct TimeRange {
    pub start: f64,
    pub end: f64,
}

impl AnalysisChunk {
    /// Create chunk from content segments within a time range.
    pub fn from_content(
        id: usize,
        content: &AnalysisContent,
        time_range: TimeRange,
    ) -> Self {
        let segments: Vec<_> = content.segments_in_range(time_range.start, time_range.end)
            .into_iter()
            .cloned()
            .collect();

        let text = segments.iter()
            .map(|s| s.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let estimated_tokens = segments.iter()
            .map(|s| s.estimated_tokens)
            .sum();

        Self { id, time_range, segments, text, estimated_tokens }
    }
}
```

#### Content Transformations (Ordered Pipeline)

Each transform implements the existing `Transform` trait from `src/asciicast/transform.rs`:

| Order | Transform | Purpose | Implementation |
|-------|-----------|---------|----------------|
| 1 | `StripAnsiCodes` | Remove escape sequences | Regex or state machine |
| 2 | `StripControlCharacters` | Remove BEL, BS, NUL, etc. | Filter non-printable |
| 3 | `DeduplicateProgressLines` | Keep only final state of `\r`-rewritten lines | See algorithm below |
| 4 | `NormalizeWhitespace` | Collapse excessive newlines/spaces | Limit consecutive chars |
| 5 | `FilterEmptyEvents` | Remove events with no remaining content | `events.retain()` |
| 6 | `StripBoxDrawing` | Remove decorative box characters | Unicode ranges (see below) |
| 7 | `StripSpinnerChars` | Remove spinner/progress indicators | From SPEC.md Section 1 |

#### Performance Considerations

The naive approach of separate transforms with multiple passes is inefficient for 100MB+ files:
- 7 passes × 500K events = 3.5M iterations
- String allocations per event per transform
- Regex overhead for ANSI stripping

**Optimized Approach: Single-Pass Byte Processing**

```rust
/// Combined single-pass content cleaner for performance.
/// Processes bytes directly, avoids multiple allocations.
pub struct ContentCleaner {
    /// Output buffer, reused across events
    buffer: Vec<u8>,
    /// State machine for ANSI sequence detection
    ansi_state: AnsiParseState,
    /// Lookup table for characters to strip (256 bytes for ASCII, HashMap for Unicode)
    strip_ascii: [bool; 128],
    strip_unicode: HashSet<char>,
    /// Characters with semantic meaning (never strip)
    semantic_chars: HashSet<char>,
}

#[derive(Default)]
enum AnsiParseState {
    #[default]
    Normal,
    Escape,          // Saw \x1b
    Csi,             // Saw \x1b[
    CsiParams,       // In CSI parameters
    Osc,             // In OSC sequence
}

impl ContentCleaner {
    pub fn new(config: &ExtractionConfig) -> Self {
        let mut strip_ascii = [false; 128];
        let mut strip_unicode = HashSet::new();
        let mut semantic_chars = HashSet::new();

        // Control characters (ASCII)
        for c in 0x00..=0x08 { strip_ascii[c] = true; }
        for c in 0x0B..=0x0C { strip_ascii[c] = true; }
        for c in 0x0E..=0x1F { strip_ascii[c] = true; }
        strip_ascii[0x7F] = true;

        // Box drawing, spinners, etc (Unicode)
        if config.strip_box_drawing {
            for c in '\u{2500}'..='\u{257F}' { strip_unicode.insert(c); }
            for c in '\u{2580}'..='\u{259F}' { strip_unicode.insert(c); }
        }
        if config.strip_spinner_chars {
            // Spinners only - NOT semantic chars
            for c in ['✻', '✳', '✢', '✶', '✽'] { strip_unicode.insert(c); }
            for c in ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'] {
                strip_unicode.insert(c);
            }
        }

        // Semantic chars - NEVER strip
        for c in ['✓', '✔', '✕', '⚠', 'ℹ', '☐', '☑'] {
            semantic_chars.insert(c);
        }

        Self {
            buffer: Vec::with_capacity(4096),
            ansi_state: AnsiParseState::Normal,
            strip_ascii,
            strip_unicode,
            semantic_chars,
        }
    }

    /// Process event data in single pass, returns cleaned bytes
    pub fn clean(&mut self, data: &str) -> String {
        self.buffer.clear();

        for byte in data.bytes() {
            match (&self.ansi_state, byte) {
                // ANSI escape start
                (AnsiParseState::Normal, 0x1b) => {
                    self.ansi_state = AnsiParseState::Escape;
                }
                // CSI sequence start
                (AnsiParseState::Escape, b'[') => {
                    self.ansi_state = AnsiParseState::Csi;
                }
                // OSC sequence start
                (AnsiParseState::Escape, b']') => {
                    self.ansi_state = AnsiParseState::Osc;
                }
                // CSI final byte (ends sequence)
                (AnsiParseState::Csi | AnsiParseState::CsiParams, b'A'..=b'Z' | b'a'..=b'z') => {
                    self.ansi_state = AnsiParseState::Normal;
                }
                // CSI parameters
                (AnsiParseState::Csi, b'0'..=b'9' | b';' | b'?') => {
                    self.ansi_state = AnsiParseState::CsiParams;
                }
                (AnsiParseState::CsiParams, b'0'..=b'9' | b';' | b'?') => {}
                // OSC terminator
                (AnsiParseState::Osc, 0x07) => {
                    self.ansi_state = AnsiParseState::Normal;
                }
                // Normal character processing
                (AnsiParseState::Normal, _) => {
                    // Fast path: ASCII
                    if byte < 128 {
                        if !self.strip_ascii[byte as usize] {
                            self.buffer.push(byte);
                        }
                    } else {
                        // Slow path: Unicode (need to decode)
                        // This is simplified - real impl needs proper UTF-8 handling
                        self.buffer.push(byte);
                    }
                }
                // Inside escape sequence - skip
                _ => {}
            }
        }

        // Handle incomplete sequences
        if !matches!(self.ansi_state, AnsiParseState::Normal) {
            self.ansi_state = AnsiParseState::Normal;
        }

        String::from_utf8_lossy(&self.buffer).into_owned()
    }
}

impl Transform for ContentCleaner {
    fn transform(&mut self, events: &mut Vec<Event>) {
        for event in events.iter_mut() {
            if event.is_output() {
                if let Some(data) = event.data_mut() {
                    *data = self.clean(data);
                }
            }
        }
    }
}
```

**Performance comparison:**

| Approach | Passes | Allocations | Est. Time (100MB) |
|----------|--------|-------------|-------------------|
| Naive (7 transforms) | 7 | ~3.5M | ~15-30s |
| Single-pass state machine | 1 | ~500K | ~2-5s |

**Key optimizations:**
1. **Single pass** - Process all stripping rules in one iteration
2. **Byte-level processing** - Avoid UTF-8 decode for ASCII (most ANSI codes)
3. **State machine for ANSI** - No regex overhead, O(1) per byte
4. **Lookup tables** - O(1) character classification
5. **Buffer reuse** - Single allocation per event, reused across events

**Trade-off:** More complex code, but 5-10x faster for large files.

---

#### Individual Transform Algorithms (Reference)

The following are conceptual algorithms. The actual implementation should use the optimized `ContentCleaner` above.

**StripControlCharacters**: Remove non-printable control characters.
```rust
impl Transform for StripControlCharacters {
    fn transform(&mut self, events: &mut Vec<Event>) {
        for event in events.iter_mut() {
            if event.is_output() {
                if let Some(data) = event.data_mut() {
                    // Remove C0 controls except \t, \n, \r (needed for structure)
                    // Remove C1 controls (0x80-0x9F)
                    data.retain(|c| {
                        !matches!(c, '\x00'..='\x08' | '\x0B'..='\x0C' | '\x0E'..='\x1F' | '\x7F')
                            && !(c >= '\u{0080}' && c <= '\u{009F}')
                    });
                }
            }
        }
    }
}
```

**StripBoxDrawing**: Remove Unicode box drawing characters (SPEC.md Section 1.5).
```rust
impl Transform for StripBoxDrawing {
    fn transform(&mut self, events: &mut Vec<Event>) {
        for event in events.iter_mut() {
            if event.is_output() {
                if let Some(data) = event.data_mut() {
                    data.retain(|c| !matches!(c,
                        // Box Drawing block (U+2500-U+257F)
                        '\u{2500}'..='\u{257F}' |
                        // Block Elements (U+2580-U+259F) - includes ▖▗▘▝
                        '\u{2580}'..='\u{259F}'
                    ));
                }
            }
        }
    }
}
```

**StripSpinnerChars**: Remove spinner animation characters ONLY. Keep semantic indicators!

```rust
impl Transform for StripSpinnerChars {
    fn transform(&mut self, events: &mut Vec<Event>) {
        for event in events.iter_mut() {
            if event.is_output() {
                if let Some(data) = event.data_mut() {
                    data.retain(|c| !matches!(c,
                        // Claude spinners ONLY (animation frames)
                        '✻' | '✳' | '✢' | '✶' | '✽' |
                        // Gemini braille spinner ONLY (animation frames)
                        '⠋' | '⠙' | '⠹' | '⠸' | '⠼' | '⠴' | '⠦' | '⠧' | '⠇' | '⠏' |
                        // Visual-only bullets (no semantic meaning)
                        '•' | '›' | '◦' | '⋮'
                    ));
                    // NOTE: DO NOT strip semantic indicators!
                    // KEEP: ✓ ✔ (success), ✕ (failure), ⚠ (warning), ℹ (info), ☐ ☑ (checkbox)
                    // These help the LLM understand success/failure moments.
                }
            }
        }
    }
}
```

**CRITICAL: Semantic vs Visual Characters**

| STRIP (visual only) | KEEP (semantic meaning) |
|---------------------|------------------------|
| `✻✳✢✶✽` (spinner frames) | `✓` `✔` (success/pass) |
| `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` (braille spinner) | `✕` (failure/error) |
| `•›◦⋮` (bullets) | `⚠` (warning) |
| | `ℹ` (information) |
| | `☐` `☑` (checkbox state) |

The LLM needs semantic indicators to identify success/failure moments (R3, AC3).
```

**StripProgressBlocks**: Remove progress bar block characters.
```rust
impl Transform for StripProgressBlocks {
    fn transform(&mut self, events: &mut Vec<Event>) {
        for event in events.iter_mut() {
            if event.is_output() {
                if let Some(data) = event.data_mut() {
                    data.retain(|c| !matches!(c,
                        '█' | '░' | '▒' | '▓' |  // Block elements
                        '▼' | '▲' | '●' | '○'    // Geometric shapes used in progress
                    ));
                }
            }
        }
    }
}
```

**NormalizeWhitespace**: Collapse excessive whitespace.
```rust
struct NormalizeWhitespace {
    max_consecutive_newlines: usize,
}

impl Transform for NormalizeWhitespace {
    fn transform(&mut self, events: &mut Vec<Event>) {
        for event in events.iter_mut() {
            if event.is_output() {
                if let Some(data) = event.data_mut() {
                    // Collapse multiple spaces to single space
                    let mut result = String::with_capacity(data.len());
                    let mut prev_space = false;
                    let mut newline_count = 0;

                    for c in data.chars() {
                        if c == '\n' {
                            newline_count += 1;
                            if newline_count <= self.max_consecutive_newlines {
                                result.push(c);
                            }
                            prev_space = false;
                        } else if c.is_whitespace() {
                            newline_count = 0;
                            if !prev_space {
                                result.push(' ');
                                prev_space = true;
                            }
                        } else {
                            newline_count = 0;
                            prev_space = false;
                            result.push(c);
                        }
                    }
                    *data = result;
                }
            }
        }
    }
}
```

**FilterEmptyEvents**: Remove events with no content.
```rust
struct FilterEmptyEvents;

impl Transform for FilterEmptyEvents {
    fn transform(&mut self, events: &mut Vec<Event>) {
        events.retain(|event| {
            // Always keep non-output events (markers, input)
            if !event.is_output() {
                return true;
            }
            // Keep output events only if they have content
            event.data().map(|d| !d.trim().is_empty()).unwrap_or(false)
        });
    }
}

#### Progress Line Deduplication Algorithm

Terminal progress bars use `\r` (carriage return) to overwrite the same line repeatedly. This creates thousands of events that all represent the same conceptual line.

```rust
/// Deduplicates progress lines that use \r to overwrite themselves.
///
/// Algorithm:
/// 1. Track "current line buffer" with timestamp of FIRST char
/// 2. When \r is encountered, clear buffer but DON'T update timestamp yet
/// 3. When \n is encountered, emit with timestamp of line START
/// 4. Non-output events (markers, input) pass through unchanged
struct DeduplicateProgressLines {
    current_line: String,
    line_start_time: f64,      // Timestamp when current line started
    is_progress_line: bool,
    deduped_count: usize,
}

impl DeduplicateProgressLines {
    pub fn new() -> Self {
        Self {
            current_line: String::new(),
            line_start_time: 0.0,
            is_progress_line: false,
            deduped_count: 0,
        }
    }
}

impl Transform for DeduplicateProgressLines {
    fn transform(&mut self, events: &mut Vec<Event>) {
        let mut output_events = Vec::with_capacity(events.len());

        for event in events.drain(..) {
            // Preserve non-output events (markers, input)
            if !event.is_output() {
                output_events.push(event);
                continue;
            }

            if let Some(data) = event.data() {
                for ch in data.chars() {
                    match ch {
                        '\r' => {
                            // Carriage return: line will be overwritten
                            // Keep line_start_time - we want timestamp of FINAL content
                            self.is_progress_line = true;
                            self.current_line.clear();
                            // Update start time to current event (progress shows final state time)
                            self.line_start_time = event.time;
                        }
                        '\n' => {
                            // Newline: emit current line with appropriate timestamp
                            if !self.current_line.is_empty() {
                                output_events.push(Event::output(
                                    self.line_start_time,
                                    format!("{}\n", self.current_line),
                                ));
                            }
                            if self.is_progress_line {
                                self.deduped_count += 1;
                            }
                            self.current_line.clear();
                            self.is_progress_line = false;
                        }
                        _ => {
                            // First char of new line sets the timestamp
                            if self.current_line.is_empty() {
                                self.line_start_time = event.time;
                            }
                            self.current_line.push(ch);
                        }
                    }
                }
            }
        }

        // Don't forget trailing content without \n
        if !self.current_line.is_empty() {
            output_events.push(Event::output(
                self.line_start_time,
                self.current_line.clone(),
            ));
        }

        *events = output_events;
    }
}
```

**Example transformation:**
```
Input events:
  [10.5, "o", "\r⠋ Building..."]
  [10.6, "o", "\r⠙ Building..."]
  [10.7, "o", "\r⠹ Building..."]
  [15.2, "o", "\r✓ Build complete\n"]

Output events:
  [15.2, "o", "✓ Build complete\n"]  // Only final state
```

```rust
/// Configuration for the extraction pipeline
pub struct ExtractionConfig {
    pub strip_ansi: bool,                    // Always true
    pub strip_control_chars: bool,           // Always true
    pub dedupe_progress_lines: bool,         // True (critical for size reduction)
    pub normalize_whitespace: bool,          // True
    pub max_consecutive_newlines: usize,     // 2
    pub strip_box_drawing: bool,             // True (decorative framing, no semantic value)
    pub strip_spinner_chars: bool,           // True (visual animation, no content)
    pub strip_progress_blocks: bool,         // True (█░ etc, visual only)
    pub preserve_hyperlink_urls: bool,       // True (URLs can be useful)
}

impl ExtractionConfig {
    /// Build transform chain based on configuration.
    ///
    /// ## Pipeline Order Rationale
    ///
    /// The order is critical for correctness:
    ///
    /// 1. **StripAnsiCodes FIRST** - ANSI codes can contain characters that look
    ///    like spinners or box drawing. Strip escape sequences before content filtering.
    ///
    /// 2. **StripControlCharacters** - Remove non-printable chars that might interfere
    ///    with text processing.
    ///
    /// 3. **StripBoxDrawing, StripSpinnerChars, StripProgressBlocks** - These operate
    ///    on clean text. Order among these doesn't matter (independent character sets).
    ///
    /// 4. **DeduplicateProgressLines AFTER character stripping** - Progress lines may
    ///    contain spinners. Strip spinners first so deduplication sees clean content.
    ///    This ensures "⠋ Building..." and "⠙ Building..." collapse to just "Building...".
    ///
    /// 5. **NormalizeWhitespace** - Clean up any excess whitespace from previous steps.
    ///
    /// 6. **FilterEmptyEvents LAST** - Only filter after all content processing.
    ///    Earlier transforms may empty out events that should be removed.
    ///
    pub fn build_pipeline(&self) -> TransformChain {
        let mut chain = TransformChain::new();

        // Order matters - see rationale above
        if self.strip_ansi {
            chain = chain.with(StripAnsiCodes::new());
        }
        if self.strip_control_chars {
            chain = chain.with(StripControlCharacters::new());
        }
        if self.strip_box_drawing {
            chain = chain.with(StripBoxDrawing::new());
        }
        if self.strip_spinner_chars {
            chain = chain.with(StripSpinnerChars::new());
        }
        if self.strip_progress_blocks {
            chain = chain.with(StripProgressBlocks::new());
        }
        if self.dedupe_progress_lines {
            chain = chain.with(DeduplicateProgressLines::new());
        }
        if self.normalize_whitespace {
            chain = chain.with(NormalizeWhitespace::new(self.max_consecutive_newlines));
        }

        // Always filter empty events at the end
        chain.with(FilterEmptyEvents)
    }
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            strip_ansi: true,
            strip_control_chars: true,
            dedupe_progress_lines: true,
            normalize_whitespace: true,
            max_consecutive_newlines: 2,
            strip_box_drawing: true,
            strip_spinner_chars: true,
            strip_progress_blocks: true,
            preserve_hyperlink_urls: true,
        }
    }
}
```

#### Memory Efficiency for 100MB+ Files

For a 100MB cast file:
- **Original**: 100MB of events with ANSI codes (~500K-1M events)
- **After transform pipeline**: Events modified in-place (same vector, smaller strings)
- **After filtering**: ~10-20% of original events retained
- **Peak memory**: Original size + minimal transform state
- **Final memory**: ~15-25MB (**75-85% reduction**)

```rust
impl ContentExtractor {
    pub fn extract(&self, cast: &mut AsciicastFile, config: &ExtractionConfig) -> AnalysisContent {
        // Build pipeline from config
        let mut pipeline = config.build_pipeline();

        // Transform events IN-PLACE (no allocation of new vector)
        pipeline.transform(&mut cast.events);

        // Shrink vector after filtering removed events
        cast.events.shrink_to_fit();

        // Create analysis segments from cleaned events
        self.create_segments(&cast.events)
    }

    fn create_segments(&self, events: &[Event]) -> AnalysisContent {
        // Group consecutive events into segments based on time gaps
        // A new segment starts when gap between events exceeds threshold (e.g., 2.0s)
        let mut segments = Vec::new();
        let mut current_segment_start = 0;
        let mut current_segment_content = String::new();
        let mut last_time = 0.0;
        const TIME_GAP_THRESHOLD: f64 = 2.0; // seconds

        for (i, event) in events.iter().enumerate() {
            let gap = event.time - last_time;

            // Start new segment on significant time gap
            if gap > TIME_GAP_THRESHOLD && !current_segment_content.is_empty() {
                segments.push(AnalysisSegment {
                    start_time: events[current_segment_start].time,
                    end_time: last_time,
                    content: std::mem::take(&mut current_segment_content),
                    estimated_tokens: 0, // Calculated below
                    event_range: (current_segment_start, i),
                });
                current_segment_start = i;
            }

            if let Some(data) = event.data() {
                current_segment_content.push_str(data);
            }
            last_time = event.time;
        }

        // Don't forget final segment
        if !current_segment_content.is_empty() {
            segments.push(AnalysisSegment {
                start_time: events[current_segment_start].time,
                end_time: last_time,
                content: current_segment_content,
                estimated_tokens: 0,
                event_range: (current_segment_start, events.len()),
            });
        }

        // Calculate token estimates for each segment
        let estimator = TokenEstimator::default();
        for segment in &mut segments {
            segment.estimated_tokens = estimator.estimate(&segment.content);
        }

        let total_tokens = segments.iter().map(|s| s.estimated_tokens).sum();

        AnalysisContent {
            segments,
            total_duration: last_time,
            total_tokens,
            stats: self.stats.clone(),
        }
    }
}
```

**Key insight**: The Transform trait's in-place mutation design was specifically created for handling 100MB+ files efficiently (see `transform.rs` documentation).

#### Options Considered

##### Option A: VTE-Based Terminal Emulation
Full terminal emulation to extract "what the user saw".

- Pros: Semantically accurate screen state
- Cons: **Massive overkill** - LLMs don't need visual state, colors, cursor position. They need TEXT + TIMESTAMPS.

##### Option B: Regex-Based ANSI Stripping
Pre-compiled regex to match and remove patterns.

- Pros: Fast, one-liner
- Cons: No span tracking, pattern maintenance, no progress deduplication

##### Option C: Transform Pipeline with Event Preservation [SELECTED]
Use existing Transform trait to build composable extraction pipeline that modifies events in-place.

- Pros:
  - **Leverages existing infrastructure** (`src/asciicast/transform.rs`)
  - Timestamps preserved naturally (events retain their time field)
  - Handles progress line deduplication via transform
  - Memory efficient (in-place mutation, no copies)
  - Composable and extensible (add/remove transforms easily)
  - Well-tested pattern already in codebase
  - Zero external dependencies
- Cons:
  - Pipeline design requires careful ordering of transforms
  - Events that become empty need filtering

**Decision**: Option C. The Transform trait provides a proven foundation. Events naturally preserve their timestamps, eliminating complex position→timestamp mapping.

---

### Chunking Strategy Options

#### Option A: Token-Budget Based Chunking [SELECTED]
Calculate chunk count from token budget, divide content to fit.

```rust
pub struct TokenBudget {
    pub max_input_tokens: usize,      // Agent-specific limit
    pub reserved_for_prompt: usize,   // ~2000 tokens
    pub reserved_for_output: usize,   // ~8000 tokens
    pub safety_margin_pct: f64,       // 10-15%
}

impl TokenBudget {
    pub fn available_for_content(&self) -> usize {
        let usable = self.max_input_tokens - self.reserved_for_prompt - self.reserved_for_output;
        (usable as f64 * (1.0 - self.safety_margin_pct)) as usize
    }
}
```

- Pros: Adapts to content size, respects agent limits, maximizes efficiency
- Cons: Token estimation is imperfect (mitigated by safety margin)

#### Option B: Adaptive Time-Based Chunking
Start with time-based chunks, validate token count, split if needed.

- Pros: Self-correcting, handles varying content density
- Cons: Multiple passes, may create uneven chunks

#### Option C: Fixed Time-Based Chunking
Hard-coded chunk durations (e.g., 5 minutes).

- Pros: Simple
- Cons: Ignores content density, may exceed limits or waste capacity

**Decision**: Option A. Dynamic token-budget chunking adapts to actual content and respects agent-specific context limits.

#### Chunk Overlap Strategy

Adjacent chunks should overlap to provide context continuity. Without overlap, the LLM might miss important context at chunk boundaries.

```rust
pub struct ChunkConfig {
    /// Overlap as percentage of chunk size (0.0 - 0.2)
    pub overlap_pct: f64,    // Default: 0.10 (10%)
    /// Minimum overlap in tokens
    pub min_overlap_tokens: usize,  // Default: 500
}
```

**Example**: With 160K available tokens and 10% overlap:
- Chunk 1: tokens 0-160K
- Chunk 2: tokens 144K-304K (16K overlap)
- Chunk 3: tokens 288K-448K (16K overlap)

**Deduplication**: Overlapping chunks may produce duplicate markers. Deduplicate by:
1. Sort all markers by timestamp
2. For markers within a time window (e.g., 2 seconds) with same category, keep only the first

> **See SPEC.md Section 5** for complete overlap implementation and deduplication code.

---

### Worker Scaling Options

#### Option A: Content-Based Heuristic Scaling [SELECTED]
Scale workers based on content size and chunk count.

```rust
pub fn calculate_workers(&self, content: &AnalysisContent, chunks: &[ChunkBoundary]) -> usize {
    if let Some(user_override) = self.user_override {
        return user_override.clamp(1, self.max_workers);
    }

    let chunk_count = chunks.len();

    // Scale factor based on content size
    let scale = match content.estimated_tokens {
        0..=100_000 => 0.5,
        100_001..=500_000 => 1.0,
        _ => 1.5,
    };

    let scaled = (chunk_count as f64 * scale).ceil() as usize;

    // Cap by CPU count
    let cpu_count = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4);

    scaled.clamp(self.min_workers, self.max_workers.min(cpu_count))
}
```

| Estimated Tokens | Typical Chunks | Workers |
|-----------------|----------------|---------|
| < 100K | 1 | 1 |
| 100K - 500K | 1-3 | 1-3 |
| 500K - 1M | 3-6 | 3-6 |
| > 1M | 6+ | 4-8 |

- Pros: Adapts to content, respects system resources, configurable
- Cons: Heuristics may not be optimal for all cases

#### Option B: Fixed Worker Count
Always use N workers (e.g., 4).

- Pros: Simple, predictable
- Cons: Wasteful for small files, insufficient for large files

**Decision**: Option A. Dynamic scaling based on content size and chunk count.

---

### Parallelism Options

#### Option A: std::thread + std::sync::mpsc
Standard library threading with message passing.

- Pros: No dependencies, well-understood
- Cons:
  - Manual thread cleanup (must call `join()`)
  - `std::mpsc` affected by same double-free bug as crossbeam (fixed in Rust 1.87)
  - Mutex contention on shared work queue

#### Option B: std::thread + crossbeam-channel
Standard threads with crossbeam's lock-free channels.

- Pros: Faster channels, cloneable receivers (MPMC), `select!` macro
- Cons:
  - **SECURITY CONCERN**: [RUSTSEC-2025-0024](https://rustsec.org/advisories/RUSTSEC-2025-0024.html) - double-free bug discovered April 2025
  - [Materialize analysis](https://materialize.com/blog/rust-concurrency-bug-unbounded-channels/) shows bug was undetected for over a year
  - Manual thread cleanup still required

#### Option C: Rayon [SELECTED]
Data parallelism library with work-stealing thread pool.

```rust
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub fn analyze_parallel(chunks: Vec<AnalysisChunk>, worker_count: usize) -> Vec<WorkResult> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(worker_count)
        .build()
        .unwrap();

    let completed = Arc::new(AtomicUsize::new(0));
    let total = chunks.len();

    pool.install(|| {
        chunks.into_par_iter()
            .map(|chunk| {
                let result = process_chunk(&chunk);
                let done = completed.fetch_add(1, Ordering::SeqCst) + 1;
                report_progress(done, total);
                result
            })
            .collect()
    })
}
```

- Pros:
  - **Automatic thread cleanup** (critical requirement)
  - [Battle-tested, gold standard](https://gendignoux.com/blog/2024/11/18/rust-rayon-optimized.html) - 7M+ downloads/month
  - No known security vulnerabilities
  - Clean API with `par_iter()`
  - Configurable thread pool size
  - Work-stealing for efficiency
- Cons:
  - New dependency
  - Progress reporting requires `AtomicUsize` (acceptable)

#### Option D: Tokio
Async runtime with cooperative multitasking.

- Pros: Excellent for I/O-bound work, clean concurrency control
- Cons:
  - [Subprocess spawning is blocking](https://docs.rs/tokio/latest/tokio/process/struct.Command.html) - negates async benefits
  - Heavy dependency
  - Requires async/await throughout codebase
  - Overkill for 1-8 concurrent processes

#### Option E: std::thread::scope (Rust 1.63+)
Scoped threads with automatic cleanup.

```rust
std::thread::scope(|s| {
    for chunk in &chunks {
        s.spawn(|| process_chunk(chunk));
    }
});  // All threads joined here, guaranteed
```

- Pros: Automatic cleanup, no external dependencies
- Cons: Less ergonomic than Rayon, no work-stealing

**Decision**: Rayon (Option C) selected for:
1. Automatic thread cleanup (critical for reliability)
2. No recent security vulnerabilities (unlike crossbeam)
3. Battle-tested in production
4. Clean API for parallel iteration

> **See SPEC.md Section 3** for detailed parallelization logic, chunk count calculation, worker scaling formulas, and execution flow code.

---

### Module Structure Options

#### Option A: Flat Module with Submodules [RECOMMENDED]

```
src/
├── analyzer/
│   ├── mod.rs              # Re-exports, AnalyzerService facade
│   ├── content.rs          # ContentExtractor, strip_ansi(), ContentSegment
│   ├── chunk.rs            # ChunkCalculator, TokenBudget, AnalysisChunk
│   ├── worker.rs           # WorkerScaler, parallel execution with Rayon
│   ├── backend.rs          # AgentBackend trait + Claude/Codex/Gemini impls
│   ├── result.rs           # RawMarker, ValidatedMarker, ResultAggregator
│   ├── tracker.rs          # TokenTracker, usage metrics
│   ├── progress.rs         # ProgressReporter, user feedback
│   └── error.rs            # AnalysisError, user-friendly messages
├── analyzer.rs             # REMOVE (replaced by analyzer/)
└── commands/
    └── analyze.rs          # CLI handler (updated to use new module)
```

- Pros:
  - Clear separation by responsibility
  - Each file is focused and testable
  - Easy to navigate
  - Follows existing codebase patterns (e.g., `asciicast/`, `player/`)
- Cons:
  - More files to manage
  - Some files may be small

#### Option B: Nested Backend Submodule

```
src/
├── analyzer/
│   ├── mod.rs              # Re-exports, AnalyzerService
│   ├── content.rs          # ContentExtractor, ANSI stripping
│   ├── chunk.rs            # Chunking logic
│   ├── worker.rs           # Parallel execution
│   ├── result.rs           # Result aggregation
│   ├── tracker.rs          # Token tracking
│   ├── progress.rs         # Progress reporting
│   ├── error.rs            # Error types
│   └── backend/            # Nested submodule for agents
│       ├── mod.rs          # AgentBackend trait, AgentType enum
│       ├── claude.rs       # ClaudeBackend
│       ├── codex.rs        # CodexBackend
│       └── gemini.rs       # GeminiBackend
```

- Pros:
  - Agent backends fully isolated
  - Adding new agents = adding new file
  - Very clean extension point
- Cons:
  - Deeper nesting
  - May be overkill for 3 backends

#### Option C: Minimal Structure

```
src/
├── analyzer/
│   ├── mod.rs              # Everything: service, chunking, workers
│   ├── backend.rs          # All agent backends in one file
│   ├── content.rs          # Content extraction
│   └── result.rs           # Results and errors
```

- Pros:
  - Fewer files
  - Simpler to start
- Cons:
  - Files grow large over time
  - Harder to test in isolation
  - Mixed responsibilities

**Decision**: Option B selected. Nested backend submodule provides cleaner extension point for agents while maintaining clear separation of concerns.

---

## Decision

### Selected Approaches

| Component | Decision | Rationale |
|-----------|----------|-----------|
| Content Extraction | Transform Pipeline (existing trait) | Leverages proven infrastructure, in-place mutation, natural timestamp preservation |
| Chunking | Token-budget based | Adapts to content, respects agent context limits |
| Worker Scaling | Content-based heuristic | Dynamic scaling 1-8 workers based on content |
| Parallelism | Rayon | Reliability, automatic thread cleanup, no security vulnerabilities |
| Module Structure | Option B (nested backend/) | Clean extension point for agents |

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            AnalyzerService (Facade)                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌──────────────┐   ┌───────────────┐   ┌────────────┐   ┌──────────────┐  │
│  │ AsciicastFile │──▶│ ContentExtractor│──▶│ Chunker    │──▶│ WorkerScaler │  │
│  │ (parse)       │   │ (ANSI strip)   │   │(token-based)│   │(dynamic)     │  │
│  └──────────────┘   └───────────────┘   └────────────┘   └──────────────┘  │
│                                                                     │        │
│                                                                     ▼        │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │                         Rayon Thread Pool                                ││
│  │  chunks.into_par_iter().map(|c| backend.analyze(c)).collect()           ││
│  └─────────────────────────────────────────────────────────────────────────┘│
│                                          │                                   │
│            ┌─────────────────────────────┼─────────────────────────┐        │
│            ▼                             ▼                         ▼        │
│     ┌─────────────┐              ┌─────────────┐           ┌─────────────┐  │
│     │AgentBackend │              │AgentBackend │           │AgentBackend │  │
│     │  (Claude)   │              │  (Codex)    │           │  (Gemini)   │  │
│     └─────────────┘              └─────────────┘           └─────────────┘  │
│            │                             │                         │        │
│            └─────────────────────────────┼─────────────────────────┘        │
│                                          ▼                                   │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │                       ResultAggregator                                   ││
│  │  (merge markers, deduplicate, sort by timestamp)                        ││
│  └─────────────────────────────────────────────────────────────────────────┘│
│                                          │                                   │
│                                          ▼                                   │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │                        MarkerManager                                     ││
│  │  (write markers to cast file)                                           ││
│  └─────────────────────────────────────────────────────────────────────────┘│
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Design Patterns (R7)

| Component | Pattern | Justification |
|-----------|---------|---------------|
| Transform/TransformChain | **Pipeline** | Composable, ordered processing stages (existing infrastructure) |
| AgentBackend | **Strategy** | Interchangeable agent implementations |
| AnalyzerService | **Facade** | Simple public API hiding complexity |
| ContentExtractor | **Adapter** | Transforms raw events to analysis format |
| ResultAggregator | **Builder** | Step-by-step construction of marker set |
| TokenTracker | **Observer** | Collects metrics from workers |
| ExtractionConfig | **Builder** | Configurable pipeline construction |

---

## Existing Marker Handling (R9)

When analyzing a file that already has markers:

```rust
impl AnalyzerService {
    pub fn analyze(&self, path: &Path, options: &AnalyzeOptions) -> Result<AnalysisResult> {
        let cast = AsciicastFile::read(path)?;

        // Check for existing markers (R9)
        let existing_markers: Vec<_> = cast.events.iter()
            .filter(|e| e.is_marker())
            .collect();

        if !existing_markers.is_empty() {
            eprintln!(
                "⚠ Warning: File already contains {} marker(s). New markers will be added alongside existing ones.",
                existing_markers.len()
            );
        }

        // Proceed with analysis...
    }
}
```

**Behavior:**
- Warn user if file has existing markers
- Do NOT prevent re-analysis (just warn)
- New markers are added alongside existing ones
- No automatic deduplication with existing markers (keep it simple per R9)

---

## Partial Chunk Success Policy

When parallel analysis results in some chunks succeeding and others failing:

```rust
/// Policy: Write all successful markers, report failed chunks
pub struct PartialSuccessPolicy;

impl PartialSuccessPolicy {
    pub fn handle_results(
        results: Vec<ChunkResult>,
        total_chunks: usize,
    ) -> Result<(Vec<ValidatedMarker>, PartialSuccessReport)> {
        let (successes, failures): (Vec<_>, Vec<_>) = results
            .into_iter()
            .partition(|r| r.result.is_ok());

        let markers: Vec<ValidatedMarker> = successes
            .into_iter()
            .flat_map(|r| r.result.unwrap())
            .collect();

        let report = PartialSuccessReport {
            successful_chunks: total_chunks - failures.len(),
            failed_chunks: failures.len(),
            failed_time_ranges: failures.iter()
                .map(|r| r.time_range.clone())
                .collect(),
            markers_found: markers.len(),
        };

        // Partial success is acceptable - write what we have
        if markers.is_empty() && !failures.is_empty() {
            return Err(AnalysisError::AllChunksFailed {
                total: total_chunks,
                errors: failures.into_iter()
                    .map(|r| r.result.unwrap_err())
                    .collect(),
            });
        }

        Ok((markers, report))
    }
}

pub struct PartialSuccessReport {
    pub successful_chunks: usize,
    pub failed_chunks: usize,
    pub failed_time_ranges: Vec<TimeRange>,
    pub markers_found: usize,
}
```

**Automatic retry for failed chunks:**

```rust
/// Automatic retry with rate limit awareness
pub struct RetryPolicy {
    pub max_attempts: usize,     // Default: 3
    pub initial_delay_ms: u64,   // Default: 1000
    pub backoff_multiplier: f64, // Default: 2.0
    pub max_delay_ms: u64,       // Default: 60000 (1 minute)
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

/// Rate limit information extracted from agent response
#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    /// When the rate limit resets (if provided by agent)
    pub retry_after: Option<Duration>,
    /// Human-readable message
    pub message: String,
}

/// Backend error with optional rate limit info
#[derive(Debug)]
pub enum BackendError {
    RateLimited(RateLimitInfo),
    Timeout(Duration),
    ExitCode { code: i32, stderr: String },
    JsonParse(serde_json::Error),
    Io(std::io::Error),
}

impl BackendError {
    /// Extract wait duration - use agent-provided retry_after if available
    pub fn wait_duration(&self, fallback_ms: u64) -> Duration {
        match self {
            BackendError::RateLimited(info) => {
                info.retry_after.unwrap_or(Duration::from_millis(fallback_ms))
            }
            _ => Duration::from_millis(fallback_ms),
        }
    }
}

/// Parse rate limit info from agent CLI stderr
pub fn parse_rate_limit_info(agent: AgentType, stderr: &str) -> Option<RateLimitInfo> {
    // Claude: "Rate limited. Retry after 45 seconds" or "retry_after_seconds: 45"
    // Codex: "throttled, retry in 30s"
    // Gemini: "RESOURCE_EXHAUSTED... retryDelay: 60s"

    let retry_after = extract_retry_seconds(stderr).map(Duration::from_secs);

    if stderr.contains("rate limit") || stderr.contains("throttled") ||
       stderr.contains("RESOURCE_EXHAUSTED") || stderr.contains("429") {
        Some(RateLimitInfo {
            retry_after,
            message: stderr.lines().next().unwrap_or("Rate limited").to_string(),
        })
    } else {
        None
    }
}

/// Extract retry delay from various formats
fn extract_retry_seconds(stderr: &str) -> Option<u64> {
    // Pattern: "retry after X seconds" or "retry_after_seconds: X" or "retry in Xs"
    let patterns = [
        r"retry.?after.?(\d+)\s*s",      // "retry after 45 seconds", "retry_after_seconds: 45"
        r"retry.?in.?(\d+)\s*s",         // "retry in 30s"
        r"retryDelay:\s*(\d+)\s*s",      // "retryDelay: 60s"
        r"(\d+)\s*seconds?\s*(?:remain|left|wait)", // "45 seconds remaining"
    ];

    for pattern in patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(caps) = re.captures(&stderr.to_lowercase()) {
                if let Some(m) = caps.get(1) {
                    if let Ok(secs) = m.as_str().parse() {
                        return Some(secs);
                    }
                }
            }
        }
    }
    None
}

/// Retry failed chunks automatically, respecting rate limit wait times
pub fn analyze_with_retry(
    chunks: Vec<AnalysisChunk>,
    backend: &dyn AgentBackend,
    policy: &RetryPolicy,
) -> Vec<ChunkResult> {
    let mut results = Vec::with_capacity(chunks.len());

    for chunk in chunks {
        let mut last_error = None;
        let mut delay_ms = policy.initial_delay_ms;

        for attempt in 1..=policy.max_attempts {
            match backend.invoke(&build_prompt(&chunk), Duration::from_secs(120)) {
                Ok(response) => {
                    results.push(ChunkResult::success(chunk.id, response));
                    break;
                }
                Err(e) => {
                    // Use agent-provided retry_after if available, otherwise exponential backoff
                    let wait = e.wait_duration(delay_ms).min(Duration::from_millis(policy.max_delay_ms));

                    if attempt < policy.max_attempts {
                        eprintln!("  Chunk {} failed (attempt {}/{}), waiting {:?}...",
                            chunk.id, attempt, policy.max_attempts, wait);
                        std::thread::sleep(wait);

                        // Only apply exponential backoff if no retry_after was provided
                        if !matches!(&e, BackendError::RateLimited(info) if info.retry_after.is_some()) {
                            delay_ms = (delay_ms as f64 * policy.backoff_multiplier)
                                .min(policy.max_delay_ms as f64) as u64;
                        }
                    }
                    last_error = Some(e);
                }
            }
        }

        if let Some(e) = last_error {
            if results.last().map(|r| r.chunk_id != chunk.id).unwrap_or(true) {
                results.push(ChunkResult::failure(chunk.id, e));
            }
        }
    }

    results
}
```

**User feedback for partial success (after automatic retries):**
```
Analyzing session... (4 chunks, ~380K tokens)
  [1/4] ████████████████████ done (32s)
  [2/4] ████████████████████ done (28s)
  [3/4] failed (attempt 1/3), retrying in 1000ms...
  [3/4] failed (attempt 2/3), retrying in 2000ms...
  [3/4] ████████████████████ done (retry succeeded)
  [4/4] ████████████████████ done (29s)

✓ Added 15 markers to session.cast
```

**If all retries fail:**
```
Analyzing session... (4 chunks, ~380K tokens)
  [1/4] ████████████████████ done (32s)
  [2/4] ████████████████████ done (28s)
  [3/4] failed (attempt 1/3), retrying in 1000ms...
  [3/4] failed (attempt 2/3), retrying in 2000ms...
  [3/4] failed (attempt 3/3)
  [4/4] ████████████████████ done (29s)

⚠ Analysis partially complete:
   ✓ 3/4 chunks analyzed (after automatic retries)
   ✕ 1 chunk failed (timestamps 450.0s - 600.0s)

   12 markers added. The failed time range may have missing markers.
```

**Design rationale:**
- Retry is automatic - no user intervention needed
- Exponential backoff avoids hammering rate-limited APIs
- Partial success is better than total failure
- User gets value from successful chunks immediately
- Failed time ranges are clearly reported after all retries exhausted

---

## Token Tracking & Visibility (R6)

Track resource usage and provide visibility:

```rust
/// Tracks token usage across analysis for visibility and smart decisions
pub struct TokenTracker {
    /// Tokens used per chunk
    chunk_usage: Vec<ChunkUsage>,
    /// Running total
    total_tokens_estimated: usize,
    /// Start time for duration tracking
    start_time: Instant,
}

pub struct ChunkUsage {
    pub chunk_id: usize,
    pub estimated_tokens: usize,
    pub actual_response_tokens: Option<usize>,  // If agent reports it
    pub duration: Duration,
    pub success: bool,
}

impl TokenTracker {
    /// Report usage summary to user (R6: visibility)
    pub fn report_summary(&self) {
        eprintln!("\n📊 Analysis Summary:");
        eprintln!("   Chunks processed: {}", self.chunk_usage.len());
        eprintln!("   Estimated tokens: ~{}", self.total_tokens_estimated);
        eprintln!("   Total duration: {:?}", self.start_time.elapsed());

        let success_rate = self.chunk_usage.iter()
            .filter(|c| c.success)
            .count() as f64 / self.chunk_usage.len() as f64 * 100.0;
        eprintln!("   Success rate: {:.0}%", success_rate);
    }

    /// Inform retry decisions (R8: smart retry)
    pub fn should_retry_chunk(&self, chunk_id: usize) -> bool {
        // Prefer retrying smaller chunks first
        if let Some(usage) = self.chunk_usage.iter().find(|c| c.chunk_id == chunk_id) {
            usage.estimated_tokens < 50_000  // Retry small chunks
        } else {
            true
        }
    }
}
```

**Visibility output example:**
```
⚠ Warning: File already contains 3 marker(s). New markers will be added alongside existing ones.
Analyzing session... (4 chunks, ~380K tokens)
  [1/4] ████████████████████ done (32s)
  [2/4] ████████████████████ done (28s)
  [3/4] ████████████████████ done (31s)
  [4/4] ████████████████████ done (29s)

📊 Analysis Summary:
   Chunks processed: 4
   Estimated tokens: ~380,000
   Total duration: 2m 0s
   Success rate: 100%

✓ Added 12 markers to session.cast
```

---

## Testing Strategy

All implementation follows **Test-Driven Development (TDD)**:
- Write failing tests before implementation
- Use `cargo insta` for snapshot testing of transformations
- Test fixtures derived from real cast files (not actual user files)
- Property-based testing with `proptest` for invariants

> **See SPEC.md Section 6** for complete TDD strategy, snapshot workflow, test categories, and coverage goals.

### Required Test Cases per Transform

| Transform | Test Cases |
|-----------|------------|
| **StripAnsiCodes** | CSI color codes, cursor movement, OSC hyperlinks, nested sequences, partial sequences at boundaries, UTF-8 + ANSI mix |
| **StripControlCharacters** | BEL (0x07), BS (0x08), NUL, preserves \t \n \r, C1 controls |
| **StripBoxDrawing** | All box chars (─│┌┐└┘), rounded corners (╭╮╰╯), block elements (▖▗▘▝), preserves normal text |
| **StripSpinnerChars** | Claude spinners (✻✳✢✶✽), Gemini braille (⠋⠙⠹...), **PRESERVES semantic chars** (✓✔✕⚠ℹ☐☑) |
| **StripProgressBlocks** | Full blocks (█), shaded (░▒▓), triangles (▼▲), preserves text |
| **DeduplicateProgressLines** | Simple \r overwrite, multiple \r in sequence, \r without \n, preserves markers, timestamp correctness |
| **NormalizeWhitespace** | Multiple spaces → single, multiple \n → max 2, mixed whitespace |
| **FilterEmptyEvents** | Removes empty/whitespace-only, preserves markers, preserves non-empty |
| **Full Pipeline** | SPEC.md Section 1.7 before/after examples, compression ratio 55-89% |

### Property-Based Test Invariants

```rust
// All transforms must satisfy:

#[test]
fn transform_preserves_markers() {
    // Markers must never be modified or removed
}

#[test]
fn transform_never_increases_size() {
    // Output size <= input size (stripping only removes)
}

#[test]
fn transform_is_idempotent() {
    // Running twice produces same result as running once
}

#[test]
fn transform_preserves_event_order() {
    // Events remain in timestamp order
}

#[test]
fn timestamps_remain_valid() {
    // All timestamps >= 0, in non-decreasing order
}
```

---

## Consequences

### What Becomes Easier

- Adding new agent backends (implement trait, done)
- Analyzing large files (parallel processing)
- Understanding analysis progress (clear feedback)
- Testing components in isolation
- Maintaining code (clear separation of concerns)

### What Becomes Harder

- Nothing significant - this is improvement over monolithic design

### Follow-ups for Later

- Self-learning token service (adaptive thresholds based on history)
- Streaming analysis (process events as they arrive)
- Custom marker categories (user-defined)

---

## Decision History

1. **Token-budget chunking selected** - Dynamic chunking based on agent context limits, not arbitrary time windows.

2. **VTE rejected for content extraction** - LLMs need text content, not visual screen state. VTE simulates terminal rendering which is irrelevant. Simple ANSI stripping is sufficient.

3. **Rayon recommended for parallelism** - Automatic thread cleanup is critical. crossbeam has recent security vulnerability (RUSTSEC-2025-0024). Rayon is battle-tested with no known issues.

4. **Dynamic worker scaling confirmed** - Scale workers based on content size and chunk count, respecting CPU cores.

5. **In-memory only processing** - No temp files. Even 400MB files should be processed entirely in memory via chunking.

6. **Subscription-based resource tracking** - Token tracking is for smart decisions (when to parallelize, retry strategies), not billing.

7. **Transform trait pattern adopted** - Existing `src/asciicast/transform.rs` provides the foundation for content extraction. The `Transform` trait and `TransformChain` enable composable, in-place event processing that naturally preserves timestamps.

8. **Rayon confirmed for parallelism** - Selected over std::thread (manual cleanup), crossbeam (security vulnerability RUSTSEC-2025-0024), and tokio (overkill for subprocess spawning).

9. **Module Structure Option B confirmed** - Nested `backend/` submodule provides clean extension point for adding new agent backends.

---

## Appendix: Research Findings

### crossbeam-channel Vulnerability (RUSTSEC-2025-0024)

- **Issue**: Double-free on Drop in unbounded channels
- **Discovered**: April 2025 by Materialize engineers
- **Duration**: Bug was present for over a year undetected
- **Impact**: Undefined behavior under rare but realizable conditions
- **Fix**: Included in Rust 1.87.0
- **Source**: [Materialize Blog](https://materialize.com/blog/rust-concurrency-bug-unbounded-channels/)

This reinforces the importance of choosing well-tested, actively maintained libraries and preferring automatic resource cleanup.

### Subprocess Spawning Performance

From [Kobzol's analysis](https://kobzol.github.io/rust/2024/01/28/process-spawning-performance-in-rust.html):
- glibc version affects spawn performance significantly
- Memory overhead impacts spawning speed
- Parallelization shows modest improvements for subprocess-heavy workloads

### Rayon Production Use

- 7M+ downloads per month
- Used by major companies for performance-critical workloads
- Described as "gold standard" for Rust parallelism
- No known security vulnerabilities
- Source: [Rayon Optimization Blog](https://gendignoux.com/blog/2024/11/18/rust-rayon-optimized.html)
