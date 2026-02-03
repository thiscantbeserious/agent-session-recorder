# Enhanced Shell Minification - Implementation Plan

**Date:** 2026-01-30
**ADR:** See ADR.md
**Approach:** TDD Red/Green/Refactor

---

## Overview

Extend `src/shell/minify.rs` with aggressive compression to achieve single-digit line output (aspirational) or 10-15 lines max (acceptable).

**Integration Point:** `src/shell/completions.rs` calls minification in `generate_zsh_init()` / `generate_bash_init()`.

**Scope Constraint:** Compression only - do NOT modify completion generation logic.

---

## Stage 1: Test Infrastructure (RED)

**Goal:** Create comprehensive failing test suite before any implementation.

### Tasks

1. [ ] Create test directory `tests/integration/shell/`
2. [ ] Create test file `tests/integration/shell/minify_test.rs`
3. [ ] Create module file `tests/integration/shell/mod.rs`
4. [ ] Register module in `tests/integration.rs`
5. [ ] Write tests for each category (all should FAIL initially)

### Test Categories

#### 1.1 Basic Compression Tests
- [ ] `test_joins_simple_statements_with_semicolons`
- [ ] `test_removes_all_comments_except_shebang`
- [ ] `test_removes_blank_lines`
- [ ] `test_removes_indentation`

#### 1.2 Function Collapsing Tests
- [ ] `test_collapses_function_body_to_single_line`
- [ ] `test_handles_nested_braces_in_functions`
- [ ] `test_preserves_function_with_complex_body`

#### 1.3 Control Structure Tests
- [ ] `test_inlines_if_then_fi`
- [ ] `test_inlines_case_statement`
- [ ] `test_removes_final_double_semicolon_before_esac`
- [ ] `test_handles_nested_case_in_function`

#### 1.4 Quote Safety Tests (CRITICAL)
- [ ] `test_preserves_content_inside_double_quotes`
- [ ] `test_preserves_content_inside_single_quotes`
- [ ] `test_preserves_spaces_in_quoted_strings`
- [ ] `test_preserves_hash_inside_quotes`
- [ ] `test_preserves_nested_quotes`

#### 1.5 Special Syntax Safety Tests
- [ ] `test_preserves_space_before_process_substitution`
- [ ] `test_preserves_heredoc_content_verbatim`
- [ ] `test_preserves_zsh_parameter_expansion`
- [ ] `test_preserves_bash_array_syntax`
- [ ] `test_preserves_word_boundaries`

#### 1.6 Operator Compression Tests
- [ ] `test_removes_spaces_around_and_or`
- [ ] `test_removes_spaces_around_pipes`
- [ ] `test_removes_spaces_around_redirects`
- [ ] `test_preserves_redirect_fd_numbers`

#### 1.7 Integration Tests
- [ ] `test_minify_zsh_init_achieves_target_lines` (≤15 max, aspirational ≤10)
- [ ] `test_minify_bash_init_achieves_target_lines` (≤15 max, aspirational ≤10)
- [ ] `test_minified_output_is_valid_shell_syntax`

#### 1.8 Functional Preservation Tests (REQ-1)
- [ ] `test_minified_zsh_completions_work` (commands complete)
- [ ] `test_minified_bash_completions_work` (commands complete)
- [ ] `test_subcommand_completion_preserved`
- [ ] `test_file_completion_preserved`

#### 1.9 Debug Mode Tests (REQ-2)
- [ ] `test_debug_flag_outputs_uncompressed`
- [ ] `test_debug_output_includes_section_comments`
- [ ] `test_default_outputs_compressed`

#### 1.10 E2E Shell Tests (in tests/e2e/)
- [ ] `test_minified_zsh_sources_without_error.sh`
- [ ] `test_minified_bash_sources_without_error.sh`
- [ ] `test_zsh_completion_function_defined.sh`
- [ ] `test_bash_completion_function_defined.sh`
- [ ] `test_debug_flag_produces_readable_output.sh`

### Verification
```bash
cargo test shell::minify -- --nocapture
# All tests must FAIL (RED phase complete)
```

### Files Created
- `tests/integration/shell/mod.rs`
- `tests/integration/shell/minify_test.rs`

---

## Stage 2: Core Minification Engine (GREEN)

**Goal:** Implement basic minification to pass first test category.

### Tasks

1. [ ] Add `minify_aggressive(script: &str) -> String` to `src/shell/minify.rs`
2. [ ] Implement quote state tracking (in single/double quote or not)
3. [ ] Implement comment removal (preserve shebang)
4. [ ] Implement blank line removal
5. [ ] Implement indentation removal
6. [ ] Run tests - basic compression tests should PASS

### Verification
```bash
cargo test shell::minify::basic
```

### Files Modified
- `src/shell/minify.rs`

---

## Stage 3: Statement Joining (GREEN)

**Goal:** Join statements with semicolons.

### Tasks

1. [ ] Implement statement boundary detection
2. [ ] Implement semicolon joining for simple statements
3. [ ] Handle edge cases (don't join across control structures)
4. [ ] Ensure word boundaries preserved (`echo $var` not `echo$var`)
5. [ ] Run tests - statement joining tests should PASS

### Verification
```bash
cargo test shell::minify::joining
```

---

## Stage 4: Function Collapsing (GREEN)

**Goal:** Collapse function bodies to single lines.

### Tasks

1. [ ] Detect function definition boundaries (`name() {` ... `}`)
2. [ ] Collapse function body statements with `;`
3. [ ] Preserve nested braces correctly
4. [ ] Handle complex bodies (multiple statements, local vars)
5. [ ] Run tests - function tests should PASS

### Verification
```bash
cargo test shell::minify::function
```

---

## Stage 5: Control Structure Inlining (GREEN)

**Goal:** Inline if/case statements where safe.

### Tasks

1. [ ] Implement if/then/fi inlining: `if cond;then cmd;fi`
2. [ ] Implement case/esac compression
3. [ ] Remove redundant final `;;` before `esac`
4. [ ] Handle nested case in functions
5. [ ] Run tests - control structure tests should PASS

### Verification
```bash
cargo test shell::minify::control
```

---

## Stage 6: Operator Compression (GREEN)

**Goal:** Remove spaces around operators.

### Tasks

1. [ ] Remove spaces around `&&`, `||` (outside quotes)
2. [ ] Remove spaces around `|` (outside quotes)
3. [ ] Remove spaces around redirects `>`, `<`, `>>` (outside quotes)
4. [ ] **PRESERVE** space in `< <(cmd)` (process substitution)
5. [ ] Preserve redirect fd numbers (`2>&1`)
6. [ ] Run tests - operator tests should PASS

### Verification
```bash
cargo test shell::minify::operator
```

---

## Stage 7: CLI Integration & Debug Flag (GREEN)

**Goal:** Wire up to CLI and add debug mode.

### Tasks

1. [ ] Add `--debug` flag to `CompletionShell` args in `src/cli.rs`
2. [ ] Pass debug flag through to `generate_zsh_init()` / `generate_bash_init()`
3. [ ] When debug=true: output readable version with section comments
4. [ ] When debug=false (default): output compressed version
5. [ ] Update `src/shell/completions.rs` to call `minify_aggressive()` by default
6. [ ] Run integration tests - line count targets met
7. [ ] Run functional tests - completions still work
8. [ ] Run ALL tests - everything PASSES

### Verification
```bash
cargo test shell::minify
agr completions --shell-init zsh | wc -l        # Should be ≤15 (target ≤10)
agr completions --shell-init zsh --debug | wc -l # Should be ~50 with comments
```

### Files Modified
- `src/cli.rs`
- `src/shell/completions.rs`
- `src/commands/completions.rs`

---

## Stage 8: Snapshot Tests & Regression Protection

**Goal:** Add snapshot tests for future regression detection.

### Tasks

1. [ ] Add snapshot test for compressed zsh output
2. [ ] Add snapshot test for compressed bash output
3. [ ] Add snapshot test for debug zsh output
4. [ ] Add snapshot test for debug bash output
5. [ ] Verify snapshots show expected line counts

### Verification
```bash
cargo test snapshot_completions
```

### Files Created/Modified
- `tests/integration/shell/minify_test.rs` (snapshot tests)
- `tests/integration/shell/snapshots/` (snapshot files)

---

## Stage 9: Refactor & Polish

**Goal:** Clean up implementation, ensure code quality.

### Tasks

1. [ ] `cargo fmt`
2. [ ] `cargo clippy -- -D warnings` - fix all warnings
3. [ ] Add doc comments to `minify_aggressive()` and helpers
4. [ ] Review test coverage - ensure all edge cases covered
5. [ ] Update PLAN.md - mark all stages complete

### Verification
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
./tests/e2e_test.sh
```

---

## Definition of Done

**Line Count Targets (per REQUIREMENTS):**
- [ ] Zsh output ≤ 15 lines (acceptable), target ≤ 10 (aspirational)
- [ ] Bash output ≤ 15 lines (acceptable), target ≤ 10 (aspirational)

**Functionality:**
- [ ] All tests pass
- [ ] Completions work after minification (functional tests)
- [ ] `--debug` flag works and includes section comments
- [ ] No clippy warnings
- [ ] Code formatted

**Platform:**
- [ ] Works on macOS
- [ ] Works on Linux (CI)

---

## Sign-off

- [ ] Implementer: All stages complete
- [ ] Reviewer: Code reviewed
