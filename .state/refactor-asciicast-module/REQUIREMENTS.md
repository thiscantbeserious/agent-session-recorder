# Requirements: Refactor Asciicast Module Structure

## Problem Statement

The current asciicast module has organizational issues that will impede upcoming feature development:

1. **Mixed concerns in `mod.rs`** - The 512-line file contains type definitions, behavior methods, cumulative time calculations, terminal preview logic, and insertion helpers all in one place
2. **No transformation abstractions** - The module is structured around parsing/serialization but lacks abstractions for the transformation pipeline needed for silence removal and spinner detection
3. **Module focuses on NDJSON mechanics** - The structure emphasizes file format details rather than asciicast semantics and event stream processing

The upcoming features (silence removal, spinner detection) require a clean separation between:
- Core types and their semantics
- Parsing (Source)
- Transformation operations (Transformer Chain)
- Serialization (Sink)

## Desired Outcome

After this refactor:
1. The module has a clear, logical organization with single-responsibility submodules
2. Transformation abstractions exist that future features can implement (traits/types for operating on event streams)
3. The public API remains stable - existing consumers of `AsciicastFile`, `Header`, `Event`, etc. continue to work
4. The architecture is prepared for a future streaming implementation without requiring it now

## Scope

### In Scope
- Reorganize `mod.rs` into focused submodules (separate types from behavior)
- Define transformation abstractions (traits/types) that silence removal and spinner detection will implement
- Keep `AsciicastFile` with `Vec<Event>` functional (no breaking changes to existing consumers)
- Ensure existing tests pass after reorganization
- Update module exports to maintain public API compatibility

### Out of Scope
- Implementing silence removal or spinner detection algorithms
- Converting to streaming/iterator-based processing (future task)
- Fixing or auditing the marker module (separate task)
- Performance optimizations
- Adding new functionality beyond structural reorganization

## Acceptance Criteria

- [ ] `mod.rs` is reduced to module exports and re-exports only (no substantial logic)
- [ ] Types (`Header`, `Event`, `EventType`, `AsciicastFile`) are in a dedicated submodule
- [ ] Transformation abstractions exist (e.g., a `Transform` trait or similar) that define how to process event streams
- [ ] Reader (parsing) and Writer (serialization) remain separate and focused
- [ ] All existing tests pass without modification (or with minimal import path updates)
- [ ] Public API surface remains compatible - code using `asciicast::AsciicastFile`, `asciicast::Event`, etc. continues to work
- [ ] Code compiles with no new warnings

## Constraints

- Must not break existing CLI commands that use the asciicast module
- Keep the `Vec<Event>` in-memory model working (streaming is a future concern)
- Transformation abstractions should be designed with the research document's pipeline model in mind (Source -> Transform -> Sink)

## Context

- Research document at `research/algorithm_for_asciicast_cutting_and_compression.md` outlines the target architecture
- Future work will add silence removal (interval clamping) and spinner detection (FSM-based pattern recognition)
- The transformation traits defined here will be implemented by those future features
- A new CLI command for processing cast files will consume these transformations (also future work)

---
**Sign-off:** Approved by user
