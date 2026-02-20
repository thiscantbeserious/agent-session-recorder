# Plan: Reduce Cognitive Complexity — analyzer

References: [ADR.md](ADR.md) | [REQUIREMENTS.md](REQUIREMENTS.md)

## Status: Completed

## Stages

### Stage 1: Baseline verification
- [x] `cargo test` passes before changes
- [x] `cargo clippy` clean before changes

### Stage 2: Extract helper functions
- [x] `extract_partial_content(content, estimated_tokens, include_start, include_end) -> String` — extracted proportional character boundary estimation from `find_segments_for_range()` in chunk.rs
- [x] `build_partial_segment(...) -> Option<AnalysisSegment>` — extracted segment boundary logic from `find_segments_for_range()` in chunk.rs
- [x] `build_semantic_chars() -> HashSet<char>` — extracted semantic character set construction from `ContentCleaner::new()` in cleaner.rs
- [x] `build_strip_chars(config, semantic_chars) -> HashSet<char>` — extracted character-set construction from `ContentCleaner::new()` in cleaner.rs
- [x] `measure_excess_time(events, max_gap) -> (f64, usize)` — extracted pass-1 loop from `redistribute_time()` in extractor.rs
- [x] `build_chunk_calculator(agent, token_budget_override) -> ChunkCalculator` — extracted budget override logic from `AnalyzerService::analyze()` in service.rs
- [x] `normalize_newlines(data, max_consecutive) -> String` — extracted char-level newline normalization from `NormalizeWhitespace::transform()` in normalize.rs
- [x] `filter_empty_lines(data, last_line_was_empty) -> String` — extracted empty-line filtering from `EmptyLineFilter::transform()` in normalize.rs
- [x] `is_line_redundant(line_trimmed_end, index, last_occurrence, lines) -> bool` — extracted dedup check from `WindowedLineDeduplicator::flush_lines()` in aggressive.rs
- [x] `try_claude_wrapper(trimmed) -> Option<BackendResult<AnalysisResponse>>` — extracted Claude CLI wrapper parsing from `extract_json()` in backend/mod.rs

### Stage 3: Regression verification
- [x] `cargo test` passes after changes
- [x] `cargo clippy` clean after changes
- [x] `cargo fmt` applied

### Stage 4: Review
- [x] Pair review: PASS
- [x] Internal review: APPROVE after fix (removed unused `_quiet` parameter from `build_chunk_calculator`)

## Files Modified
- `src/analyzer/chunk.rs` — extracted `extract_partial_content()` and `build_partial_segment()` from `find_segments_for_range()`
- `src/analyzer/transforms/cleaner.rs` — extracted `build_semantic_chars()` and `build_strip_chars()` from `ContentCleaner::new()`
- `src/analyzer/extractor.rs` — extracted `measure_excess_time()` from `redistribute_time()`
- `src/analyzer/service.rs` — extracted `build_chunk_calculator()` from `analyze()`
- `src/analyzer/transforms/normalize.rs` — extracted `normalize_newlines()` and `filter_empty_lines()`
- `src/analyzer/transforms/aggressive.rs` — extracted `is_line_redundant()` from `flush_lines()`
- `src/analyzer/backend/mod.rs` — extracted `try_claude_wrapper()` from `extract_json()`

## Extracted Functions
| Original Function | Score | Extracted Helpers | New Score |
|---|---|---|---|
| `find_segments_for_range()` | 27 | `extract_partial_content()`, `build_partial_segment()` | < 15 |
| `ContentCleaner::new()` | 22 | `build_semantic_chars()`, `build_strip_chars()` | < 15 |
| `redistribute_time()` | 20 | `measure_excess_time()` | < 15 |
| `AnalyzerService::analyze()` | 18 | `build_chunk_calculator()` | < 15 |
| `NormalizeWhitespace::transform()` | 16 | `normalize_newlines()` | < 15 |
| `EmptyLineFilter::transform()` | 16 | `filter_empty_lines()` | < 15 |
| `flush_lines()` | 16 | `is_line_redundant()` | < 15 |
| `extract_json()` | 16 | `try_claude_wrapper()` | < 15 |
