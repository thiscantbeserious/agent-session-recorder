# Plan: Reduce Cognitive Complexity — clipboard

References: [ADR.md](ADR.md) | [REQUIREMENTS.md](REQUIREMENTS.md)

## Status: Completed

## Stages

### Stage 1: Baseline verification
- [x] `cargo test` passes before changes
- [x] `cargo clippy` clean before changes

### Stage 2: Extract helper functions
- [x] `try_copy_file_with_tools(&self, path: &Path) -> Result<CopyResult, Option<String>>` — extracted file-copy tool iteration loop (lines 49-70) into private method on `Copy`
- [x] `try_copy_text_with_tools(&self, content: &str) -> Result<CopyResult, Option<String>>` — extracted text-copy tool iteration loop (lines 85-104) into private method on `Copy`
- [x] Flattened guard clauses in extracted methods (early `continue` instead of nested `if`)
- [x] Consolidated error reporting: cross-phase last-error now preserved from file-copy phase into text-copy phase fallback

### Stage 3: Regression verification
- [x] `cargo test` passes after changes
- [x] `cargo clippy` clean after changes
- [x] `cargo fmt` applied

### Stage 4: Review
- [x] Pair review: PASS
- [x] Internal review: APPROVE after fix (preserved cross-phase error context)

## Files Modified
- `src/clipboard/copy.rs` — extracted two helper methods from `Copy::file()`, flattened nesting, unified error propagation

## Extracted Functions
| Original Function | Score | Extracted Helpers | New Score |
|---|---|---|---|
| `Copy::file()` | 16 | `try_copy_file_with_tools()`, `try_copy_text_with_tools()` | < 15 |
