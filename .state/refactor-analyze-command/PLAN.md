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

Goal: Implement content extraction using the existing Transform trait pattern.

**TDD Order** (tests before implementation):

- [ ] Create `src/analyzer/mod.rs` with module structure
- [ ] Create `tests/fixtures/` with sample events from real cast files (Section 1.7 of SPEC.md)

**ContentCleaner (Optimized Single-Pass - see ADR Performance section):**
- [ ] 🔴 Write unit tests for ANSI stripping (CSI, OSC, simple escapes)
- [ ] 🔴 Write unit tests for control char stripping
- [ ] 🔴 Write unit tests for box drawing removal
- [ ] 🔴 Write unit tests for spinner removal (Claude, Gemini, Codex)
- [ ] 🔴 Write unit tests for **semantic char preservation** (✓✔✕⚠ℹ☐☑)
- [ ] 🔴 Write unit tests for progress block removal
- [ ] 🟢 Implement `ContentCleaner` with state machine
- [ ] 🔵 Refactor for clarity, verify 5x+ faster than naive approach

**DeduplicateProgressLines** (separate transform, runs after ContentCleaner):
- [ ] 🔴 Write snapshot test with \r-based progress
- [ ] 🔴 Write test for timestamp preservation
- [ ] 🔴 Write test for marker preservation
- [ ] 🟢 Implement transform (see ADR algorithm)
- [ ] 🔵 Refactor if needed

**NormalizeWhitespace:**
- [ ] 🔴 Write unit tests
- [ ] 🟢 Implement transform
- [ ] 🔵 Refactor if needed

**FilterEmptyEvents:**
- [ ] 🔴 Write unit tests (preserves markers)
- [ ] 🟢 Implement transform
- [ ] 🔵 Refactor if needed

**TokenEstimator:**
- [ ] 🔴 Write unit test for chars/4 estimation
- [ ] 🔴 Write test for estimation AFTER cleanup
- [ ] 🟢 Implement `TokenEstimator` struct
- [ ] 🔵 Refactor if needed

**StatsCollector:**
- [ ] 🔴 Write unit test for stats accumulation
- [ ] 🟢 Implement `StatsCollector`
- [ ] 🔵 Refactor if needed

**Full Pipeline:**
- [ ] 🔴 Write integration snapshot test
- [ ] 🟢 Create `ExtractionConfig` with `build_pipeline()` method
- [ ] 🟢 Create `ContentExtractor` with segment creation
- [ ] 🔵 **Benchmark with real 70MB+ cast file** - target <5s extraction time

- [ ] Create `AnalysisSegment` struct with start_time, end_time, content, estimated_tokens
- [ ] Verify compression ratios match SPEC.md expectations (55-89% reduction)

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

Goal: Implement dynamic chunking based on agent token limits.

**TDD Order**:

- [ ] **TokenBudget**:
  - [ ] Write unit tests for budget calculation → fails
  - [ ] Create `TokenBudget` struct with agent-specific limits → passes
- [ ] **Token Estimation**:
  - [ ] Write property test (chars/4 ≈ tokens) → fails
  - [ ] Implement estimation with safety margin → passes
- [ ] **Chunk Calculation**:
  - [ ] Write test: single chunk when content < budget → fails
  - [ ] Write test: multi-chunk when content > budget → fails
  - [ ] Write test: verify chunks match SPEC.md scaling table → fails
  - [ ] Implement `ChunkCalculator` → passes
- [ ] Create `AnalysisChunk` struct with id, time_range, segments, text, estimated_tokens
- [ ] Create `TimeRange` struct for chunk boundaries
- [ ] **Overlap Logic**:
  - [ ] Write test for overlap percentage → fails
  - [ ] Implement `AnalysisChunk::from_content()` with overlap → passes
- [ ] **Timestamp Resolution**:
  - [ ] Write property test: absolute timestamp always in valid range → fails
  - [ ] Implement `resolve_marker_timestamp()` → passes

Files: `src/analyzer/chunk.rs`

Considerations:
- Edge case: Content smaller than one chunk (single chunk, no splitting)
- Edge case: Very dense content that requires many chunks
- Edge case: Segment boundaries vs chunk boundaries
- Watch out for: Chunk boundaries cutting mid-sentence (prefer segment boundaries)

### Stage 3: Agent Backend Abstraction

Goal: Create extensible agent backend trait with implementations (Strategy pattern).

**TDD Order**:

- [ ] Create `src/analyzer/backend/mod.rs` with `AgentBackend` trait and `AgentType` enum
- [ ] **JSON Parsing** (test with mock responses):
  - [ ] Write tests for valid JSON parsing → fails
  - [ ] Write tests for Codex text extraction (SPEC.md Section 2.4) → fails
  - [ ] Write tests for malformed JSON handling → fails
  - [ ] Implement `extract_json()` and Rust types → passes
- [ ] Create `RawMarker` struct for parsing agent responses
- [ ] **ClaudeBackend**:
  - [ ] Write mock response test → fails
  - [ ] Implement in `src/analyzer/backend/claude.rs` → passes
- [ ] **CodexBackend**:
  - [ ] Write mock response test (text with embedded JSON) → fails
  - [ ] Implement in `src/analyzer/backend/codex.rs` → passes
- [ ] **GeminiBackend**:
  - [ ] Write mock response test → fails
  - [ ] Implement in `src/analyzer/backend/gemini.rs` → passes
- [ ] **Availability Check**:
  - [ ] Write tests for `is_available()` → fails
  - [ ] Implement per-backend availability → passes
- [ ] Define analysis prompt template with engineering-focused categories

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

Goal: Implement parallel chunk processing with Rayon (automatic thread cleanup).

- [ ] Add `rayon` dependency to Cargo.toml
- [ ] Implement `WorkerScaler` with content-based heuristic scaling
  - Scale factor based on token count (0.5 for <100K, 1.0 for 100K-500K, 1.5 for >500K)
  - Respect CPU count limits
  - User override option
- [ ] Implement parallel processing using `ThreadPoolBuilder` and `par_iter()`
- [ ] Add progress tracking with `Arc<AtomicUsize>`
- [ ] Implement `ProgressReporter` for user feedback (chunk X of Y)
- [ ] Test parallelism with multiple chunks
- [ ] Verify thread cleanup on success and failure (Rayon handles this)
- [ ] Test with configurable worker counts

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

Goal: Merge results from parallel analysis and write markers (Builder pattern).

- [ ] Implement `ResultAggregator` to collect chunk results (Builder pattern)
- [ ] Implement timestamp resolution using `AnalysisChunk::resolve_marker_timestamp()`
- [ ] Implement marker deduplication (configurable time window, e.g., 0.5s)
- [ ] Implement marker sorting by timestamp
- [ ] Create `ValidatedMarker` struct with timestamp, label, category
- [ ] Implement marker validation (timestamp in range, non-empty label)
- [ ] Integrate with existing `MarkerManager` from `src/asciicast/marker.rs`
- [ ] Warn user if file already has markers (R9 - idempotency)
- [ ] Test full pipeline end-to-end

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

Goal: Implement robust error handling with token-informed retry (Observer pattern for tracking).

- [ ] Define `AnalysisError` enum with all failure modes:
  - `AgentNotAvailable { agent: AgentType }`
  - `AgentTimeout { chunk_id: usize, timeout_secs: u64 }`
  - `JsonParseError { chunk_id: usize, response: String }`
  - `ChunkFailed { chunk_id: usize, reason: String }`
  - `AllChunksFailed { errors: Vec<AnalysisError> }`
  - `RateLimited { retry_after_secs: Option<u64> }`
- [ ] Implement user-friendly error messages (no stack traces)
- [ ] Implement `RetryCoordinator` with configurable retry count and backoff
- [ ] Add fallback from parallel to sequential on repeated failures
- [ ] Implement `TokenTracker` (Observer pattern) for usage metrics:
  - Track tokens per chunk, per agent
  - Track success/failure rates
  - Inform retry decisions (e.g., retry small chunks first)
- [ ] Connect retry decisions to token tracking
- [ ] Test error scenarios and retry behavior

Files: `src/analyzer/error.rs`, `src/analyzer/tracker.rs`

Considerations:
- Edge case: Rate limiting from agent (detect via error message patterns)
- Edge case: Network timeout (configurable, default 60s per chunk)
- Edge case: Partial success (some chunks succeed, some fail)
- Watch out for: Infinite retry loops (max 3 retries per chunk)
- Watch out for: Exponential backoff to avoid hammering agents

### Stage 7: CLI Integration & Cleanup

Goal: Wire everything together with AnalyzerService facade.

- [ ] Create `AnalyzerService` facade in `src/analyzer/mod.rs`
  - Simple public API: `analyze(path, options) -> Result<AnalysisResult>`
  - Hide all internal complexity
- [ ] Update `src/commands/analyze.rs` to use `AnalyzerService`
- [ ] Add CLI flags:
  - `--workers N` - Override worker count (default: auto-scale)
  - `--agent <claude|codex|gemini>` - Select agent (default: claude)
  - `--timeout N` - Chunk timeout in seconds (default: 60)
  - `--no-parallel` - Disable parallelism (sequential mode)
- [ ] Remove old `src/analyzer.rs`
- [ ] Update `src/lib.rs` exports for new analyzer module
- [ ] Run full e2e tests with real cast files
- [ ] Verify acceptance criteria:
  - AC1: Automatic marker addition works
  - AC2: File integrity preserved
  - AC3: Marker quality (engineering categories)
  - AC4: Performance (<10 min for large files)
  - AC5: User experience (clear errors, warnings)
  - AC6: Architecture (clean, extensible)

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
| 1 | pending | |
| 2 | pending | |
| 3 | pending | |
| 4 | pending | |
| 5 | pending | |
| 6 | pending | |
| 7 | pending | |
