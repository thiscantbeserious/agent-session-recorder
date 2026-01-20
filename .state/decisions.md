# Key Decisions Log

## 2025-01-19: Project Initialization

### Decision: Use Rust with specified dependencies
**Context:** Need a fast, single-binary CLI tool
**Choice:** Rust with clap, serde_json, toml, ctrlc, dirs, humansize
**Rationale:**
- Single static binary, zero runtime dependencies
- Fast execution
- Good CLI ecosystem
- Easy cross-compilation

### Decision: asciicast v3 format with native markers
**Context:** Need to annotate session recordings
**Choice:** Use asciicast v3's native marker support (`"m"` events)
**Rationale:**
- Native format support, no custom extensions needed
- Markers stored directly in .cast files
- Compatible with asciinema player

### Decision: Shell out to asciinema for recording
**Context:** The `asciinema` crate is a binary, not a library
**Choice:** Shell out to `asciinema rec` command
**Rationale:**
- asciinema CLI handles PTY management, terminal capture
- We handle file management, marker injection natively

### Decision: TDD with 90% coverage target
**Context:** Ensure code quality and correctness
**Choice:** Behavior-focused tests, cargo-tarpaulin for coverage
**Rationale:**
- Tests describe what system does, not how
- Coverage enforcement in Docker build
