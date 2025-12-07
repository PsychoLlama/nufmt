# nufmt Improvement Tickets

Code quality improvements identified during feature freeze.

**Status: All 20 tickets completed.**

## Critical

### NUFMT-001: Fix unused `self` in quote conversion functions [COMPLETED]

**File:** `crates/nufmt/src/format.rs:569, 596`

The `to_double_quotes()` and `to_single_quotes()` methods don't use `self` but are defined as instance methods. This triggers `clippy::unused_self`.

**Fix:** Converted to free functions outside the impl block.

---

## Code Organization

### NUFMT-002: Extract `process_gap()` into smaller functions [COMPLETED]

**File:** `crates/nufmt/src/format.rs:275-360`

This 86-line function handles comments, whitespace, and empty lines in source gaps. It has ~12 conditional branches and is difficult to understand.

**Fix:** Extracted `process_gap_comment()`, `process_gap_content()`, `process_gap_empty()`.

### NUFMT-003: Extract `process_token()` handlers [COMPLETED]

**File:** `crates/nufmt/src/format.rs:191-273`

This 83-line function handles multiple token types inline with 7+ conditional branches. Each token type handler could be a separate method.

**Fix:** Extracted `is_valid_span()`, `try_process_block()`, `process_pipe_token()`, `process_string_token()`, `write_token()`.

### NUFMT-004: Extract `process_block_token()` closure parsing [COMPLETED]

**File:** `crates/nufmt/src/format.rs:362-455`

The closure parameter parsing logic (lines 368-396) is complex with nested string manipulation.

**Fix:** Extracted `parse_block_content()`, `parse_closure_params()`, `write_block_open()`, `write_block_inner()`, `write_block_close()`.

### NUFMT-005: Extract config file loading helper [COMPLETED]

**File:** `crates/nufmt-cli/src/main.rs:100-130`

`load_config()` has duplicated TOML parsing logic for explicit config vs found config paths.

**Fix:** Extracted `load_config_file(path: &Path) -> Result<Config, Error>` with validation.

---

## Dead Code

### NUFMT-006: Remove unused `last_token` field [COMPLETED]

**File:** `crates/nufmt/src/format.rs:155`

The `last_token` field is set at lines 237, 246, 264, 272 but never read.

**Fix:** Removed the unused field.

### NUFMT-007: Clarify or remove empty branch in `process_gap()` [COMPLETED]

**File:** `crates/nufmt/src/format.rs:348-350`

This branch does nothing. Verify it's intentional or remove it.

**Fix:** Addressed during NUFMT-002 refactoring.

---

## Documentation

### NUFMT-008: Add doc comments to core formatting functions [COMPLETED]

**File:** `crates/nufmt/src/format.rs`

**Fix:** Added doc comments to all core formatting functions including `process_token()`, `process_gap()`, `process_block_token()`, `process_delimiter_token()`, and helper methods.

### NUFMT-009: Add doc comments to CLI functions [COMPLETED]

**File:** `crates/nufmt-cli/src/main.rs`

**Fix:** Added doc comments to `format_stdin()`, `format_file()`, `print_diff()`, `Error` enum and variants.

### NUFMT-010: Clean up confusing comments in quote conversion [COMPLETED]

**File:** `crates/nufmt/src/format.rs:579-588`

Multiple contradictory explanation attempts in the quote conversion code.

**Fix:** Addressed during NUFMT-001 when converting to free functions with clear documentation.

### NUFMT-011: Add example config file [COMPLETED]

**Fix:** Created `.nufmt.example.toml` documenting all configuration options with comments.

---

## Testing

### NUFMT-012: Add CLI unit tests [COMPLETED]

**File:** `crates/nufmt-cli/src/main.rs`

Zero unit tests for CLI logic.

**Fix:** Added 17 unit tests covering config loading, file formatting, and diff output. Added `tempfile` dev dependency.

### NUFMT-013: Add edge case fixtures [COMPLETED]

**File:** `crates/nufmt/tests/fixtures/`

**Fix:** Added fixtures for:
- Empty files (empty.nu)
- Files with only comments (comments_only.nu)
- Deeply nested structures (deeply_nested.nu)
- UTF-8 special characters (unicode.nu)

---

## Robustness

### NUFMT-014: Add configuration validation [COMPLETED]

**File:** `crates/nufmt/src/config.rs`

Config accepts any `usize` values without validation.

**Fix:** Added `Config::validate()` method with meaningful error messages. Added `ConfigError` type. Validation enforces:
- `indent_width`: 1-16
- `max_width`: 20-500

### NUFMT-015: Review diff algorithm edge cases [COMPLETED]

**File:** `crates/nufmt-cli/src/main.rs:192-226`

The custom diff algorithm has complex nested conditions.

**Fix:** Added 8 edge case tests for empty files, single-line files, and large insertions/deletions.

### NUFMT-016: Add test cases for quote conversion edge cases [COMPLETED]

**File:** `crates/nufmt/src/format.rs:569-616`

**Fix:** Added 9 test cases for quote conversion covering:
- Strings containing both quote types
- Escaped characters
- Empty strings
- Strings with only whitespace

---

## Performance

### NUFMT-017: Pre-allocate output string [COMPLETED]

**File:** `crates/nufmt/src/format.rs`

**Fix:** Pre-allocate output string with `String::with_capacity(source.len() + source.len() / 10)` to reduce allocations.

### NUFMT-018: Consider parallel file processing [COMPLETED]

**File:** `crates/nufmt-cli/src/main.rs:75-87`

**Fix:** Added rayon for parallel file processing with `par_iter()`. Uses atomic booleans/counters for thread-safe state tracking.

---

## Future Enhancements

### NUFMT-019: Glob pattern support [COMPLETED]

**File:** `crates/nufmt-cli/src/main.rs`

**Fix:** Added glob pattern support using the `glob` crate. Patterns containing `*`, `?`, or `[` are expanded; literal paths pass through.

### NUFMT-020: Progress indication for batch operations [COMPLETED]

**Fix:** Added summary output showing formatted file counts:
- "Formatted X of Y files" when files were changed
- "X of Y files would be reformatted" in check mode
