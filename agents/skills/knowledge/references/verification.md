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
| E2E tests | `./tests/e2e_test.sh` |
| Release build | `cargo build --release` |
| Docker build | `./build.sh && ls dist/` |

## Requirements

- All unit tests must pass
- All E2E tests must pass (requires asciinema)
- No clippy warnings
- Code formatted with rustfmt
