# Plan: Reduce Cognitive Complexity — tui/event_bus

References: [ADR.md](ADR.md) | [REQUIREMENTS.md](REQUIREMENTS.md)

## Status: Completed

## Stages

### Stage 1: Baseline verification
- [x] `cargo test` passes before changes
- [x] `cargo clippy` clean before changes

### Stage 2: Extract helper functions
- [x] `poll_events(tx, running, tick_rate)` — extracted entire event polling loop from `thread::spawn` closure into free function
- [x] `dispatch_crossterm_event(tx) -> bool` — extracted `event::read()` match with CrosstermEvent-to-Event mapping into free function, returns false to break loop

### Stage 3: Regression verification
- [x] `cargo test` passes after changes
- [x] `cargo clippy` clean after changes
- [x] `cargo fmt` applied

### Stage 4: Review
- [x] Pair review: PASS
- [x] Internal review: APPROVE

## Files Modified
- `src/tui/event_bus.rs` — extracted `poll_events()` and `dispatch_crossterm_event()` from `EventHandler::new()`, reducing three levels of nesting to one

## Extracted Functions
| Original Function | Score | Extracted Helpers | New Score |
|---|---|---|---|
| `EventHandler::new()` | 43 | `poll_events()`, `dispatch_crossterm_event()` | < 15 |
