# Sub-ADR: clipboard -- 1 Violation

Parent: [ADR.md](ADR.md)

## Scope

File: `src/clipboard/copy.rs`
Violations: 1 function, score 16

## SonarCloud-to-Source Mapping (verified)

| SonarCloud Name | SonarCloud Line | Actual Function | Actual Line | Score |
|-----------------|-----------------|-----------------|-------------|-------|
| `copy_output()` | 40 | `Copy::file()` (method on `Copy` struct) | 40 | 16 |

Note: SonarCloud reports the struct as `ClipboardManager` but the actual struct name is `Copy` (re-exported as the clipboard copy implementation).

## Function Analysis

### `Copy::file()` at line 40 -- score 16

**Signature:**
```rust
pub fn file(&self, path: &Path) -> Result<CopyResult, ClipboardError>
```

**Current structure:**
Lines 40-112. Two sequential tool-iteration loops with fallback:

1. **Validate file exists** (lines 42-46): early return on missing file.

2. **File copy loop** (lines 49-70): `for tool in &self.tools` with:
   - `if tool.is_available() && tool.can_copy_files()` (line 51)
     - `match tool.try_copy_file(path)` (line 52) with 4 arms:
       - `Ok(())` -- return success
       - `Err(NotSupported)` -- continue
       - `Err(NotFound)` -- continue
       - `Err(Failed(msg))` -- log, save error, continue

3. **Size check** (lines 73-79): check file size before content fallback.

4. **Content copy loop** (lines 82-104): `for tool in &self.tools` with:
   - `if tool.is_available()` (line 86)
     - `match tool.try_copy_text(&content)` (line 87) with same 4 arms

5. **All tools failed** (lines 107-111): report last error, return `NoToolAvailable`.

**Why complexity is high:** Score 16 barely exceeds the threshold. The two loops each have `if available` + `match` with 4 arms, contributing nesting. The `Failed` arm has a nested `eprintln!`.

**Borrow checker constraint:** `&self` method iterating over `self.tools`. Extracted helpers can be methods or free functions. The tools are `Box<dyn CopyTool>`, so references to them are straightforward.

**Extraction targets:**

1. `try_copy_file_with_tools(&self, path: &Path) -> Result<CopyResult, Option<String>>` -- method. Covers lines 49-70. Iterates tools attempting file copy. Returns `Ok(result)` on success, `Err(Some(last_error))` or `Err(None)` on all-tools-failed. This separates the file-copy attempt loop from the main function.

2. `try_copy_text_with_tools(&self, content: &str) -> Result<CopyResult, Option<String>>` -- method. Covers lines 85-104. Same pattern for text copy.

**Alternative (simpler):** Extract a single `try_tool_copy<F>(&self, attempt: F) -> Result<CopyResult, Option<String>>` generic helper that takes a closure performing the tool-specific copy attempt. Both loops use an identical pattern (check available, try operation, handle error variants). However, the `can_copy_files()` check in the file loop and its absence in the text loop mean the generic version would need a filter predicate, adding complexity. The two separate methods are simpler.

**Minimum viable extraction:** Even extracting just (1) alone might reduce the score below 15 since the file loop is the first of two complexity sources. The implementer should check the score after extracting (1) and add (2) if needed.

## Dependencies

- `Copy::file()` is the public API for clipboard file copying.
- It calls methods on `Box<dyn CopyTool>` trait objects.
- No other functions in this file are flagged.

## Testability Assessment

**Existing tests:** Run `cargo test clipboard` or `cargo test` as baseline. The clipboard module likely has integration-level tests that verify the copy workflow.

**TDD approach:** Full `cargo test` is the baseline. The extracted methods would need mock `CopyTool` implementations to unit test, which is out of scope for pure refactoring.
