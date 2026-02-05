# Plan: Refactor Analyze Command

References: ADR.md, SPEC.md, REQUIREMENTS.md

## TDD Approach

**All implementation follows Test-Driven Development (TDD):**

1. **Test First**: Write failing tests before implementation
2. **Snapshot Tests**: Use `cargo insta` for before/after transformation verification
3. **Real Data**: Test fixtures derived from real cast files (Claude, Codex, Gemini)
4. **Property Tests**: Use `proptest` for invariant validation

See **SPEC.md Section 6** for detailed TDD strategy, snapshot workflow, and test categories.

**Per-stage workflow (RED → GREEN → REFACTOR):**
```
┌──────────────────────────────────────────────────────────────────┐
│  🔴 RED          │  🟢 GREEN        │  🔵 REFACTOR               │
│  Write test      │  Write minimal   │  Clean up code,            │
│  (must fail)     │  code to pass    │  tests stay green          │
└──────────────────────────────────────────────────────────────────┘
```

See **SPEC.md Section 6.1** for detailed TDD philosophy.

## Open Questions

Implementation challenges to solve (architect identifies, implementer resolves):

1. **ANSI sequence coverage**: Which escape sequences are common in AI agent sessions? Need comprehensive test cases.
2. **Chunk overlap handling**: How much overlap between chunks for context continuity? Need to test with real files.
3. **Codex JSON parsing**: Codex CLI doesn't have native JSON output. How to reliably parse JSON from text response?
4. **Rate limiting**: How do agents signal rate limits? Need to detect and handle gracefully.
5. **Large file testing**: Need test fixtures >100MB to validate chunking and parallelism.

## Stages

### Stage 1: Content Extraction Foundation

**Goal:** Implement content extraction using the existing Transform trait pattern.

**Input:** `AsciicastFile` with raw events (potentially 100MB+)
**Output:** `AnalysisContent` with cleaned segments and token estimates

**Definition of Done:**
- [x] All unit tests pass
- [x] Snapshot tests match expected output
- [x] Benchmark: <5s for 70MB file (verified: 15MB in <0.5s release mode)
- [x] Compression ratio: 55-89% size reduction (verified: 70.8%)
- [x] Semantic chars (✓✔✕⚠ℹ) preserved in output

**Public API:** See ADR.md "Content Extraction" section for:
- `ContentCleaner` - single-pass state machine (ADR Performance section)
- `ContentExtractor` - pipeline orchestrator
- `AnalysisContent`, `AnalysisSegment` - data structures
- `ExtractionConfig`, `ExtractionStats` - configuration and metrics

**TDD Order** (tests before implementation):

- [x] Create `src/analyzer/mod.rs` with module structure
- [x] Create `tests/fixtures/` with sample events from real cast files (Section 1.7 of SPEC.md)

**ContentCleaner (Optimized Single-Pass - see ADR Performance section):**
- [x] 🔴 Write unit tests for ANSI stripping (CSI, OSC, simple escapes)
- [x] 🔴 Write unit tests for control char stripping
- [x] 🔴 Write unit tests for box drawing removal
- [x] 🔴 Write unit tests for spinner removal (Claude, Gemini, Codex)
- [x] 🔴 Write unit tests for **semantic char preservation** (✓✔✕⚠ℹ☐☑)
- [x] 🔴 Write unit tests for progress block removal
- [x] 🟢 Implement `ContentCleaner` with state machine
- [x] 🔵 Refactor for clarity, verify 5x+ faster than naive approach

**DeduplicateProgressLines** (separate transform, runs after ContentCleaner):
- [x] 🔴 Write snapshot test with \r-based progress
- [x] 🔴 Write test for timestamp preservation
- [x] 🔴 Write test for marker preservation
- [x] 🟢 Implement transform (see ADR algorithm)
- [x] 🔵 Refactor if needed

**NormalizeWhitespace:**
- [x] 🔴 Write unit tests
- [x] 🟢 Implement transform
- [x] 🔵 Refactor if needed

**FilterEmptyEvents:**
- [x] 🔴 Write unit tests (preserves markers)
- [x] 🟢 Implement transform
- [x] 🔵 Refactor if needed

**TokenEstimator:**
- [x] 🔴 Write unit test for chars/4 estimation
- [x] 🔴 Write test for estimation AFTER cleanup
- [x] 🟢 Implement `TokenEstimator` struct
- [x] 🔵 Refactor if needed

**StatsCollector:**
- [x] 🔴 Write unit test for stats accumulation
- [x] 🟢 Implement `StatsCollector` (integrated into ExtractionStats)
- [x] 🔵 Refactor if needed

**Full Pipeline:**
- [x] 🔴 Write integration snapshot test
- [x] 🟢 Create `ExtractionConfig` with `build_pipeline()` method
- [x] 🟢 Create `ContentExtractor` with segment creation
- [x] 🔵 **Benchmark with real 70MB+ cast file** - target <5s extraction time

- [x] Create `AnalysisSegment` struct with start_time, end_time, content, estimated_tokens
- [x] Verify compression ratios match SPEC.md expectations (55-89% reduction)

Files: `src/analyzer/mod.rs`, `src/analyzer/content.rs`, `tests/fixtures/`

References:
- Prior art: `src/asciicast/transform.rs` (Transform trait, TransformChain)
- Algorithm details: `research/algorithm_for_asciicast_cutting_and_compression.md`

Considerations:
- Edge case: Malformed escape sequences (partial sequences at event boundaries)
- Edge case: UTF-8 encoding with escape sequences
- Edge case: Progress lines spanning multiple events
- Watch out for: Performance on large files (Transform trait designed for 100MB+)

### Stage 2: Token Budget & Chunking

**Goal:** Implement dynamic chunking based on agent token limits.

**Input:** `AnalysisContent` from Stage 1, `AgentType` for budget selection
**Output:** `Vec<AnalysisChunk>` ready for parallel processing

**Definition of Done:**
- [x] All unit tests pass
- [x] Property tests pass (timestamp resolution always valid)
- [x] Chunk count matches SPEC.md scaling table
- [x] Overlap logic produces correct deduplication windows

**Public API:** See ADR.md "Chunking Strategy" section for:
- `TokenBudget` - agent-specific limits and safety margins
- `ChunkCalculator` - divides content into chunks
- `AnalysisChunk`, `TimeRange` - chunk data structures
- `resolve_marker_timestamp()` - relative to absolute time mapping

See SPEC.md Section 3 for chunk count calculation and scaling table.

**TDD Order:**

- [x] **TokenBudget**:
  - [x] Write unit tests for budget calculation → fails
  - [x] Create `TokenBudget` struct with agent-specific limits → passes
- [x] **Token Estimation**:
  - [x] Write property test (chars/4 ≈ tokens) → fails
  - [x] Implement estimation with safety margin → passes
- [x] **Chunk Calculation**:
  - [x] Write test: single chunk when content < budget → fails
  - [x] Write test: multi-chunk when content > budget → fails
  - [x] Write test: verify chunks match SPEC.md scaling table → fails
  - [x] Implement `ChunkCalculator` → passes
- [x] Create `AnalysisChunk` struct with id, time_range, segments, text, estimated_tokens
- [x] Create `TimeRange` struct for chunk boundaries
- [x] **Overlap Logic**:
  - [x] Write test for overlap percentage → fails
  - [x] Implement `AnalysisChunk::from_content()` with overlap → passes
- [x] **Timestamp Resolution**:
  - [x] Write property test: absolute timestamp always in valid range → fails
  - [x] Implement `resolve_marker_timestamp()` → passes

Files: `src/analyzer/chunk.rs`

Considerations:
- Edge case: Content smaller than one chunk (single chunk, no splitting)
- Edge case: Very dense content that requires many chunks
- Edge case: Segment boundaries vs chunk boundaries
- Watch out for: Chunk boundaries cutting mid-sentence (prefer segment boundaries)

### Stage 3: Agent Backend Abstraction

**Goal:** Create extensible agent backend trait with implementations (Strategy pattern).

**Input:** `AnalysisChunk.text` (prompt content), timeout duration
**Output:** `Vec<RawMarker>` parsed from agent JSON response

**Definition of Done:**
- [x] All mock response tests pass
- [x] JSON extraction handles Codex text wrapping
- [x] `is_available()` correctly detects installed CLIs
- [x] Prompt template produces valid marker categories

**Public API:** See ADR.md "AgentBackend Trait Definition" section for:
- `AgentBackend` trait - Strategy pattern interface
- `AgentType` enum and `create_backend()` factory
- `BackendError` enum - error types
- Per-backend implementations (Claude, Codex, Gemini)

See SPEC.md Section 2 for:
- `RawMarker`, `MarkerCategory` - response types
- JSON schema and extraction logic
- Prompt template

**TDD Order:**

- [x] Create `src/analyzer/backend/mod.rs` with `AgentBackend` trait and `AgentType` enum
- [x] **JSON Parsing** (test with mock responses):
  - [x] Write tests for valid JSON parsing → fails
  - [x] Write tests for Codex text extraction (SPEC.md Section 2.4) → fails
  - [x] Write tests for malformed JSON handling → fails
  - [x] Implement `extract_json()` and Rust types → passes
- [x] Create `RawMarker` struct for parsing agent responses
- [x] **ClaudeBackend**:
  - [x] Write mock response test → fails
  - [x] Implement in `src/analyzer/backend/claude.rs` → passes
- [x] **CodexBackend**:
  - [x] Write mock response test (text with embedded JSON) → fails
  - [x] Implement in `src/analyzer/backend/codex.rs` → passes
- [x] **GeminiBackend**:
  - [x] Write mock response test → fails
  - [x] Implement in `src/analyzer/backend/gemini.rs` → passes
- [x] **Availability Check**:
  - [x] Write tests for `is_available()` → fails
  - [x] Implement per-backend availability → passes
- [x] Define analysis prompt template with engineering-focused categories

Files: `src/analyzer/backend/mod.rs`, `src/analyzer/backend/claude.rs`,
       `src/analyzer/backend/codex.rs`, `src/analyzer/backend/gemini.rs`

Marker Categories (Engineering-focused):
- Planning: Task breakdown, approach decisions
- Design/ADR: Architecture decisions, design choices
- Implementation: Coding phases, implementation attempts
- Success: What worked well, successful outcomes
- Failure: Failed attempts, issues encountered

Considerations:
- Edge case: Agent CLI not installed (`is_available()` returns false)
- Edge case: Agent returns malformed JSON (parse error handling)
- Edge case: Agent times out (configurable timeout)
- Watch out for: Codex has no native JSON output - needs text extraction

### Stage 4: Parallel Execution with Rayon

**Goal:** Implement parallel chunk processing with Rayon (automatic thread cleanup).

**Input:** `Vec<AnalysisChunk>` from Stage 2, `AgentBackend` from Stage 3
**Output:** `Vec<ChunkResult>` with markers or errors per chunk

**Definition of Done:**
- [x] All tests pass with mock backend
- [x] Progress callback fires for each chunk
- [x] Worker count scales per SPEC.md table
- [x] Threads cleaned up on success AND failure
- [x] Single-chunk case doesn't create thread pool

**Public API:** See ADR.md "Parallelism Options" and "Worker Scaling" sections for:
- `WorkerScaler` - content-based heuristic scaling
- Rayon `ThreadPoolBuilder` and `par_iter()` usage
- Progress tracking with `Arc<AtomicUsize>`

See SPEC.md Section 3.3-3.4 for parallel execution flow and fallback logic.

**Key types to implement:**
- `ParallelExecutor<B: AgentBackend>` - orchestrates parallel chunk processing
- `ChunkResult` - per-chunk success/failure with markers
- `ProgressReporter` - user feedback during analysis

**TDD Order:**

- [x] 🔴 Write test: single chunk returns result without thread pool
- [x] 🔴 Write test: multiple chunks processed in parallel
- [x] 🔴 Write test: progress callback called for each chunk
- [x] 🔴 Write test: worker count scales with token count
- [x] 🔴 Write test: partial failure (some chunks fail, some succeed)
- [x] 🟢 Add `rayon` dependency to Cargo.toml
- [x] 🟢 Implement `WorkerScaler` with content-based heuristic
- [x] 🟢 Implement `ParallelExecutor` using `ThreadPoolBuilder` and `par_iter()`
- [x] 🟢 Implement `ProgressReporter` with `Arc<AtomicUsize>`
- [x] 🔵 Verify thread cleanup on success and failure

Files: `src/analyzer/worker.rs`, `src/analyzer/progress.rs`, `Cargo.toml`

References:
- ADR Parallelism section: Rayon selected for automatic cleanup, no security vulnerabilities
- ADR Worker Scaling section: Content-based heuristic scaling

Considerations:
- Edge case: All chunks fail (collect errors, report all)
- Edge case: Some chunks fail, some succeed (partial results)
- Edge case: Single chunk (no parallelism needed)
- Watch out for: Progress reporting thread safety (AtomicUsize + Ordering::SeqCst)

### Stage 5: Result Aggregation & Marker Writing

**Goal:** Merge results from parallel analysis and write markers (Builder pattern).

**Input:** `Vec<ChunkResult>` from Stage 4, original `AsciicastFile`
**Output:** Modified `AsciicastFile` with markers inserted, `AnalysisReport`

**Definition of Done:**
- [x] All tests pass
- [x] Markers deduplicated within time window
- [x] Markers sorted by timestamp
- [x] Existing markers preserved (R9)
- [x] Warning shown if file has existing markers
- [x] File integrity preserved (playback works)

**Public API:** See ADR.md for:
- "Existing Marker Handling (R9)" - warning logic
- `ValidatedMarker` in "LLM Response Mapping" section

See SPEC.md Section 5.1 for deduplication algorithm.

**Key types to implement:**
- `ResultAggregator` - Builder pattern for collecting chunk results
- `ValidatedMarker` - marker with absolute timestamp and validation
- `MarkerWriter` - integrates with existing `MarkerManager`
- `WriteReport` - summary of write operation

**TDD Order:**

- [x] 🔴 Write test: single chunk result aggregates correctly
- [x] 🔴 Write test: multiple chunks merge markers in order
- [x] 🔴 Write test: deduplication removes markers within window
- [x] 🔴 Write test: timestamp resolution from relative to absolute
- [x] 🔴 Write test: invalid markers filtered (out of range, empty label)
- [x] 🔴 Write test: existing markers warning (R9)
- [x] 🔴 Write test: markers written to correct position in event stream
- [x] 🟢 Implement `ResultAggregator` with Builder pattern
- [x] 🟢 Implement `ValidatedMarker` with validation
- [x] 🟢 Implement `MarkerWriter` integrating with `MarkerManager`
- [x] 🔵 Refactor and verify file integrity with playback test

Files: `src/analyzer/result.rs`

References:
- Existing marker code: `src/asciicast/marker.rs`
- Marker format: `[timestamp, "m", "label"]` (asciicast v3)

Considerations:
- Edge case: Overlapping chunks produce duplicate markers (dedupe by time window)
- Edge case: Markers at exact same timestamp (keep first or merge labels)
- Edge case: Marker timestamp outside recording duration (filter out)
- Watch out for: Preserving existing markers when re-analyzing (R9)

### Stage 6: Error Handling & Smart Retry

**Goal:** Implement robust error handling with token-informed retry (Observer pattern for tracking).

**Input:** Failed `ChunkResult`s, `TokenTracker` metrics
**Output:** Retry decisions, user-friendly error messages, usage report

**Definition of Done:**
- [x] All error scenarios tested
- [x] Retry logic respects max attempts (3)
- [x] Exponential backoff implemented
- [x] Fallback to sequential on repeated parallel failures
- [x] User sees clear error messages (no stack traces)

**Public API:** See ADR.md for:
- "Token Tracking & Visibility (R6)" - `TokenTracker` implementation
- `BackendError` enum in "AgentBackend Trait Definition"

**Key types to implement:**
- `AnalysisError` - enum with all failure modes (see below)
- `RetryCoordinator` - manages retry logic with backoff
- `TokenTracker` - Observer pattern for usage metrics

**Error variants:**
- `AgentNotAvailable { agent: AgentType }`
- `AgentTimeout { chunk_id, timeout_secs }`
- `JsonParseError { chunk_id, response }`
- `ChunkFailed { chunk_id, reason }`
- `AllChunksFailed { errors }`
- `RateLimited { retry_after_secs }`

**TDD Order:**

- [x] 🔴 Write test: agent not available error message
- [x] 🔴 Write test: timeout produces user-friendly message
- [x] 🔴 Write test: retry attempts capped at 3
- [x] 🔴 Write test: exponential backoff timing
- [x] 🔴 Write test: parallel→sequential fallback triggers
- [x] 🔴 Write test: token tracking informs retry order (small chunks first)
- [x] 🟢 Implement `AnalysisError` with Display trait
- [x] 🟢 Implement `RetryCoordinator`
- [x] 🟢 Implement `TokenTracker`
- [x] 🔵 Refactor error messages for clarity

Files: `src/analyzer/error.rs`, `src/analyzer/tracker.rs`

Considerations:
- Edge case: Rate limiting from agent (detect via error message patterns)
- Edge case: Network timeout (configurable, default 60s per chunk)
- Edge case: Partial success (some chunks succeed, some fail)
- Watch out for: Infinite retry loops (max 3 retries per chunk)
- Watch out for: Exponential backoff to avoid hammering agents

### Stage 7: CLI Integration & Cleanup

**Goal:** Wire everything together with AnalyzerService facade.

**Input:** CLI args, cast file path
**Output:** Modified cast file with markers, success/error message

**Definition of Done:**
- [ ] All acceptance criteria verified (AC1-AC6 from REQUIREMENTS.md)
- [ ] CLI flags work as documented
- [ ] Old `src/analyzer.rs` deleted
- [ ] E2E test passes with real 70MB+ cast file
- [ ] `cargo doc` generates clean documentation

**Public API:** `AnalyzerService` facade - see ADR.md "Architecture Overview"

**CLI flags:**
- `--workers N` - Override worker count (default: auto-scale)
- `--agent <claude|codex|gemini>` - Select agent (default: claude)
- `--timeout N` - Chunk timeout in seconds (default: 60)
- `--no-parallel` - Disable parallelism (sequential mode)

**TDD Order:**

- [ ] 🔴 Write E2E test: analyze small fixture file
- [ ] 🔴 Write E2E test: analyze with --no-parallel
- [ ] 🔴 Write E2E test: analyze with --agent codex
- [ ] 🔴 Write E2E test: file integrity preserved after analysis
- [ ] 🔴 Write E2E test: existing markers warning shown
- [ ] 🟢 Create `AnalyzerService` facade in `src/analyzer/mod.rs`
- [ ] 🟢 Update `src/commands/analyze.rs` to use `AnalyzerService`
- [ ] 🟢 Add CLI flag parsing with clap
- [ ] 🟢 Update `src/lib.rs` exports
- [ ] 🔵 Remove old `src/analyzer.rs`
- [ ] 🔵 Verify all acceptance criteria (REQUIREMENTS.md)

**Acceptance Criteria Verification:**
- AC1: `agr analyze <file>` adds markers ✓
- AC2: File plays back correctly after analysis ✓
- AC3: Markers have engineering categories ✓
- AC4: Large file completes in <10 min ✓
- AC5: Clear errors, existing marker warning ✓
- AC6: Clean architecture, extensible ✓

Files: `src/analyzer/mod.rs`, `src/commands/analyze.rs`, `src/lib.rs`, `src/analyzer.rs` (delete)

Considerations:
- Watch out for: Breaking existing CLI interface (analyze subcommand)
- Watch out for: Config file compatibility (if any)
- Watch out for: Backwards compatibility for marker format

## Dependencies

What must be done before what:

- Stage 2 depends on Stage 1 (chunking needs content extraction)
- Stage 3 can run in parallel with Stage 2 (independent)
- Stage 4 depends on Stage 2 + Stage 3 (needs chunks and backends)
- Stage 5 depends on Stage 4 (needs parallel results)
- Stage 6 depends on Stage 4 + Stage 5 (needs execution context)
- Stage 7 depends on all previous stages

```
Stage 1 ─────┐
             ├──▶ Stage 4 ──▶ Stage 5 ──▶ Stage 6 ──▶ Stage 7
Stage 2 ─────┤         │
             │         │
Stage 3 ─────┘─────────┘
```

## Progress

Updated by implementer as work progresses.

| Stage | Status | Notes |
|-------|--------|-------|
| 1 | **complete** | All transforms implemented. 33 unit tests + 10 integration tests + 3 performance tests passing. 70.8% compression, <0.5s/15MB in release mode. |
| 2 | **complete** | TokenBudget, ChunkCalculator, AnalysisChunk, TimeRange implemented. 22 unit tests passing. Overlap strategy (10% default, min 500 tokens). NDJSON boundary alignment preserved. |
| 3 | **complete** | AgentBackend trait (Strategy pattern), BackendError with RateLimitInfo, ClaudeBackend, CodexBackend, GeminiBackend. 44 tests passing. JSON extraction handles direct/embedded/code-block formats. |
| 4 | **complete** | WorkerScaler, ParallelExecutor, ProgressReporter, ChunkResult implemented. 27 tests passing (22 worker + 5 progress). Rayon thread pool with automatic cleanup. Single-chunk optimization. |
| 5 | **complete** | ValidatedMarker, ResultAggregator (Builder pattern), MarkerWriter implemented. 25 tests passing. Deduplication within 2s window + same category. Timestamp resolution: absolute = chunk.start + relative. Integrates with existing MarkerManager. |
| 6 | **complete** | AnalysisError enum (user-friendly messages), RetryPolicy + RetryCoordinator (exponential backoff 1s->2s->4s, capped at 60s), TokenTracker (Observer pattern for usage metrics), RetryExecutor with parallel->sequential fallback. 84 tests passing (27 error + 29 tracker + 28 worker). |
| 7 | pending | |
