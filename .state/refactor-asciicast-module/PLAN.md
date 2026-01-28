# Plan: Refactor Asciicast Module Structure

References: ADR.md

## Open Questions

Implementation challenges to solve (architect identifies, implementer resolves):

1. Should `cumulative_times()` and related helper methods stay on `AsciicastFile` or move to a utility module?
   **Resolution:** Keep on `AsciicastFile` - it's behavior that belongs with the type.

2. How should the Transform trait handle errors - return `Result<()>` or panic on invalid state?
   **Resolution:** Keep infallible (`()` return) - transforms that can fail should use `Option` filtering or handle errors internally.

## Stages

### Stage 1: Create types.rs with documentation

Goal: Extract all type definitions from mod.rs into a dedicated types.rs with proper rustdoc

- [x] Create `src/asciicast/types.rs`
- [x] Move `Header`, `TermInfo`, `EnvInfo` structs with rustdoc
- [x] Move `EventType` enum with rustdoc on each variant
- [x] Move `Event` struct with rustdoc
- [x] Move `AsciicastFile` struct with rustdoc
- [x] Add module-level documentation explaining the asciicast v3 format
- [x] Update mod.rs to re-export from types.rs

Files: `src/asciicast/types.rs`, `src/asciicast/mod.rs`

Considerations:
- Keep `impl` blocks for constructors and type checks with the types
- Parsing (`from_json`) stays in reader.rs, serialization (`to_json`) stays in writer.rs
- Ensure all existing tests still compile with updated imports

### Stage 2: Delete dead terminal preview code

Goal: Remove unused terminal preview methods and their tests

- [x] Delete `terminal_preview_at()` method from `AsciicastFile`
- [x] Delete `styled_preview_at()` method from `AsciicastFile`
- [x] Delete `terminal_preview_at_zero_is_empty` test
- [x] Delete `terminal_preview_at_end_has_all_output` test
- [x] Remove `use crate::terminal_buffer::*` import if no longer needed

Files: `src/asciicast/types.rs` (after Stage 1 moves code there)

Considerations:
- Verify no external code depends on these methods (already confirmed: only internal tests)
- The TUI's `SessionPreview::load_streaming()` in `file_explorer.rs` serves this purpose better

### Stage 3: Create transform.rs with Transform trait

Goal: Add the transformation abstraction for future silence removal and spinner detection

- [x] Create `src/asciicast/transform.rs`
- [x] Define `Transform` trait with `fn transform(&mut self, events: &mut Vec<Event>)`
- [x] Add `TransformChain` struct to compose multiple transforms
- [x] Add comprehensive rustdoc with examples
- [x] Update mod.rs to re-export `Transform` and `TransformChain`

Files: `src/asciicast/transform.rs`, `src/asciicast/mod.rs`

Considerations:
- Keep it simple - no streaming yet
- `TransformChain` should apply transforms in order, mutating the same Vec
- Example transforms in doc comments (e.g., a simple "remove markers" transform)

### Stage 4: Add documentation to reader.rs and writer.rs

Goal: Improve rustdoc on existing reader and writer modules

- [x] Add module-level doc to reader.rs explaining parsing behavior
- [x] Document `Event::from_json()` with error conditions
- [x] Document `AsciicastFile::parse()`, `parse_reader()`, `parse_str()`
- [x] Add module-level doc to writer.rs explaining serialization
- [x] Document `Event::to_json()`
- [x] Document `AsciicastFile::write()`, `write_to()`, `to_string()`

Files: `src/asciicast/reader.rs`, `src/asciicast/writer.rs`

Considerations:
- Focus on behavior, error conditions, and usage examples
- Keep docs concise but complete

### Stage 5: Clean up mod.rs and verify public API

Goal: Reduce mod.rs to re-exports only and verify API stability

- [x] Remove all type definitions from mod.rs (moved in Stage 1)
- [x] Remove tests from mod.rs (moved with types in Stage 1)
- [x] Ensure mod.rs only contains module declarations and re-exports
- [x] Update module-level documentation in mod.rs
- [x] Verify `lib.rs` exports still work: `AsciicastFile`, `Event`, `EventType`, `Header`, `MarkerInfo`, `MarkerManager`
- [x] Run full test suite to confirm no regressions

Files: `src/asciicast/mod.rs`, `src/lib.rs`

Considerations:
- The marker module re-exports should remain unchanged
- All imports in other modules (player, tui) should still work

## Dependencies

- Stage 2 depends on Stage 1 (types must be in types.rs before deleting preview methods)
- Stage 3 can run in parallel with Stage 2 (independent)
- Stage 4 can run in parallel with Stage 2 and 3 (independent)
- Stage 5 depends on Stage 1, 2, 3, 4 (final cleanup after all changes)

## Progress

Updated by implementer as work progresses.

| Stage | Status | Notes |
|-------|--------|-------|
| 1 | completed | Types extracted to types.rs with full rustdoc |
| 2 | completed | Dead terminal preview code removed during extraction |
| 3 | completed | Transform trait and TransformChain implemented |
| 4 | completed | reader.rs and writer.rs fully documented |
| 5 | completed | mod.rs cleaned up, all tests pass |
