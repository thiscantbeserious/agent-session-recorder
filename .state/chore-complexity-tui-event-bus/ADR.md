# Sub-ADR: tui/event_bus -- 1 Violation

Parent: [ADR.md](ADR.md)

## Scope

File: `src/tui/event_bus.rs` (182 lines)
Violations: 1 function, score 43

## SonarCloud-to-Source Mapping (verified)

| SonarCloud Name | SonarCloud Line | Actual Function | Actual Line | Score |
|-----------------|-----------------|-----------------|-------------|-------|
| `recv()` | 45 | `EventHandler::new()` | 45 | 43 |

## Function Analysis

### `EventHandler::new()` at line 45 -- score 43

**Signature:**
```rust
pub fn new(tick_rate: Duration) -> Self
```

**Current structure:**
Lines 45-114. The function:
1. Creates channel and running flag (lines 46-48)
2. Spawns a thread with `thread::spawn(move || { ... })` (lines 50-107)
3. Constructs and returns `Self` (lines 109-113)

Inside the spawned thread closure (lines 50-107):
- `while thread_running.load(Ordering::Relaxed)` loop (line 51)
  - `match event::poll(tick_rate)` -- 3 arms: `Ok(true)`, `Ok(false)`, `Err(_)` (lines 53-105)
    - `Ok(true)` arm (lines 54-93):
      - Early stop check `if !thread_running.load(...)` (lines 56-58)
      - `match event::read()` -- 6 arms (lines 60-93):
        - `Ok(CrosstermEvent::Key(key))` (lines 61-74): nested `if key.code == KeyCode::Char('c') && key.modifiers.contains(CONTROL)` for Ctrl+C, then `if tx.send(...).is_err()` for break
        - `Ok(CrosstermEvent::Resize(w, h))` (lines 76-79): `if tx.send(...).is_err()`
        - `Ok(CrosstermEvent::Mouse(mouse))` (lines 81-84): `if tx.send(...).is_err()`
        - `Ok(CrosstermEvent::Paste(text))` (lines 86-89): `if tx.send(...).is_err()`
        - `Ok(_)` (line 91): ignored
        - `Err(_)` (line 92): break
    - `Ok(false)` arm (lines 95-99): send Tick, break on error
    - `Err(_)` arm (lines 101-103): break

**Why complexity is high:** Three levels of nesting (`while` > `match poll` > `match read`), each with multiple arms and conditional breaks. The `Key` arm has an additional nesting level for the Ctrl+C check.

**Borrow checker constraint:** The entire body runs inside `thread::spawn(move || { ... })`. The closure captures `tx: mpsc::Sender<Event>`, `thread_running: Arc<AtomicBool>`, and `tick_rate: Duration` by move. Extracted helpers must be **free functions** that accept these captured values as parameters. The helper signature must be `Send + 'static` since it runs in a spawned thread -- `mpsc::Sender`, `Arc<AtomicBool>`, and `Duration` all satisfy this.

**Extraction targets:**

1. `poll_events(tx: mpsc::Sender<Event>, running: Arc<AtomicBool>, tick_rate: Duration)` -- free function. Moves the entire `while running.load(...)` loop body out of the closure. The `thread::spawn` call becomes:
   ```rust
   let handle = thread::spawn(move || poll_events(tx, thread_running, tick_rate));
   ```
   This alone eliminates one nesting level but does not drop below 15.

2. `dispatch_crossterm_event(event: CrosstermEvent, tx: &mpsc::Sender<Event>) -> bool` -- free function inside `poll_events()`. Maps a `CrosstermEvent` to an `Event` and sends it. Returns `false` to signal the loop should break (Ctrl+C or send error), `true` to continue. Covers lines 60-93 (the `match event::read()` Ok arms). The Ctrl+C check lives here.

With both extractions, `new()` reduces to: create channel, create flag, spawn thread calling `poll_events()`, return Self. `poll_events()` reduces to: while loop, poll, on Ok(true) call `dispatch_crossterm_event()`, on Ok(false) send Tick, on Err break. Both should be well below 15.

## Dependencies

- `EventHandler::new()` is the only constructor. It is called from TUI app startup code.
- The `Event` enum and `EventHandler` struct are defined in the same file.
- `EventHandler::next()` and `EventHandler::stop()` are separate methods with no complexity issues.

## Testability Assessment

**Existing tests (lines 136-181):**
- `event_debug_format()` -- tests `Event` enum debug formatting
- `event_clone_works()` -- tests `Event` clone
- `event_handler_stop()` -- creates an `EventHandler`, calls `stop()`, verifies `next()` fails. This test exercises `new()` indirectly.
- `event_paste_variant_debug()` and `event_paste_clone()` -- tests for `Event::Paste`

**TDD approach:** The `event_handler_stop` test is the most relevant -- it creates a handler (calling `new()`), stops it, and verifies shutdown. After extraction, this test verifies the refactored code still functions. Additionally, `cargo test event_bus` runs all tests in this module.

The extracted `dispatch_crossterm_event()` function could be unit-tested by constructing `CrosstermEvent` variants and checking the `Event` sent through the channel, but writing new tests is out of scope for pure refactoring.
