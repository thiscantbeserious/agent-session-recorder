# Commands Reference

## AGR CLI

```
agr record <agent> [-- args...]      # Start recording session
agr analyze <file> [--agent <name>]  # Analyze recording with AI
agr status                           # Show storage stats
agr cleanup                          # Interactive cleanup
agr list [agent]                     # List sessions
agr marker add <file> <time> <label> # Add marker
agr marker list <file>               # List markers
agr agents list                      # List configured agents
agr agents add <name>                # Add agent to config
agr config show                      # Show current config
agr config edit                      # Open config in editor
```

## Cargo

```bash
cargo fmt             # Format code
cargo clippy          # Lint for common issues
cargo test            # Run all tests
cargo build --release # Build release binary
cargo run -- <args>   # Run in development
```

## Build Scripts

```bash
./build.sh            # Docker build (runs tests + builds binary)
./install.sh          # Install to system
./uninstall.sh        # Remove from system
./tests/e2e_test.sh   # Run E2E tests
```
