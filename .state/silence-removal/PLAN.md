# Plan: Silence Removal Transform Implementation

References: ADR.md

## Open Questions

Implementation challenges to solve (architect identifies, implementer resolves):

1. How should the transform handle the first event (which typically has time=0)?
   **Resolution:** The first event is treated the same as any other event. If time=0, it stays 0 (0 is not > threshold). If the first event has a non-zero time exceeding threshold (unusual but possible), it gets clamped. This is consistent with the simple algorithm and doesn't require special-casing.

2. Should the benchmark test use a separate test binary or be an ignored test with `--ignored` flag?
   **Resolution:** Use `#[ignore]` attribute for slow tests (>1s). This keeps tests in the same file as the implementation, makes them discoverable via `cargo test`, and allows running them explicitly with `cargo test -- --ignored`. Criterion benchmarks can be added later if more rigorous statistical analysis is needed.

3. How should `--output` handle existing files - overwrite silently or require `--force`?
   **Resolution:** _To be resolved by implementer_

## Stages

### Stage 1: SilenceRemoval transform with tests

Goal: Implement the core transform and behavioral tests (write tests first)

#### Setup
- [x] Create `src/asciicast/silence_removal.rs`
- [x] Update `src/asciicast/mod.rs` to export `SilenceRemoval`

#### Behavioral Tests (from ADR scenarios)
- [x] Test: User went to lunch during recording (1800s gap -> 2.0s)
- [x] Test: Rapid CI build output stays untouched (intervals < threshold unchanged)
- [x] Test: Mixed typing and thinking (8s gap -> 2.0s, 0.1s preserved)
- [x] Test: Recording with no long pauses (content unchanged when all below threshold)

#### Edge Case Tests
- [x] Test: Single event file (still processed correctly)
- [x] Test: Empty events list (no panic, no change)
- [x] Test: Very small threshold (0.01s) - aggressive clamping works
- [x] Test: Very large threshold (1000s) - effectively no-op
- [x] Test: First event with time=0 (unchanged, it's the start)

#### Data Integrity Tests
- [x] Test: Markers preserved with correct relative timing
- [x] Test: All event types handled (Output, Input, Marker, Resize, Exit)
- [x] Test: Unicode content preserved (transform doesn't corrupt data field)
- [x] Test: Event order unchanged
- [x] Test: Event count unchanged

#### Composition Tests
- [x] Test: Works with TransformChain (multiple transforms in sequence)
- [x] Test: Can chain two SilenceRemoval transforms (stricter second pass)

#### Validation Tests
- [x] Test: Reject threshold <= 0 (panic with clear message)
- [x] Test: Reject NaN threshold
- [x] Test: Reject Infinity threshold

#### Implementation
- [x] Implement `SilenceRemoval` struct with threshold field
- [x] Implement `new()` with validation
- [x] Implement `Transform` trait for `SilenceRemoval`

Files: `src/asciicast/silence_removal.rs`, `src/asciicast/mod.rs`

Considerations:
- Event.time is the interval since previous event (delta time)
- Algorithm: `if event.time > threshold { event.time = threshold; }`
- Validation happens at construction time, not transform time

### Stage 2: Performance benchmark

Goal: Create performance test proving 100MB file handles in < 5 seconds

#### Test Infrastructure
- [x] Create helper function to generate synthetic events (configurable count)
- [x] Create helper to generate 100MB equivalent (1 million events)
- [x] Create helper to measure transform execution time

#### Performance Tests
- [x] Test: 1 million events transforms in < 5 seconds
- [x] Test: 10 million events transforms in < 50 seconds (linear scaling)
- [x] Test: Memory usage stays bounded (no OOM, < 10MB delta)
- [x] Test: No event vector cloning (verify in-place mutation)

#### Complexity Verification
- [x] Verify O(n) time: double input = double time (within margin)
- [x] Verify O(1) space: double input ≠ double memory

#### Scalability Edge Cases
- [x] Test: File with 1 event (no overhead issues)
- [x] Test: File with 100 events (small file fast path)
- [x] Test: File with 10,000 events (medium file)

Files: `src/asciicast/silence_removal.rs` (tests module), `benches/` if using criterion

Considerations:
- Use `#[ignore]` attribute if test is too slow for regular CI
- Synthetic file can use simple repeating output events
- Memory assertion: compare heap before/after with reasonable delta
- Consider criterion for proper benchmarking with statistical analysis

### Stage 3: CLI transform subcommand

Goal: Add `agr transform --remove-silence` command with threshold resolution

- [x] Create `src/commands/transform.rs`
- [x] Add `transform` subcommand to CLI
- [x] Implement threshold resolution: CLI arg > header idle_time_limit > 2.0s default
- [x] Implement `--output <FILE>` flag for separate output
- [x] Implement in-place modification (default, no --output)
- [x] Write test: Header idle_time_limit used when no CLI threshold
- [x] Write test: CLI threshold overrides header
- [x] Write test: Invalid threshold rejected with clear error

Files: `src/commands/transform.rs`, `src/commands/mod.rs`, `src/main.rs`

Considerations:
- Parse file header before constructing transform
- Validate threshold before any file modification
- Clear error messages for user feedback

### Stage 4: Integration tests

Goal: End-to-end CLI tests and error handling

#### File Operation Tests
- [x] Test: In-place modification works (default behavior)
- [x] Test: --output preserves original file unchanged
- [x] Test: --output creates new file with transformed content
- [x] Test: --output to same path as input (should work or error clearly)

#### Error Handling Tests
- [x] Test: Corrupt JSON file - clear error, no partial output
- [x] Test: Truncated file - clear error, file not modified
- [x] Test: Missing header - clear error
- [x] Test: File not found - clear error
- [x] Test: Permission denied (read-only file) - clear error
- [x] Test: Invalid threshold from CLI - clear error before any file ops

#### Round-Trip Tests
- [x] Test: Transform then parse again - valid asciicast format
- [x] Test: Transform preserves header fields (width, height, etc.)
- [x] Test: Transform preserves all event data fields
- [x] Test: Multiple transforms on same file - cumulative effect

#### Large File CLI Tests
- [x] Test: 100MB file via CLI completes successfully
- [x] Test: Output file is valid and playable

Files: `tests/integration/transform_test.rs`

Considerations:
- Use temp files for testing file operations
- Ensure no partial writes on error
- Verify output is valid asciicast format
- Test both success and failure paths

### Stage 5: Documentation

Goal: Update user-facing documentation

- [x] Add rustdoc to `SilenceRemoval` struct with examples
- [x] Update README with transform command usage
- [x] Add `--help` examples for transform subcommand
- [x] Document threshold resolution logic in --help

Files: `src/asciicast/silence_removal.rs`, `README.md`, `src/commands/transform.rs`

Considerations:
- Keep examples practical and copy-pasteable
- Document the threshold priority clearly

## Dependencies

```
Stage 1 ─────────────────────────┬──> Stage 3 ──> Stage 4
                                 │
Stage 2 (can run parallel) ──────┘

Stage 4 ──> Stage 5

```

- Stage 1 must complete before Stage 3 (CLI needs transform)
- Stage 2 can run in parallel with Stage 1 (independent benchmarking)
- Stage 3 must complete before Stage 4 (integration tests need CLI)
- Stage 4 must complete before Stage 5 (documentation needs final API)

## Progress

Updated by implementer as work progresses.

| Stage | Status | Notes |
|-------|--------|-------|
| 1 | completed | 26 tests pass, clippy clean |
| 2 | completed | 35 total tests (4 ignored slow perf), 1M events in 0.003s, 10M in 0.037s, O(n) time verified |
| 3 | completed | CLI with --remove-silence[=SECONDS], --output, threshold resolution (CLI > header > default), 8 unit tests |
| 4 | completed | 23 integration tests (21 fast + 2 slow/ignored), covers file ops, error handling, round-trips, large files |
| 5 | completed | Rustdoc verified (8 doc tests pass), README updated with transform examples, --help text comprehensive |
