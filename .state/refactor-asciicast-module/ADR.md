# ADR: Refactor Asciicast Module Structure

## Status

Accepted

## Context

The asciicast module needs reorganization to support upcoming transformation features (silence removal, spinner detection). The current structure has several issues:

1. **Mixed concerns in `mod.rs`** - The 512-line file contains type definitions, behavior methods, utility functions, and tests all in one place
2. **No transformation abstractions** - No traits exist for the Source -> Transform -> Sink pipeline needed for future features
3. **Dead code** - `terminal_preview_at()` and `styled_preview_at()` are unused (the TUI has its own streaming implementation)
4. **Missing documentation** - Public types and methods lack proper rustdoc comments

Cast files can grow to 100+ MB, containing millions of events. Any transformation design must avoid unnecessary memory copies.

### Constraints

- Public API must remain stable (`AsciicastFile`, `Event`, `EventType`, `Header`, `MarkerInfo`, `MarkerManager`)
- Streaming implementation is out of scope - keep `Vec<Event>` in-memory model
- Marker module is out of scope for this refactor

## Options Considered

### Option 1: Minimal Extraction (Selected)

Extract types to `types.rs`, add `transform.rs` with Transform trait, keep reader/writer as-is.

```
src/asciicast/
  mod.rs          (re-exports only, ~30 lines)
  types.rs        (Header, Event, EventType, AsciicastFile + docs)
  reader.rs       (unchanged structure, improved docs)
  writer.rs       (unchanged structure, improved docs)
  marker.rs       (unchanged)
  transform.rs    (Transform trait + pipeline utilities)
```

- Pros: Minimal disruption, clear separation, easy to implement incrementally
- Cons: Some methods still scattered across impl blocks in multiple files

### Option 2: Full Separation by Responsibility

Separate types into a `types/` subdirectory with header.rs, event.rs, file.rs.

- Pros: Maximum single-responsibility adherence
- Cons: More files to navigate, larger refactoring effort

### Option 3: Pipeline-First Architecture

Design around Source/Sink traits matching the research document's streaming model.

- Pros: Future-ready for streaming
- Cons: Over-engineered for current needs, streaming is explicitly out of scope

## Decision

**Option 1: Minimal Extraction** with the following specifics:

1. **Module structure**: Extract types to `types.rs`, add `transform.rs`
2. **Dead code removal**: Delete `terminal_preview_at()` and `styled_preview_at()` - they are unused and the TUI already has a better streaming implementation in `file_explorer.rs`
3. **Transform trait design**: Use in-place mutation to avoid memory copies with large files:
   ```rust
   pub trait Transform {
       fn transform(&mut self, events: &mut Vec<Event>);
   }
   ```
   - `&mut self` allows stateful transforms (e.g., tracking time offsets)
   - `&mut Vec<Event>` avoids cloning millions of events per transform in a chain
   - Simple to implement: silence removal = `events.retain(...)`, time rebase = iterate and mutate
4. **Documentation**: All public items get proper rustdoc with module-level docs explaining purpose

## Consequences

### What becomes easier

- Finding type definitions (all in `types.rs`)
- Adding new transforms (implement the trait)
- Understanding module purpose (clear documentation)
- Chaining transforms without memory explosion

### What becomes harder

- Nothing significant - this is additive restructuring

### Follow-ups to scope for later

- Streaming/iterator-based transforms (add `StreamingTransform` trait when needed)
- Performance optimizations (parallel processing, zero-copy parsing)
- Marker module audit

## Decision History

1. User selected Option 1 (Minimal Extraction) over full separation or pipeline-first approaches
2. Dead terminal preview methods confirmed unused - TUI has its own streaming implementation in `file_explorer.rs` (lines 108-174)
3. Transform trait uses `&mut Vec<Event>` for in-place mutation to handle 100+ MB files without memory explosion from cloning
4. User requested proper rustdoc documentation on all public items as part of the refactor scope
