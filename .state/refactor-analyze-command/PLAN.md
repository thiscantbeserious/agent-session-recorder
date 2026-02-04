# Plan: Refactor Analyze Command

References: ADR.md

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

- [ ] Create `src/analyzer/mod.rs` with module structure
- [ ] Implement `StripAnsiCodes` transform in `src/analyzer/content.rs`
- [ ] Implement `StripControlCharacters` transform
- [ ] Implement `DeduplicateProgressLines` transform
- [ ] Implement `NormalizeWhitespace` transform
- [ ] Implement `FilterEmptyEvents` transform
- [ ] Create `ExtractionConfig` with `build_pipeline()` method
- [ ] Create `ContentExtractor` that uses the pipeline and produces `AnalysisContent`
- [ ] Create `AnalysisSegment` struct with start_time, end_time, content, estimated_tokens
- [ ] Add comprehensive tests for each transform
- [ ] Test full pipeline with real cast files of various sizes

Files: `src/analyzer/mod.rs`, `src/analyzer/content.rs`

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

- [ ] Create `TokenBudget` struct with agent-specific limits (Claude 160K, Codex 150K, Gemini 800K)
- [ ] Implement token estimation (chars/4 heuristic with safety margin)
- [ ] Implement `ChunkCalculator` that divides `AnalysisContent` into chunks
- [ ] Create `AnalysisChunk` struct with id, time_range, segments, text, estimated_tokens
- [ ] Create `TimeRange` struct for chunk boundaries
- [ ] Implement `AnalysisChunk::from_content()` to create chunks from segments
- [ ] Implement `resolve_marker_timestamp()` for mapping LLM responses back to absolute time
- [ ] Add chunk overlap logic for context continuity (configurable overlap percentage)
- [ ] Test chunk sizing with various content sizes

Files: `src/analyzer/chunk.rs`

Considerations:
- Edge case: Content smaller than one chunk (single chunk, no splitting)
- Edge case: Very dense content that requires many chunks
- Edge case: Segment boundaries vs chunk boundaries
- Watch out for: Chunk boundaries cutting mid-sentence (prefer segment boundaries)

### Stage 3: Agent Backend Abstraction

Goal: Create extensible agent backend trait with implementations (Strategy pattern).

- [ ] Create `src/analyzer/backend/mod.rs` with `AgentBackend` trait and `AgentType` enum
- [ ] Implement `ClaudeBackend` in `src/analyzer/backend/claude.rs`
  - `--print` mode, `--output-format json`, `--json-schema`
  - `--dangerously-skip-permissions` flag handling
- [ ] Implement `CodexBackend` in `src/analyzer/backend/codex.rs`
  - `exec` mode, text output (needs JSON extraction from response)
  - `--dangerously-bypass-approvals-and-sandbox` flag handling
- [ ] Implement `GeminiBackend` in `src/analyzer/backend/gemini.rs`
  - Positional prompt, `--output-format json`
  - `--yolo` flag handling
- [ ] Define analysis prompt template with engineering-focused categories
- [ ] Define JSON schema for marker responses (timestamp, label, category)
- [ ] Create `RawMarker` struct for parsing agent responses
- [ ] Create `MarkerPosition` enum (RelativeTimestamp, TextSearch)
- [ ] Add `is_available()` check for each backend
- [ ] Test each backend with mock responses

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

Goal: Merge results from parallel analysis and write markers.

- [ ] Implement `ResultAggregator` to collect chunk results
- [ ] Implement timestamp adjustment (relative to absolute)
- [ ] Implement marker deduplication (within time window)
- [ ] Implement marker sorting by timestamp
- [ ] Create `ValidatedMarker` with validation logic
- [ ] Integrate with existing `MarkerManager` for writing
- [ ] Test full pipeline end-to-end

Files: `src/analyzer/result.rs`

Considerations:
- Edge case: Overlapping chunks produce duplicate markers
- Edge case: Markers at exact same timestamp
- Watch out for: Preserving existing markers (R9)

### Stage 6: Error Handling & Smart Retry

Goal: Implement robust error handling with token-informed retry.

- [ ] Define `AnalysisError` enum with all failure modes
- [ ] Implement user-friendly error messages
- [ ] Implement `RetryCoordinator` with retry logic
- [ ] Add fallback from parallel to sequential
- [ ] Implement `TokenTracker` for usage metrics
- [ ] Connect retry decisions to token tracking
- [ ] Test error scenarios and retry behavior

Files: `src/analyzer/error.rs`, `src/analyzer/tracker.rs`

Considerations:
- Edge case: Rate limiting from agent
- Edge case: Network timeout
- Watch out for: Infinite retry loops

### Stage 7: CLI Integration & Cleanup

Goal: Wire everything together and update CLI.

- [ ] Update `src/commands/analyze.rs` to use new `AnalyzerService`
- [ ] Add CLI flags for worker count override
- [ ] Add CLI flags for agent selection
- [ ] Remove old `src/analyzer.rs`
- [ ] Update lib.rs exports
- [ ] Run full e2e tests
- [ ] Update documentation

Files: `src/commands/analyze.rs`, `src/lib.rs`, `src/analyzer.rs` (delete)

Considerations:
- Watch out for: Breaking existing CLI interface
- Watch out for: Config file compatibility

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
