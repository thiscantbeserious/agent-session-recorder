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
cargo test              # Unit tests (includes snapshot tests)
./tests/e2e_test.sh     # E2E tests (requires asciinema)
```

## Testing Requirements

- All unit tests must pass
- Coverage should be >=80%
- E2E tests must pass before PR
- Log results to `.state/phase-N/test-results.md` if tracking

## Test Organization

Tests are separate from source code. Never use inline `#[cfg(test)]` modules.

```
tests/
  integration.rs          # Integration test module root
  integration/
    storage_test.rs       # Tests for storage module
    markers_test.rs       # Tests for markers module
    snapshots/            # Snapshot files for visual tests
  e2e/
    *.sh                  # End-to-end shell scripts
  fixtures/
    *.cast                # Test data files
```

**Preference:** Integration tests in `tests/` over inline `#[cfg(test)]` modules, unless not feasible.

**Naming:** `<module>_test.rs` for files, descriptive behavior names for functions.

## Writing Good Tests

- Test behavior, not implementation
- One assertion per test when possible
- Use descriptive test names
- Test edge cases and error conditions

## Snapshot Testing (Visual Components)

For TUI/visual components, use `insta` for snapshot testing. This ensures visual output remains consistent.

### Location

- Test file: `tests/unit/tui_test.rs`
- Snapshots: `tests/unit/snapshots/`

### Current Snapshots

| Snapshot | Purpose |
|----------|---------|
| `snapshot_theme_colors` | Captures exact color values (text_primary, accent, etc.) |
| `snapshot_logo_visual` | Renders Logo widget and captures visual output with colors |
| `snapshot_logo_rec_line_scales` | Verifies REC line scales at 40/80/120 widths |

### Updating Snapshots

When theme colors or visual output intentionally changes:

```bash
# Run tests - will fail with diff showing changes
cargo test tui_test

# Review the .snap.new files in tests/unit/snapshots/
# If changes are correct, accept them:
cd tests/unit/snapshots
for f in *.snap.new; do mv "$f" "${f%.new}"; done

# Re-run to confirm
cargo test tui_test
```

IMPORTANT: If snapshot tests fail unexpectedly, ASK THE USER before accepting new snapshots. Only accept if the user confirms the visual changes were intentional.

### Adding New Snapshot Tests

```rust
use insta;

#[test]
fn snapshot_my_widget() {
    let output = render_widget_to_string(MyWidget::new(), 80, 24);
    insta::assert_snapshot!(output);
}
```

First run creates `.snap.new` file. Review and rename to `.snap` to accept.

### MANDATORY: Snapshot Before Visual Changes

Before making ANY visual changes (colors, theme, layout, styling):

1. **Run existing snapshot tests first** to capture current state:
   ```bash
   cargo test tui_test
   ```

2. **If tests pass**: Current snapshots are valid baseline
3. **If tests fail**: Review changes, ask user if intentional before accepting

4. **After making changes**: Run tests again
   - Tests will fail showing visual diff
   - Ask user to verify the visual change is correct
   - Only accept snapshots after user confirmation

This ensures visual consistency and prevents accidental style regressions.
