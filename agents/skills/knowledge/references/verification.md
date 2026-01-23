# Verification

**MANDATORY: Run before every PR/commit**

## Quick Check (All-in-One)

```bash
cargo fmt && cargo clippy && cargo test && ./tests/e2e_test.sh
```

## Individual Commands

| Check | Command |
|-------|---------|
| Format | `cargo fmt` |
| Lint | `cargo clippy` |
| Unit tests | `cargo test` |
| Snapshot tests | `cargo test tui_test` |
| E2E tests | `./tests/e2e_test.sh` |
| Release build | `cargo build --release` |
| Docker build | `./build.sh && ls dist/` |

## Requirements

- All unit tests must pass (including snapshot tests)
- All E2E tests must pass (requires asciinema)
- No clippy warnings
- Code formatted with rustfmt

## Snapshot Tests

Visual components (TUI) use `insta` snapshot testing to ensure consistent output.

- **Location**: `tests/unit/tui_test.rs`, snapshots in `tests/unit/snapshots/`
- **What's tested**: Theme colors, logo rendering, REC line scaling

When visual output changes intentionally:

```bash
# Tests will fail showing diff
cargo test tui_test

# Accept new snapshots if changes are correct
cd tests/unit/snapshots
for f in *.snap.new; do mv "$f" "${f%.new}"; done
```

IMPORTANT: If unsure whether snapshot changes are intentional, ASK THE USER before accepting.

See `tdd.md` for detailed snapshot testing documentation.
