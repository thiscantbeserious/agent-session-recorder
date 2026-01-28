# ADR: Silence Removal Transform Implementation

## Status

Accepted

## Context

Long pauses in terminal recordings make playback tedious. The "Silence Removal" feature provides the first concrete `Transform` implementation for the recently refactored asciicast module.

### Requirements

- Implement `SilenceRemoval` struct using the existing `Transform` trait
- CLI integration via `agr transform --remove-silence <threshold> <file>`
- Simple interval clamping algorithm: `f(delta_t) = min(delta_t, T_limit)`

### Constraints

1. **Transform trait signature is fixed**: `fn transform(&mut self, events: &mut Vec<Event>)` - in-place mutation
2. **Memory efficiency**: Files can be 100+ MB with millions of events - must work from day 1
3. **CLI design needed**: No `transform` subcommand currently exists

### Deferred Decisions from Product Owner

1. **Default threshold value**: What's sensible when user doesn't specify?
2. **Header integration**: Should we use `Header.idle_time_limit` when present?

## Options Considered

### Option A: Simple Transform with Required Threshold

Threshold always required from CLI. Simplest implementation.

**Pros:** Explicit, simple, no magic
**Cons:** User must always specify threshold, no leverage of header's `idle_time_limit`

### Option B: Transform with Header Fallback

Transform accepts optional threshold; falls back to header's `idle_time_limit`.

**Pros:** Respects header, convenient
**Cons:** Requires trait change or complex resolution, implicit behavior

### Option C: CLI Handles Resolution, Transform Stays Simple

Transform always requires explicit threshold. CLI resolves from header/defaults before constructing transform.

**Pros:** Transform stays simple and reusable, CLI handles UX, clear separation of concerns
**Cons:** Resolution logic in CLI rather than near transform

## Decision

**Option C: CLI Handles Resolution, Transform Stays Simple**

### Algorithm

Simple interval clamping - for each event, cap the interval at the threshold:

```rust
if event.time > threshold {
    event.time = threshold;
}
```

### Default Threshold: 2.0 seconds

- Long enough to preserve natural reading pauses
- Short enough to eliminate "went to get coffee" pauses
- Industry common value between aggressive (1s) and conservative (3s)

### Threshold Resolution Priority

1. CLI argument (explicit user intent)
2. Header's `idle_time_limit` (recording author's intent)
3. Default constant (2.0 seconds)

### CLI Design

New `transform` subcommand:

```
agr transform --remove-silence [SECONDS] <FILE>
agr transform --remove-silence --output <OUT> <FILE>
```

Examples:
- `agr transform --remove-silence session.cast` - uses header or default
- `agr transform --remove-silence 1.5 session.cast` - explicit 1.5s threshold

### Validation

- Reject threshold <= 0 (would collapse all timing or be nonsensical)
- Reject NaN/Infinity (invalid values)

## Performance Requirements

Large file support is mandatory from day 1, not a follow-up.

| Metric | Requirement |
|--------|-------------|
| File size | Must handle 100+ MB files |
| Event count | Must handle 1+ million events |
| Time complexity | O(n) - linear with event count |
| Memory | O(1) extra - no file duplication in memory |
| Benchmark target | 100MB file in < 5 seconds |

### Performance Test

A benchmark test must exist that fails CI if performance regresses:

- Load 100MB synthetic file (1 million events)
- Apply silence removal transform
- Assert completion time < 5 seconds
- Assert memory delta < 10MB (no full copy)

## Test Strategy

Tests are written as behavioral scenarios describing real user situations.

### Scenario: User went to lunch during recording

**Given** a recording where the user took a 30-minute lunch break (1800 second gap)
**When** silence removal is applied with 2.0s threshold
**Then** the gap becomes 2.0 seconds
**And** total recording time drops from ~35 minutes to ~5 minutes

### Scenario: Rapid CI build output stays untouched

**Given** a CI build log with rapid output (0.001s intervals between lines)
**When** silence removal is applied with any reasonable threshold (e.g., 2.0s)
**Then** all intervals remain unchanged (all below threshold)
**And** the recording plays back at original speed

### Scenario: Mixed typing and thinking

**Given** a recording where user types a command (0.1s intervals), thinks for 8 seconds, types more
**When** silence removal is applied with 2.0s threshold
**Then** the 8-second thinking pause becomes 2.0 seconds
**And** the typing rhythm (0.1s intervals) is preserved exactly

### Scenario: Recording with header idle_time_limit

**Given** a recording file with `idle_time_limit: 1.5` in the header
**When** user runs `agr transform --remove-silence session.cast` (no explicit threshold)
**Then** the header's 1.5s value is used as threshold
**And** gaps > 1.5s are clamped to 1.5s

### Scenario: User overrides header with explicit threshold

**Given** a recording file with `idle_time_limit: 1.5` in the header
**When** user runs `agr transform --remove-silence 3.0 session.cast`
**Then** the explicit 3.0s value is used (overrides header)
**And** gaps > 3.0s are clamped to 3.0s

### Scenario: Large production recording (100MB file)

**Given** a real-world recording of a long debugging session (100MB, 1 million events)
**When** silence removal is applied
**Then** transform completes in under 5 seconds
**And** no out-of-memory errors occur
**And** output file is valid and playable

### Scenario: Recording with no long pauses

**Given** a fast-paced demo recording where all intervals are under 1 second
**When** silence removal is applied with 2.0s threshold
**Then** file content is unchanged (no intervals exceeded threshold)
**And** transform still completes quickly (doesn't skip processing)

### Scenario: Recording is modified in-place

**Given** a recording file at `session.cast`
**When** user runs `agr transform --remove-silence session.cast` (no --output flag)
**Then** the original file is modified in-place
**And** a backup is NOT created (user's responsibility)

### Scenario: User wants to preserve original

**Given** a recording file at `session.cast`
**When** user runs `agr transform --remove-silence --output fast.cast session.cast`
**Then** original `session.cast` is unchanged
**And** transformed version is written to `fast.cast`

### Scenario: Invalid threshold rejected

**Given** a user attempting to use an invalid threshold
**When** user runs `agr transform --remove-silence 0 session.cast`
**Then** command fails with clear error message
**And** file is not modified

### Scenario: Corrupt file handled gracefully

**Given** a corrupted or truncated .cast file
**When** user runs `agr transform --remove-silence session.cast`
**Then** command fails with clear error message about parse failure
**And** no partial/corrupt output is written

## File Structure

```
src/asciicast/
    silence_removal.rs        # SilenceRemoval transform
src/commands/
    transform.rs              # CLI subcommand
```

## Consequences

### What becomes easier

- Users can quickly speed up recordings with one command
- Transform is reusable programmatically
- Future transforms (spinner removal) follow the same pattern
- Header's `idle_time_limit` is respected automatically

### What becomes harder

- Nothing significant - this is additive

### Follow-ups for Later

- `--dry-run` flag to preview changes without modifying
- Statistics output (e.g., "Reduced duration from 5m to 2m")
- Additional transforms: spinner removal, time scaling

## Decision History

1. Transform trait signature is fixed from previous refactor - in-place mutation for memory efficiency
2. Chose Option C to keep transform simple while providing good CLI UX
3. Default threshold of 2.0s balances demo speed vs. readability
4. Header's `idle_time_limit` integration provides consistency with asciinema ecosystem
5. Large file handling is a day-1 requirement with performance benchmarks, not a follow-up
