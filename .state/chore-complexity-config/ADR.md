# Sub-ADR: config -- 2 Violations

Parent: [ADR.md](ADR.md)

## Scope

Files:
- `src/config/migrate/mod.rs` -- 1 violation (score 34)
- `src/config/docs.rs` -- 1 violation (score 28)

Violations: 2 functions, combined score 62

## SonarCloud-to-Source Mapping (verified)

| SonarCloud Name | SonarCloud Line | Actual Function | Actual Line | Score |
|-----------------|-----------------|-----------------|-------------|-------|
| `migrate_config()` | 147 | `sort_toml_text()` | 147 | 34 |
| `format_field()` | 165 | `insert_optional_field_templates()` | 165 | 28 |

## Function Analysis

### 1. `sort_toml_text()` at line 147 -- score 34

**Signature:**
```rust
fn sort_toml_text(text: &str) -> String
```

**Current structure:**
Lines 147-233. Three phases:

1. **Find section headers** (lines 148-159): `for (i, line)` loop with `if let Some(name) = parse_section_header(trimmed)` extracting group names.

2. **Build section blocks** (lines 171-191): `for (idx, (start, group))` loop:
   - Compute block end (lines 175-179): `if idx + 1 < headers.len()` / else
   - Join block text (line 181)
   - `if let Some(pos) = seen_groups.iter().position(|g| g == group)` (line 184) -- append to existing / else create new block

3. **Sort blocks** (lines 193-204): Two loops -- first iterating `SECTION_ORDER` to collect known sections, then iterating `blocks` to collect unknowns.

4. **Reassemble** (lines 206-232):
   - Preamble handling with `if !preamble.is_empty()` and `if !result.ends_with('\n')` (lines 209-213)
   - Block loop with `if i > 0 || !preamble.is_empty()` and nested `if !result.ends_with("\n\n")` with further `if result.ends_with('\n')` / `else` (lines 216-225)
   - Trailing newline check (lines 227-229)

**Why complexity is high:** The reassembly phase has 3 levels of nested conditionals for blank-line normalization between blocks. The block-building phase has 2 levels from the `if let Some(pos)` merge logic.

**Borrow checker constraint:** Free function taking `&str`. No borrow issues.

**Extraction targets:**

1. `collect_section_blocks(headers: &[(usize, String)], lines: &[&str]) -> Vec<(String, String)>` -- free function. Covers lines 171-191. Builds the block vector with group merging. Returns `Vec<(group_name, block_text)>`.

2. `reassemble_sorted_toml(preamble: &str, sorted_blocks: &[&(String, String)]) -> String` -- free function. Covers lines 206-232. Takes the preamble and sorted blocks, reassembles with blank-line normalization.

With both extractions, `sort_toml_text()` becomes: find headers, early return if empty, compute preamble, call `collect_section_blocks()`, sort blocks by SECTION_ORDER, call `reassemble_sorted_toml()`. This should reduce complexity well below 15.

### 2. `insert_optional_field_templates()` at line 165 -- score 28

**Signature:**
```rust
pub fn insert_optional_field_templates(toml_str: &str) -> String
```

**Current structure:**
Lines 165-257. Two phases:

1. **Collect present fields** (lines 168-192): For each line:
   - `if trimmed.starts_with('[') && !trimmed.starts_with("[[")` (line 173) -- track current section
   - `else if let Some((before_eq, _)) = trimmed.split_once('=')` (line 181) -- detect field assignments, including commented-out templates via `before_eq.strip_prefix('#')`. Push to `present` HashMap.

2. **Insert missing field templates** (lines 194-250): Build `section_fields` list, iterate in reverse:
   - Filter missing fields: `fields.iter().filter(...)` with nested `section_present.map(|p| !p.iter().any(...)).unwrap_or(true)` (lines 206-213)
   - `if missing.is_empty() { continue; }` (lines 215-217)
   - Find section start: `let section_start = lines.iter().position(|l| l.trim() == header)` (line 221)
   - `if let Some(start) = section_start` (line 222):
     - Find section end: `lines[start + 1..].iter().position(|l| ...)` with nested section header check (lines 223-230)
     - Find last content line with `for (i, line) in lines.iter().enumerate().take(section_end).skip(start + 1)` and `if !line.trim().is_empty()` (lines 234-239)
     - Build and insert templates (lines 242-248)

**Why complexity is high:** The "insert missing" phase has 4 levels of nesting: reverse iteration > if let section_start > section_end search > last-content-line search. The "collect" phase has 2 levels.

**Borrow checker constraint:** Free function, no issues.

**Extraction targets:**

1. `collect_present_fields(lines: &[String]) -> HashMap<String, Vec<String>>` -- free function. Covers lines 168-192. Scans TOML lines and returns present fields per section. Pure computation.

2. `insert_missing_for_section(lines: &mut Vec<String>, section_name: &str, missing: &[&FieldDoc])` -- free function. Covers lines 219-249 (the body of the `if let Some(start) = section_start` block). Takes the lines vec, section name, and missing fields. Finds the section bounds, locates the insert position, and inserts template lines. This extracts the deepest nesting block.

With both extractions, `insert_optional_field_templates()` becomes: collect present fields, build section_fields list, iterate in reverse: compute missing, find section start, call `insert_missing_for_section()`.

## Dependencies

- `sort_toml_text()` is called from `sort_sections()` in `config/migrate/mod.rs`.
- `insert_optional_field_templates()` is called from `handle_show()` in `commands/config.rs`.
- The two functions are in **different files** (`config/migrate/mod.rs` and `config/docs.rs`), so they can be refactored in parallel.

## Testability Assessment

**For `sort_toml_text()` (`config/migrate/mod.rs`):**
- Tested via `cargo test migrate`. The migration tests include section sorting verification.
- The function is private (`fn sort_toml_text`), called from `sort_sections()`.

**For `insert_optional_field_templates()` (`config/docs.rs`):**
- Tested via `cargo test docs`. The docs module has tests for annotation and template insertion.
- The function is public.

**TDD approach:** Run `cargo test migrate` and `cargo test docs` respectively before and after each extraction. Both functions have good test coverage through their module tests. Extracted helpers are pure functions that could be independently tested but this is out of scope.
