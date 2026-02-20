# Sub-ADR: commands -- 3 Violations

Parent: [ADR.md](ADR.md)

## Scope

Files:
- `src/commands/analyze.rs` (328 lines)
- `src/commands/config.rs` (402 lines)

Violations: 3 functions, combined score 107

## SonarCloud-to-Source Mapping (verified)

| SonarCloud Name | SonarCloud Line | Actual Function | Actual Line | Score |
|-----------------|-----------------|-----------------|-------------|-------|
| `execute()` | 34 | `analyze::handle()` | 34 | 62 |
| `handle()` | 243 | `config::print_diff_preview()` | 243 | 28 |
| `run()` | 70 | `config::handle_migrate()` | 70 | 17 |

## Function Analysis

### 1. `analyze::handle()` at line 34 -- score 62

**Signature:**
```rust
#[cfg(not(tarpaulin_include))]
#[allow(clippy::too_many_arguments)]
pub fn handle(
    file: &str, agent_override: Option<&str>, workers: Option<usize>,
    timeout: Option<u64>, no_parallel: bool, curate: bool, debug: bool,
    output: Option<String>, fast: bool, wait: bool,
) -> Result<()>
```

**Current structure:**
Lines 34-282. A long sequential pipeline:
1. Config load and agent resolution (lines 46-53)
2. File path resolution and validation (lines 55-73)
3. Agent config lookup (line 76)
4. **Option building** (lines 78-127): Three-tier cascade for workers (lines 82-86), timeout (lines 89-93), no_parallel/debug/output (lines 95-103), fast (lines 106-108), then per-agent config application (lines 111-127) with 4 nested `if let Some(...) { if !...is_empty() { ... } }` blocks.
5. Service creation and availability check (lines 129-140)
6. **Existing markers prompt** (lines 142-158): stdin prompt with `if existing_count > 0` and `if input.trim().eq_ignore_ascii_case("y")`.
7. Analysis execution and result reporting (lines 160-176)
8. **Curation flow** (lines 178-228): `if result.markers.len() > CURATION_THRESHOLD` with nested `if effective_curate` / `else` prompt, then `if should_curate` with `match service.curate_markers(...)` (Ok/Err arms).
9. **Rename suggestion flow** (lines 236-272): `if !result.markers.is_empty()` with nested `match service.suggest_rename(...)` -> `Some(suggested)` with `if new_path != filepath && !new_path.exists()` and stdin prompt.
10. Wait prompt (lines 274-279)

**Why complexity is high:** Long function with 12+ levels of sequential branching. The option cascade and curation/rename interactive flows contribute the most nesting.

**Borrow checker constraint:** This is a free function (not a method). All data flows through parameters and local variables. No borrow checker issues. Extracted helpers can be free functions in the same module.

**Tarpaulin/test constraint:** The function is `#[cfg(not(tarpaulin_include))]` and interacts with `stdin`/`stdout`. No unit tests can exercise it. All extracted helpers that also use stdin/stdout should also be marked `#[cfg(not(tarpaulin_include))]`. The TDD baseline is `cargo test` passing.

**Extraction targets:**

1. `build_analyze_options(config: &Config, agent: AgentType, workers: Option<usize>, timeout: Option<u64>, no_parallel: bool, debug: bool, output: Option<String>, fast: bool) -> AnalyzeOptions` -- free function. Covers lines 79-108. Builds the options struct with the three-tier cascade. Pure computation, no stdin/stdout.

2. `apply_agent_config(options: AnalyzeOptions, agent_config: Option<&AgentConfig>) -> AnalyzeOptions` -- free function. Covers lines 110-127. Applies per-agent extra_args and token_budget. Pure computation, no stdin/stdout.

3. `handle_curation(service: &AnalyzerService, markers: &[ValidatedMarker], total_duration: f64, effective_curate: bool, filepath: &Path, timeout: Option<u64>) -> Result<usize>` -- free function. Covers lines 178-228. Handles the curation prompt and execution. Uses stdin/stdout, so mark `#[cfg(not(tarpaulin_include))]`.

4. `handle_rename_suggestion(service: &AnalyzerService, markers: &[ValidatedMarker], total_duration: f64, filepath: &Path, timeout: Option<u64>) -> Result<()>` -- free function. Covers lines 236-272. Handles the rename prompt. Uses stdin/stdout, so mark `#[cfg(not(tarpaulin_include))]`.

With all 4 extractions, `handle()` becomes a linear pipeline of named function calls.

### 2. `config::print_diff_preview()` at line 243 -- score 28

**Signature:**
```rust
fn print_diff_preview(new_content: &str, added_fields: &[String], is_new_file: bool)
```

**Current structure:**
Lines 243-306. Iterates over `new_content.lines()` with mutable state tracking:
- `current_section: String` -- current TOML section name
- `section_has_additions: bool` -- whether current section has added fields
- `pending_section_header: Option<String>` -- deferred section header output

For each line:
1. **Section header branch** (lines 256-272): `if let Some(section_name) = parse_simple_section_header(trimmed)` with nested `let is_added_section = ...`, assignment to `current_section` and `section_has_additions`, and `if is_new_file || is_added_section` for queuing the header.
2. **Field assignment branch** (lines 275-297): `if let Some(eq_pos) = trimmed.find('=')` with nested `if is_new_file || is_added` for printing green-prefixed output, and `if let Some(header) = pending_section_header.take()` for flushing deferred headers. Then `else if section_has_additions` with another `if let Some(header)`.
3. **Remaining lines branch** (lines 298-304): `else if is_new_file && !trimmed.is_empty()` for printing comments in new files.

**Why complexity is high:** Three-way branching inside a loop, with each branch containing 2-3 levels of nested conditionals. The `pending_section_header` deferred output pattern adds control flow complexity.

**Borrow checker constraint:** Free function taking references. No borrow issues.

**Tarpaulin/test constraint:** This function is not marked `#[cfg(not(tarpaulin_include))]` itself, but it prints directly to stdout. No unit tests exist for it.

**Extraction targets:**

1. `handle_section_header_line(trimmed: &str, added_fields: &[String], is_new_file: bool, current_section: &mut String, section_has_additions: &mut bool, pending_section_header: &mut Option<String>)` -- free function. Covers lines 256-272. Updates the mutable state variables for a section header line and decides whether to queue the header. Returns early if the line is not a section header (i.e., returns `bool` indicating whether it was processed).

2. `print_field_line(line: &str, trimmed: &str, current_section: &str, added_field_set: &HashSet<&str>, is_new_file: bool, section_has_additions: bool, pending_section_header: &mut Option<String>)` -- free function. Covers lines 275-297. Handles a field assignment line: checks if added, prints with green prefix or as context.

With both extractions, the main loop becomes a simple dispatch: parse section header? call (1). Has `=`? call (2). Else handle remaining. This should reduce complexity well below 15.

### 3. `config::handle_migrate()` at line 70 -- score 17

**Signature:**
```rust
#[cfg(not(tarpaulin_include))]
pub fn handle_migrate(auto_confirm: bool) -> Result<()>
```

**Current structure:**
Lines 70-183. Three sequential cases:
1. **Case 1: No changes** (lines 86-89): `if !result.has_changes()` -- early return.
2. **Case 2: New config** (lines 92-116): `if !file_exists` -- prints preview, prompts, creates dir, writes file, early return. Has nested `if let Some(parent) = config_path.parent()`.
3. **Case 3: Existing config update** (lines 118-182): Prints version info (`if result.old_version != result.new_version`), removed fields (`if !result.removed_fields.is_empty()`), added fields summary (`if total_fields > 0` with nested `if total_sections > 0`), preview, prompt, and write.

**Why complexity is high:** Score 17 barely exceeds the threshold. The branching comes from the three cases plus nested conditionals within Case 3 for version info, removed fields, and added fields display.

**Borrow checker constraint:** Free function, no issues.

**Tarpaulin/test constraint:** Marked `#[cfg(not(tarpaulin_include))]`, uses stdin/stdout. No unit tests. Baseline is `cargo test`.

**Extraction targets:**

1. `print_migration_info(result: &MigrationResult, theme: &Theme)` -- free function. Covers lines 120-163. Prints version info, removed fields, and added fields summary. This is the display logic for Case 3 that contributes most of the conditional nesting.

If extracting the display logic is not sufficient, an alternative is:

2. `handle_new_config_creation(config_path: &Path, result: &MigrationResult, auto_confirm: bool, theme: &Theme) -> Result<bool>` -- free function. Covers lines 92-116 (Case 2). Returns `true` if handled (early return from caller), `false` to continue.

## Dependencies Between Functions

- `handle_migrate()` calls `print_diff_preview()`. Both are in `src/commands/config.rs`.
- `print_diff_preview()` is at line 243, `handle_migrate()` is at line 70. They do not overlap.
- **Important:** Both functions are in the same file. If refactored in parallel, edits would conflict. They must be done sequentially. Recommendation: refactor `print_diff_preview()` first (it is lower in the file at line 243), then `handle_migrate()` (at line 70) so line-number shifts from the first edit do not affect the second.

## Testability Assessment

**Existing tests in `src/commands/analyze.rs` (lines 304-327):**
- `parse_agent_type_claude()`, `parse_agent_type_codex()`, `parse_agent_type_gemini()`, `parse_agent_type_unknown()` -- test the `parse_agent_type()` helper, not `handle()` itself.

**Existing tests for `config.rs`:** None directly. `parse_simple_section_header()` and `should_proceed()` are helper functions that could have tests but do not.

**TDD approach:**
- `analyze::handle()`: Not unit-testable (stdin/stdout, `#[cfg(not(tarpaulin_include))]`). Full `cargo test` is the baseline. The extracted `build_analyze_options()` and `apply_agent_config()` are pure functions that could be unit-tested, but writing tests is out of scope.
- `config::print_diff_preview()`: Not unit-testable (prints to stdout). Full `cargo test` is the baseline.
- `config::handle_migrate()`: Not unit-testable (stdin/stdout, `#[cfg(not(tarpaulin_include))]`). Full `cargo test` is the baseline.
