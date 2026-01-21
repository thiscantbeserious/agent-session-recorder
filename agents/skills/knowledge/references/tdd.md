# Test-Driven Development (TDD)

## Red-Green-Refactor Cycle

1. Write failing test first (behavior-focused)
2. Run test - must fail
3. Write minimal code to pass
4. Run test - must pass
5. Refactor if needed
6. Format: `cargo fmt`
7. Lint: `cargo clippy`
8. Commit

## Test Commands

```bash
cargo test              # Unit tests
./tests/e2e_test.sh     # E2E tests (requires asciinema)
```

## Testing Requirements

- All unit tests must pass
- Coverage should be >=80%
- E2E tests must pass before PR
- Log results to `.state/phase-N/test-results.md` if tracking

## Writing Good Tests

- Test behavior, not implementation
- One assertion per test when possible
- Use descriptive test names
- Test edge cases and error conditions
