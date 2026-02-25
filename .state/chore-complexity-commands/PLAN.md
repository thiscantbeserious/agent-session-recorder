# Plan: Reduce Cognitive Complexity — commands

References: [ADR.md](ADR.md) | [REQUIREMENTS.md](REQUIREMENTS.md)

## Status: Completed

## Stages

### Stage 1: Baseline verification
- [x] `cargo test` passes before changes
- [x] `cargo clippy` clean before changes

### Stage 2: Extract helper functions
- [x] `build_analyze_options(config, agent, workers, timeout, no_parallel, debug, output, fast) -> AnalyzeOptions` — extracted three-tier cascade option building from `analyze::handle()`
- [x] `apply_agent_config(options, agent_config) -> AnalyzeOptions` — extracted per-agent extra_args/token_budget application from `analyze::handle()`
- [x] `prompt_remove_existing_markers(filepath) -> Result<()>` — extracted existing-marker prompt from `analyze::handle()`
- [x] `handle_curation(service, markers, total_duration, effective_curate, filepath, timeout) -> Result<usize>` — extracted curation flow with prompt from `analyze::handle()`
- [x] `prompt_for_curation(marker_count, effective_curate) -> Result<bool>` — extracted curation prompt logic
- [x] `handle_rename_suggestion(service, markers, total_duration, filepath, timeout) -> Result<()>` — extracted rename suggestion flow from `analyze::handle()`
- [x] `handle_new_config_creation(config_path, result, auto_confirm, theme) -> Result<()>` — extracted Case 2 (new config) from `handle_migrate()`
- [x] `print_migration_info(result, theme)` — extracted Case 3 display logic from `handle_migrate()`
- [x] `handle_section_header_line(line, section_name, added_fields, is_new_file, ...)` — extracted section header state tracking from `print_diff_preview()`
- [x] `print_field_line(line, trimmed, current_section, added_field_set, ...)` — extracted field assignment printing from `print_diff_preview()`

### Stage 3: Regression verification
- [x] `cargo test` passes after changes
- [x] `cargo clippy` clean after changes
- [x] `cargo fmt` applied

### Stage 4: Review
- [x] Pair review: PASS
- [x] Internal review: APPROVE

## Files Modified
- `src/commands/analyze.rs` — extracted 6 functions from `analyze::handle()`, converting 280-line monolith into linear pipeline
- `src/commands/config.rs` — extracted 4 functions from `handle_migrate()` and `print_diff_preview()`

## Extracted Functions
| Original Function | Score | Extracted Helpers | New Score |
|---|---|---|---|
| `analyze::handle()` | 62 | `build_analyze_options()`, `apply_agent_config()`, `prompt_remove_existing_markers()`, `handle_curation()`, `prompt_for_curation()`, `handle_rename_suggestion()` | < 15 |
| `config::print_diff_preview()` | 28 | `handle_section_header_line()`, `print_field_line()` | < 15 |
| `config::handle_migrate()` | 17 | `handle_new_config_creation()`, `print_migration_info()` | < 15 |
