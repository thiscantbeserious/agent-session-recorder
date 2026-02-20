# Sub-ADR: analyzer -- 9 Violations

Parent: [ADR.md](ADR.md)

## Scope

Files:
- `src/analyzer/chunk.rs` -- 1 violation (score 27)
- `src/analyzer/extractor.rs` -- 1 violation (score 20)
- `src/analyzer/service.rs` -- 1 violation (score 18)
- `src/analyzer/transforms/cleaner.rs` -- 1 violation (score 22)
- `src/analyzer/transforms/normalize.rs` -- 2 violations (scores 16, 16)
- `src/analyzer/transforms/aggressive.rs` -- 1 violation (score 16)
- `src/analyzer/backend/mod.rs` -- 1 violation (score 16)

Violations: 9 functions, combined score 167

## SonarCloud-to-Source Mapping (verified)

| SonarCloud Name | SonarCloud Line | Actual Function | Actual Line | Score |
|-----------------|-----------------|-----------------|-------------|-------|
| `create_chunks()` | 285 | `ChunkCalculator::find_segments_for_range()` | 285 | 27 |
| `clean()` | 53 | `ContentCleaner::handle_escape_char()` or `clean()` | 53/125/149 | 22 |
| `extract()` | 176 | `ContentExtractor::redistribute_time()` | 176 | 20 |
| `analyze()` | 378 | `AnalyzerService::analyze()` | 378 | 18 |
| `normalize()` (line 31) | 31 | `NormalizeWhitespace::transform()` | 31 | 16 |
| `normalize()` (line 115) | 115 | `EmptyLineFilter::transform()` | 115 | 16 |
| `transform()` | 579 | `WindowedLineDeduplicator::flush_lines()` | 579 | 16 |
| `process()` | 338 | `extract_json()` | 338 | 16 |

Note: The SonarCloud entry at line 53 for `clean()` may actually flag `ContentCleaner::new()` (lines 53-122) which has a long constructor with multiple `if config.strip_*` branches and several `for` loops over character sets. The implementer must verify by checking the SonarCloud dashboard. If `new()` is the source, the extraction targets differ (extract `build_strip_chars()` and `build_semantic_chars()`). If `handle_escape_char()` (line 149) is the source, the targets are as described below.

## Function Analysis

### 1. `find_segments_for_range()` at line 285 -- score 27

**Signature:**
```rust
fn find_segments_for_range(&self, content: &AnalysisContent, start_tokens: usize, end_tokens: usize)
    -> (Vec<AnalysisSegment>, TimeRange)
```

**Current structure:**
Lines 285-377. Iterates over `content.segments` building partial segments:
- `if segment_end > start_tokens && segment_start < end_tokens` (line 301) -- overlap check
  - `if include_end > include_start` (line 306) -- valid slice check
    - Time calculation (lines 308-315)
    - `if start_time.is_none()` (line 317) -- first-segment start time
    - **Partial content extraction** (lines 326-355):
      - `if include_start == 0 && include_end == segment.estimated_tokens` (line 326) -- full segment
      - `else if segment.content.is_empty()` (line 330) -- empty content
      - `else { ... }` (lines 332-355) -- proportional character boundary estimation with:
        - `if char_end <= char_start && included_tokens > 0` (line 342) -- fallback for failed calculation
    - Push segment (lines 357-363)
- `if accumulated_tokens >= end_tokens` (lines 370-372) -- early exit

**Why complexity is high:** Three levels of nesting from the overlap/valid-slice/content-extraction conditionals, plus the 3-way content extraction branch with its fallback.

**Borrow checker constraint:** `&self` method. Extracted helpers can be methods or free functions.

**Extraction targets:**

1. `extract_partial_content(content: &str, estimated_tokens: usize, include_start: usize, include_end: usize) -> String` -- free function or method. Covers lines 326-355. Takes segment content and token range, returns the partial content string. Handles: full inclusion, empty content, proportional character mapping with fallback. This is the deepest nesting block.

### 2. `handle_escape_char()` at line 149 (or `ContentCleaner::new()` at line 53) -- score 22

**Signature (handle_escape_char):**
```rust
fn handle_escape_char(&mut self, c: char)
```

**Current structure of `handle_escape_char()` (lines 149-193):**
A `match self.ansi_state` with 5 arms:
- `Escape` (lines 151-162): nested `match c` with 3 arms (`[`, `]`, alphabetic/charset, default)
- `Csi | CsiParams` (lines 164-174): 3 conditions for parameter bytes, final bytes, and unexpected bytes
- `Osc` (lines 176-180): `match c` with 3 arms
- `OscEscape` (lines 181-189): `if c == '\\'` with else
- `Normal` (line 190-192): unreachable

**Current structure of `ContentCleaner::new()` (lines 53-122):**
Three `if config.strip_*` blocks (lines 71-112), each containing `for` loops over character arrays with `if !semantic_chars.contains(&c)` guards. The semantic chars block (lines 58-68) is a simple `for` loop.

**Ambiguity:** SonarCloud reports line 53 with name `clean()`. This could be:
- `ContentCleaner::new()` at line 53 (constructor) -- the `if config.strip_box_drawing` with nested `for` loops and `if !semantic_chars.contains()` guards contribute significant complexity
- `clean()` at line 125 -- but `clean()` is only 15 lines with a simple `for` loop and `if/else`
- `handle_escape_char()` at line 149 -- the `match` state machine

The implementer must check SonarCloud to determine which function is flagged.

**Extraction targets (if `new()` is flagged):**

1. `build_strip_chars(config: &ExtractionConfig, semantic_chars: &HashSet<char>) -> HashSet<char>` -- free function. Covers lines 70-112. Builds the `strip_chars` set from config flags.

**Extraction targets (if `handle_escape_char()` is flagged):**

1. `handle_csi_char(&mut self, c: char)` -- method. Covers lines 164-174. Handles CSI/CsiParams state transitions.
2. `handle_osc_char(&mut self, c: char)` -- method. Covers lines 176-189. Handles Osc and OscEscape state transitions.

### 3. `redistribute_time()` at line 176 -- score 20

**Signature:**
```rust
fn redistribute_time(events: &mut [Event], max_gap: f64)
```

**Current structure:**
Lines 176-218. Two-pass algorithm:
- **Pass 1** (lines 181-189): `for event in events.iter()` with `if event.is_output()` and nested `if event.time > max_gap` / `else` counting.
- Early return on zero excess (lines 191-193)
- Bonus calculation (lines 195-199): `if normal_output_count > 0` / `else`
- **Pass 2** (lines 202-210): `for event in events.iter_mut()` with `if event.is_output()` and nested `if event.time > max_gap` / `else`.
- Fallback (lines 213-217): `if normal_output_count == 0` with `if let Some(last) = events.last_mut()`.

**Why complexity is high:** Two loops each with 2 levels of nesting, plus the early return and fallback branches.

**Extraction targets:**

1. `measure_excess_time(events: &[Event], max_gap: f64) -> (f64, usize)` -- free function. Covers pass 1 (lines 180-189). Returns `(excess, normal_output_count)`. Pure computation.

This single extraction should reduce the score sufficiently. If not, the second pass can also be extracted as `apply_time_redistribution(events: &mut [Event], max_gap: f64, bonus: f64)`.

### 4. `AnalyzerService::analyze()` at line 378 -- score 18

**Signature:**
```rust
pub fn analyze<P: AsRef<Path>>(&self, path: P) -> Result<AnalysisResult, AnalysisError>
```

**Current structure:**
Lines 378-522. A long sequential pipeline:
1. Parse cast file (lines 381-385)
2. Check existing markers (lines 388-395)
3. Extract content (lines 398-401)
4. Print stats and handle debug output (lines 403-418)
5. Check for empty content (lines 420-422)
6. **Chunk calculator setup** (lines 424-440): `if let Some(budget_tokens) = self.options.token_budget_override` with nested `if budget_tokens < 10000` (warning and fallback) / `else` (override budget). The `else` arm at line 438-440 is the default case.
7. Execute analysis with retry (lines 443-488)
8. Aggregate, write markers, report summary (lines 490-521)

PR #141 already extracted `print_extraction_stats()`, `handle_debug_output()`, and `report_analysis_summary()`.

**Why complexity is high:** The chunk calculator setup (step 6) has 3 branches. The sequential pipeline with multiple early returns and conditional blocks accumulates incremental complexity.

**Extraction targets:**

1. `build_chunk_calculator(agent: AgentType, token_budget_override: Option<usize>, quiet: bool) -> ChunkCalculator` -- method or free function. Covers lines 424-440. Handles the budget override logic with the minimum check and warning.

### 5. `NormalizeWhitespace::transform()` at line 31 -- score 16

**Signature:**
```rust
fn transform(&mut self, events: &mut Vec<Event>)   // impl Transform
```

**Current structure:**
Lines 31-51. For each event: `if event.is_output()` then iterate chars with `if c == '\n'` (increment counter, `if count <= max`) / `else` (reset counter). Three nesting levels: loop > is_output > char iteration > newline check.

**Extraction targets:**

1. `normalize_newlines(data: &str, max_consecutive: usize) -> String` -- free function. Covers lines 34-48. Takes the event data string and max_consecutive parameter, returns the normalized string. The `transform()` method becomes a loop calling this helper.

### 6. `EmptyLineFilter::transform()` at line 115 -- score 16

**Signature:**
```rust
fn transform(&mut self, events: &mut Vec<Event>)   // impl Transform
```

**Current structure:**
Lines 115-158. For each event: `if !event.is_output()` (keep, add accumulated time) / else process with `split_inclusive('\n')` loop checking `is_empty && self.last_line_was_empty` / then `if !new_data.is_empty()` (keep) / else (accumulate time). Final `if accumulated_time > 0.0` with `if let Some(last)`.

**Extraction targets:**

1. `filter_empty_lines(data: &str, last_line_was_empty: &mut bool) -> String` -- free function. Covers lines 127-139. Takes event data and mutable state ref, returns filtered string. The `transform()` method handles the event-level logic (keep non-output, check empty result, accumulate time).

### 7. `WindowedLineDeduplicator::flush_lines()` at line 579 -- score 16

**Signature:**
```rust
fn flush_lines(&mut self, output: &mut Vec<Event>)
```

**Current structure:**
Lines 579-638. Drains line buffer, builds `last_occurrence` HashMap, then iterates lines with:
- `if trimmed.is_empty()` (continue, accumulate)
- `is_repeated` check via HashMap (lines 610-613)
- `is_prefix` check via linear scan, gated by `if !is_repeated` (lines 616-624)
- `if !is_prefix && !is_repeated` (keep line) / `else` (dedup, accumulate time)

**Extraction targets:**

1. `is_line_redundant(line_trimmed_end: &str, index: usize, last_occurrence: &HashMap<&str, usize>, lines: &[(String, f64)]) -> bool` -- free function. Covers lines 609-624. Checks if the line at the given index is either an exact repeat (via HashMap) or a prefix of a later line (via scan). Returns `true` if redundant.

### 8. `extract_json()` at line 338 -- score 16

**Signature:**
```rust
pub fn extract_json(response: &str) -> BackendResult<AnalysisResponse>
```

**Current structure:**
Lines 338-372. Tries Claude CLI wrapper format first:
- `if let Ok(wrapper) = serde_json::from_str::<ClaudeWrapper>(trimmed)` (line 344)
  - `if wrapper.response_type.as_deref() == Some("result")` (line 345)
    - `if wrapper.is_error == Some(true)` (lines 347-353) -- return error
    - `if let Some(structured) = wrapper.structured_output` (lines 356-358) -- parse structured output
    - `if let Some(inner) = wrapper.result` (line 362) with `if !inner.is_empty()` (line 363) -- fall back to result field
- Falls back to `extract_json_inner(trimmed)` (line 371)

**Why complexity is high:** Three levels of nesting from the wrapper parse + type check + inner field checks.

**Extraction targets:**

1. `try_claude_wrapper(trimmed: &str) -> Option<BackendResult<AnalysisResponse>>` -- free function. Covers lines 344-368. Attempts to parse as `ClaudeWrapper` and extract the response. Returns `Some(result)` if the response is a Claude wrapper, `None` to fall through to standard extraction. This eliminates all the nesting from `extract_json()` which becomes:
   ```rust
   if let Some(result) = try_claude_wrapper(trimmed) { return result; }
   extract_json_inner(trimmed)
   ```

## Dependencies Between Functions

All 9 functions are in **different files** within the `src/analyzer/` directory. There are no cross-file dependencies that would prevent parallel implementation. Specifically:
- `find_segments_for_range()` is in `chunk.rs`
- `redistribute_time()` is in `extractor.rs`
- `analyze()` is in `service.rs`
- `handle_escape_char()` / `ContentCleaner::new()` is in `transforms/cleaner.rs`
- `NormalizeWhitespace::transform()` and `EmptyLineFilter::transform()` are in `transforms/normalize.rs` (same file, but independent functions)
- `flush_lines()` is in `transforms/aggressive.rs`
- `extract_json()` is in `backend/mod.rs`

All sub-stages can run in parallel.

## Testability Assessment

**Well-tested functions:**
- `find_segments_for_range()`: Tested via `cargo test chunk` -- chunk calculation tests exercise this.
- `redistribute_time()`: Tested via `cargo test extractor` and `cargo test analyzer_content_test`.
- `NormalizeWhitespace::transform()`: Tested in `normalize.rs` (lines 161-197): `preserves_spaces_and_tabs`, `limits_consecutive_newlines`, `preserves_tabs`.
- `EmptyLineFilter::transform()`: Tested in `normalize.rs` (lines 200-303): multiple tests for empty line filtering, time accumulation.
- `flush_lines()`: Tested via `cargo test aggressive_transform_test`.
- `extract_json()`: Tested via `cargo test backend`.

**Less tested:**
- `ContentCleaner::new()` / `handle_escape_char()`: Tested via `cargo test content_cleaner_test` and `cargo test analyzer_content_test`. The cleaner has good test coverage for ANSI stripping behavior.
- `analyze()`: Tested via `cargo test service` and `cargo test analyzer_service_test`. Integration-level tests.

**TDD approach:** All analyzer functions have good test coverage. Run module-specific tests (`cargo test chunk`, `cargo test extractor`, etc.) before and after each extraction. The extracted helpers (like `extract_partial_content()`, `measure_excess_time()`, `normalize_newlines()`) are all pure functions that could be independently tested, but writing new tests is out of scope.
