# Requirements: Silence Removal Transform

## Problem Statement
Long pauses in terminal recordings make playback tedious. Users must wait through idle periods where nothing happens, reducing the utility of recordings for demos, documentation, and review.

## Desired Outcome
- First concrete Transform implementation using the new framework
- CLI integration via `agr transform --remove-silence`
- Recordings play back smoothly without excessive waiting

## Scope

### In Scope
- `SilenceRemoval` struct implementing the `Transform` trait
- Simple interval clamping algorithm (cap delays exceeding threshold)
- CLI flag `--remove-silence <threshold>` on transform subcommand
- Unit tests for the transform
- Integration with existing `TransformChain`

### Out of Scope
- Adaptive/context-aware thresholds (newline vs mid-word) - future enhancement
- Streaming mode (process without loading full file) - future enhancement
- Header `idle_time_limit` integration - defer decision to architect
- Default threshold value selection - defer to architect

## Acceptance Criteria
- [ ] `SilenceRemoval` struct exists and implements `Transform` trait
- [ ] Transform clamps event intervals exceeding the configured threshold
- [ ] `agr transform --remove-silence <seconds> <file>` works from CLI
- [ ] Intervals below threshold remain unchanged
- [ ] Unit tests cover: normal clamping, edge cases (zero threshold, negative values)
- [ ] Works correctly with `TransformChain` composition

## Constraints
- Must use existing `Transform` trait signature: `fn transform(&mut self, events: &mut Vec<Event>)`
- Must use in-place mutation (no copies) - files can be 100+ MB
- Must be stateless per the simple clamping algorithm

## Context
- Research document: `research/algorithm_for_asciicast_cutting_and_compression.md` Section 4
- Transform framework from previous cycle: `src/asciicast/transform.rs`
- Algorithm: `f(delta_t) = min(delta_t, T_limit)` - clamp intervals exceeding threshold

---
**Sign-off:** Approved by user
