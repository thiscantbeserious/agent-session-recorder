# Coding Principles

Guidelines for maintaining clean, readable, and maintainable code.

## File Size

**Target: ~400 lines max per file - NO EXCEPTIONS**

All files must stay within this limit, including entry points. Large entry points should delegate to focused modules.

### When to Split

- File exceeds 400 lines
- File handles multiple distinct responsibilities
- Logical groupings emerge that could stand alone

### How to Split

**Modules:** Convert file to directory with `mod.rs` and split by responsibility.
```
# Before: src/feature.rs (600 lines)
# After:
src/feature/mod.rs       # Public API, re-exports
src/feature/core.rs      # Core logic
src/feature/helpers.rs   # Supporting functions
```

**Entry points:** Keep CLI definition and arg parsing in main, move handlers to separate modules.
```
# Before: src/main.rs (1500 lines with embedded handlers)
# After:
src/main.rs              # CLI definition, arg parsing, dispatch only
src/commands/mod.rs      # Command handler modules
src/commands/foo.rs      # Handler for one command
src/commands/bar.rs      # Handler for another command
```

The entry point should only:
- Define CLI structure
- Parse arguments
- Dispatch to handler modules
- Handle top-level errors

## Function Size

**Target: ~20 lines max per function**

Functions should do one thing well. If a function exceeds 20 lines, consider:
- Extracting helper functions
- Breaking into sequential steps
- Separating concerns

### Exceptions

A central dispatch function (like `main` or a command router) may exceed 20 lines when it consists primarily of a match/switch statement dispatching to other functions. The dispatch itself should contain minimal logic - just routing.

```rust
// Acceptable: dispatch-only function
fn run_command(cmd: Command) -> Result<()> {
    match cmd {
        Command::Foo(args) => handle_foo(args),
        Command::Bar(args) => handle_bar(args),
        Command::Baz(args) => handle_baz(args),
        // ... more arms are fine if they just dispatch
    }
}
```

### Example: Splitting a Large Function

```rust
// Before: 45-line function
fn process_recording(path: &Path) -> Result<Recording> {
    // validate input (8 lines)
    // parse header (12 lines)
    // process events (15 lines)
    // add markers (10 lines)
}

// After: orchestrator + focused helpers
fn process_recording(path: &Path) -> Result<Recording> {
    let content = validate_and_read(path)?;
    let header = parse_header(&content)?;
    let events = process_events(&content)?;
    add_markers(header, events)
}
```

## Single Responsibility

**Each function should have one clear intent**

Ask: "What does this function do?" If the answer contains "and", split it.

### Good

```rust
fn parse_timestamp(line: &str) -> Result<f64>
fn validate_event_type(event: &Event) -> bool
fn format_duration(seconds: f64) -> String
```

### Avoid

```rust
fn parse_and_validate_timestamp(line: &str) -> Result<f64>  // Two jobs
fn process_event_and_update_state(event: &Event) -> State   // Two jobs
```

## Nesting Depth

**Max 3 levels of indentation**

Deep nesting indicates complex logic that should be extracted.

### Counting Levels

```rust
fn example() {                    // Level 0
    if condition {                // Level 1
        for item in items {       // Level 2
            if check(item) {      // Level 3 - MAX
                // ...
            }
        }
    }
}
```

### Reducing Nesting

**Early returns:**
```rust
// Before
fn process(input: Option<Data>) -> Result<Output> {
    if let Some(data) = input {
        if data.is_valid() {
            // deep logic here
        }
    }
}

// After
fn process(input: Option<Data>) -> Result<Output> {
    let data = input.ok_or(Error::NoInput)?;
    if !data.is_valid() {
        return Err(Error::Invalid);
    }
    // logic at level 1
}
```

**Extract inner logic:**
```rust
// Before
for item in items {
    if condition_a {
        if condition_b {
            // complex logic
        }
    }
}

// After
for item in items {
    process_item(item)?;
}

fn process_item(item: &Item) -> Result<()> {
    if !condition_a || !condition_b {
        return Ok(());
    }
    // logic at level 1
}
```

## Test Organization

**Tests belong in separate files**

Unit tests go in `tests/unit/` directory, not inline with source code.

### Structure

```
src/
  storage.rs              # Implementation only
  markers.rs              # Implementation only
tests/
  unit.rs                 # Module root for unit tests
  unit/
    storage_test.rs       # Tests for storage module
    markers_test.rs       # Tests for markers module
    helpers/
      mod.rs              # Shared test utilities
  e2e/
    *.sh                  # End-to-end shell scripts
  fixtures/
    *.cast                # Test data files
```

### Naming Convention

- Test file: `<module>_test.rs`
- Test function: descriptive behavior name (e.g., `parse_header_extracts_version`)
- Use `#[test]` attribute

### Example

```rust
// tests/markers_test.rs
use agr::markers::*;

#[test]
fn parse_marker_extracts_timestamp_and_label() {
    let line = r#"[1.5, "m", {"label": "test"}]"#;
    let marker = parse_marker(line).unwrap();
    assert_eq!(marker.timestamp, 1.5);
    assert_eq!(marker.label, "test");
}
```

## Documentation

**Document the non-obvious, not the obvious**

Each public function/struct should have 1-2 sentences covering:
- Purpose (if not clear from name)
- Connections to other components
- Important constraints or side effects

### What to Document

- **Connections:** "Reads config from `~/.config/agr/config.toml`"
- **Side effects:** "Creates directory if it doesn't exist"
- **Constraints:** "Timestamp must be monotonically increasing"
- **Non-obvious behavior:** "Returns None for marker events"

### What NOT to Document

- Self-evident behavior: `/// Returns the user's name` on `fn get_name()`
- Implementation details that may change
- Redundant type information

### Examples

```rust
/// Spawns the configured analysis agent after recording completes.
/// Falls back to no-op if `auto_analyze` is disabled in config.
pub fn run_post_analysis(recording: &Path, config: &Config) -> Result<()>

/// Parses asciicast v3 format. Handles both header and event lines.
/// Marker events (type "m") are collected separately for quick access.
pub fn parse_recording(content: &str) -> Result<Recording>

/// Storage paths follow `~/recorded_agent_sessions/<agent>/<timestamp>.cast`
pub fn get_session_path(agent: &str) -> PathBuf
```

### Struct Documentation

```rust
/// Runtime configuration loaded from TOML.
/// Merged from system defaults, user config, and CLI overrides.
pub struct Config {
    /// Base directory for all recordings. Defaults to ~/recorded_agent_sessions.
    pub storage_path: PathBuf,

    /// Agent-specific settings keyed by agent name.
    pub agents: HashMap<String, AgentConfig>,
}
```

## Summary Checklist

Before committing, verify:

- [ ] No file exceeds ~400 lines
- [ ] No function exceeds ~20 lines (dispatch-only routing functions may exceed)
- [ ] Each function has single responsibility
- [ ] Nesting depth stays at 3 levels max
- [ ] Tests are in `tests/` directory
- [ ] Public items have 1-2 sentence docs covering non-obvious details
