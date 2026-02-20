# Plan: Reduce Cognitive Complexity — config

References: [ADR.md](ADR.md) | [REQUIREMENTS.md](REQUIREMENTS.md)

## Status: Completed

## Stages

### Stage 1: Baseline verification
- [x] `cargo test` passes before changes
- [x] `cargo clippy` clean before changes

### Stage 2: Extract helper functions
- [x] `collect_section_blocks(headers, lines) -> Vec<(String, String)>` — extracted block-building loop with group merging from `sort_toml_text()`
- [x] `sort_blocks_by_order(blocks) -> Vec<&(String, String)>` — extracted SECTION_ORDER sorting from `sort_toml_text()`
- [x] `reassemble_toml(preamble, sorted_blocks) -> String` — extracted TOML reassembly with blank-line normalization from `sort_toml_text()`
- [x] `ensure_blank_line_separator(result)` — extracted nested blank-line conditional from reassembly
- [x] `collect_present_fields(lines) -> HashMap<String, Vec<String>>` — extracted field scanning from `insert_optional_field_templates()`
- [x] `insert_missing_for_section(lines, section_name, missing)` — extracted section-bounds search and template insertion from `insert_optional_field_templates()`

### Stage 3: Regression verification
- [x] `cargo test` passes after changes
- [x] `cargo clippy` clean after changes
- [x] `cargo fmt` applied

### Stage 4: Review
- [x] Pair review: PASS
- [x] Internal review: APPROVE

## Files Modified
- `src/config/migrate/mod.rs` — extracted `collect_section_blocks()`, `sort_blocks_by_order()`, `reassemble_toml()`, `ensure_blank_line_separator()` from `sort_toml_text()`
- `src/config/docs.rs` — extracted `collect_present_fields()`, `insert_missing_for_section()` from `insert_optional_field_templates()`

## Extracted Functions
| Original Function | Score | Extracted Helpers | New Score |
|---|---|---|---|
| `sort_toml_text()` | 34 | `collect_section_blocks()`, `sort_blocks_by_order()`, `reassemble_toml()`, `ensure_blank_line_separator()` | < 15 |
| `insert_optional_field_templates()` | 28 | `collect_present_fields()`, `insert_missing_for_section()` | < 15 |
