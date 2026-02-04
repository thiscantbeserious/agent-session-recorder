# ADR: Refactor Analyze Command

## Status

**Approved** - Ready for implementation

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

Token estimation: ~4 characters per token (rough heuristic).

### Research: Agent CLI Capabilities

| Feature | Claude | Codex | Gemini |
|---------|--------|-------|--------|
| Non-interactive mode | `--print` | `exec` | positional prompt |
| JSON output | `--output-format json` | N/A (text only) | `--output-format json` |
| Structured schema | `--json-schema <schema>` | N/A | N/A |
| Permission bypass | `--dangerously-skip-permissions` | `--dangerously-bypass-approvals-and-sandbox` | `--yolo` |
| Stdin input | Yes (pipe) | Yes (pipe) | Yes (pipe) |

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
        .with(DeduplicateProgressLines::new()) // Keep only final state of \r lines
        .with(NormalizeWhitespace::new())      // Collapse excessive newlines
        .with(StripBinaryGarbage::new())       // Remove non-UTF8 data
}

/// Each transform implements the existing Transform trait
struct StripAnsiCodes { /* state for tracking partial sequences */ }

impl Transform for StripAnsiCodes {
    fn transform(&mut self, events: &mut Vec<Event>) {
        for event in events.iter_mut() {
            if let Some(data) = event.data_mut() {
                *data = strip_ansi_from_string(data);
            }
        }
    }
}
```

**Key insight**: The Transform trait works on `Vec<Event>`, preserving timestamps naturally. We don't need a separate mapping system - the events retain their original timestamps throughout the pipeline.

#### What is "Useless" for LLMs?

It's not just ANSI codes. Terminal output contains many categories of noise:

| Category | Examples | Why Useless |
|----------|----------|-------------|
| **ANSI Escape Sequences** | `\x1b[38;5;174m`, `\x1b[H`, `\x1b[2J` | Rendering instructions, no semantic value |
| **Control Characters** | `\x07` (BEL), `\x08` (BS), `\x00`-`\x06` | Audio/visual signals, no content |
| **Progress Bar Spam** | Same line rewritten 1000x via `\r` | Only final state matters |
| **Spinner Characters** | `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` cycling | Visual animation, no content |
| **Box Drawing** | `┌─┐│└─┘├┤┬┴┼` | Decorative framing |
| **Excessive Whitespace** | `\n\n\n\n\n\n` | Adds nothing |
| **Binary Garbage** | Non-UTF8, image data, base64 blobs | Corrupted or embedded data |

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
#[derive(Debug, Default)]
pub struct ExtractionStats {
    pub original_bytes: usize,
    pub extracted_bytes: usize,
    pub ansi_sequences_stripped: usize,
    pub control_chars_stripped: usize,
    pub progress_lines_deduplicated: usize,
    pub events_processed: usize,
    pub events_retained: usize,
}
```

#### LLM Response Mapping

When the LLM returns markers, they include timestamps relative to the chunk. The `AnalysisChunk` handles mapping back to absolute timestamps:

```rust
/// Marker position types the LLM can return
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

1. **ANSI Stripping**: Remove all escape sequences
2. **Control Character Removal**: Strip BEL, BS, NUL, etc.
3. **Progress Line Deduplication**: Keep only final state of `\r`-rewritten lines
4. **Whitespace Normalization**: Collapse excessive newlines/spaces
5. **Span Position Adjustment**: Update span positions after each transform

```rust
pub struct ExtractionConfig {
    pub strip_ansi: bool,                    // Always true
    pub strip_control_chars: bool,           // Always true
    pub dedupe_progress_lines: bool,         // True (critical for size reduction)
    pub normalize_whitespace: bool,          // True
    pub max_consecutive_newlines: usize,     // 2
    pub strip_box_drawing: bool,             // False (may have meaning)
    pub extract_hyperlink_urls: bool,        // True (URLs can be useful)
}
```

#### Memory Efficiency for 100MB+ Files

For a 100MB cast file:
- **Original**: 100MB of events with ANSI codes
- **After extraction**: ~10-20MB clean text + ~2-4MB span metadata
- **Spans**: ~40 bytes each, ~100K spans = ~4MB
- **Total in-memory**: ~15-25MB (**75-85% reduction**)

```rust
impl ContentExtractor {
    pub fn extract(&self, cast: &AsciicastFile) -> ExtractedContent {
        // Pre-allocate based on estimated output size (20% of original)
        let estimated_size = self.estimate_output_size(cast);
        let mut output = String::with_capacity(estimated_size);
        let mut spans = Vec::with_capacity(cast.events.len() / 10);

        // ... extraction logic ...

        // Shrink to actual size
        output.shrink_to_fit();
        spans.shrink_to_fit();

        ExtractedContent { text: output, spans, ... }
    }
}
```

#### Options Considered

##### Option A: VTE-Based Terminal Emulation
Full terminal emulation to extract "what the user saw".

- Pros: Semantically accurate screen state
- Cons: **Massive overkill** - LLMs don't need visual state, colors, cursor position. They need TEXT + TIMESTAMPS.

##### Option B: Regex-Based ANSI Stripping
Pre-compiled regex to match and remove patterns.

- Pros: Fast, one-liner
- Cons: No span tracking, pattern maintenance, no progress deduplication

##### Option C: Streaming Extraction with Span Tracking [SELECTED]
Character-by-character processing with full position→timestamp mapping.

- Pros:
  - Preserves bidirectional mapping (critical for marker placement)
  - Handles progress line deduplication
  - Memory efficient (streaming, pre-allocated)
  - Configurable transforms
  - Zero external dependencies
- Cons:
  - More complex than naive stripping (~200 lines vs ~50)
  - Span metadata adds memory overhead (~4MB for large files)

**Decision**: Option C. The mapping problem is fundamental - we MUST track where content came from to place markers accurately.

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
| Content Extraction | Streaming with span tracking | Preserves position→timestamp mapping for accurate marker placement |
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
| AgentBackend | **Strategy** | Interchangeable agent implementations |
| AnalyzerService | **Facade** | Simple public API hiding complexity |
| ContentExtractor | **Adapter** | Transforms raw events to analysis format |
| ResultAggregator | **Builder** | Step-by-step construction of marker set |
| TokenTracker | **Observer** | Collects metrics from workers |

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
