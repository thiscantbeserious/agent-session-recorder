# Key Decisions Log

## 2026-01-20: Multi-Agent Coordination Architecture

### Decision: Coordinator session manages sub-agents
**Context:** Need better separation of concerns and verification
**Choice:** Main session acts as coordinator, spawns impl/verify agents
**Rationale:**
- Coordinator never implements directly - only orchestrates
- Fresh agents for each task prevent context pollution
- Separate verify agents ensure clean validation
- State files enable communication between agents
- Enables parallel task execution with proper verification

### Agent Types:
1. **Coordinator** - Plans, spawns, monitors, gates merges
2. **Impl Agent** - Implements features on feature branches
3. **Verify Agent** - Fresh session to run tests, review PRs

### State Files:
- `.state/coordinator.md` - Coordinator's active tracking
- `.state/phase-N/impl-results/` - Impl agent outputs
- `.state/phase-N/verify-results/` - Verify agent outputs

### Orchestrator Test (PR #2) - Learnings:
- **Workflow works:** Impl Agent → Verify Agent → Coordinator merge
- **CodeRabbit skips markdown:** Expected (path_filters exclude `*.md`)
- **Verify Agent thoroughness:** Went beyond tests - validated each checkbox against actual code
- **Fresh sessions work:** No context pollution between impl and verify
- **Agent IDs tracked:** Can resume if needed (a1dc796, ae1c323)

---

## 2025-01-19: Project Initialization

### Decision: Use Rust with specified dependencies
**Context:** Need a fast, single-binary CLI tool
**Choice:** Rust with clap, serde_json, toml, ctrlc, dirs, humansize, chrono, anyhow, thiserror
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

### Decision: TDD with behavior-focused tests
**Context:** Ensure code quality and correctness
**Choice:** Behavior-focused tests with e2e verification
**Rationale:**
- Tests describe what system does, not how
- E2E tests verify real asciinema integration

## 2026-01-20: Implementation Decisions

### Decision: Config path ~/.config/asr on ALL platforms
**Context:** dirs::config_dir() returns ~/Library/Application Support on macOS
**Choice:** Explicitly use ~/.config/asr on all platforms
**Rationale:**
- Consistent cross-platform behavior
- User expectation for CLI tools
- Easier to document and find

### Decision: Support Exit event type ("x")
**Context:** asciinema produces "x" events for exit codes
**Choice:** Add EventType::Exit to asciicast parser
**Rationale:**
- Real asciinema recordings include exit events
- E2E tests would fail without it

### Decision: Mandatory E2E tests before merge
**Context:** Unit tests alone don't verify real asciinema integration
**Choice:** tests/e2e_test.sh must pass before any PR merge
**Rationale:**
- Verifies actual recording/playback works
- Catches integration issues unit tests miss

### Decision: Keep all feature branches after merge
**Context:** Git history and rollback capability
**Choice:** Never delete branches with --delete-branch
**Rationale:**
- Preserves full history
- Easy to reference or cherry-pick

### Decision: Rust 1.92+ / rust:latest for Docker
**Context:** Need recent Rust for dependencies
**Choice:** Use rust:latest in Dockerfile, user has 1.92 locally
**Rationale:**
- Latest stable features
- Better dependency compatibility

## Important Paths
- **Project:** ~/git/simon/agent-session-recorder/
- **Repo:** github.com/thiscantbeserious/agent-session-record
- **Config:** ~/.config/asr/config.toml
- **Storage:** ~/recorded_agent_sessions/<agent>/
- **Binary:** target/release/asr

## Build/Test Commands
```bash
# Source cargo (if new shell)
. "$HOME/.cargo/env"

# Full verification (REQUIRED before PR)
cargo test                    # 79 tests
cargo build --release         # Native binary
./tests/e2e_test.sh          # E2E with real asciinema
```
