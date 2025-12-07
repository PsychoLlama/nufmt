# nufmt Improvement Tickets

Code quality improvements identified during feature freeze.

## Critical

### NUFMT-001: Fix unused `self` in quote conversion functions

**File:** `crates/nufmt/src/format.rs:569, 596`

The `to_double_quotes()` and `to_single_quotes()` methods don't use `self` but are defined as instance methods. This triggers `clippy::unused_self`.

**Fix:** Convert to associated functions or make them free functions.

---

## Code Organization

### NUFMT-002: Extract `process_gap()` into smaller functions

**File:** `crates/nufmt/src/format.rs:275-360`

This 86-line function handles comments, whitespace, and empty lines in source gaps. It has ~12 conditional branches and is difficult to understand.

**Suggested extraction:**
- `process_comment_in_gap()`
- `process_whitespace_content()`
- `process_empty_line_in_gap()`

### NUFMT-003: Extract `process_token()` handlers

**File:** `crates/nufmt/src/format.rs:191-273`

This 83-line function handles multiple token types inline with 7+ conditional branches. Each token type handler could be a separate method.

### NUFMT-004: Extract `process_block_token()` closure parsing

**File:** `crates/nufmt/src/format.rs:362-455`

The closure parameter parsing logic (lines 368-396) is complex with nested string manipulation. The calculation `param_start + param_end + 2` is not self-documenting.

**Extract:** `parse_closure_params(token: &str) -> (Option<&str>, &str)`

### NUFMT-005: Extract config file loading helper

**File:** `crates/nufmt-cli/src/main.rs:100-130`

`load_config()` has duplicated TOML parsing logic for explicit config vs found config paths.

**Extract:** `load_config_file(path: &Path) -> Result<Config, Error>`

---

## Dead Code

### NUFMT-006: Remove unused `last_token` field

**File:** `crates/nufmt/src/format.rs:155`

The `last_token` field is set at lines 237, 246, 264, 272 but never read. Either remove it or add a comment explaining future intent.

### NUFMT-007: Clarify or remove empty branch in `process_gap()`

**File:** `crates/nufmt/src/format.rs:348-350`

```rust
} else if !first_line {
    // Trailing part after last newline but no content
    // Don't add space, newline already happened
}
```

This branch does nothing. Verify it's intentional or remove it.

---

## Documentation

### NUFMT-008: Add doc comments to core formatting functions

**File:** `crates/nufmt/src/format.rs`

These functions lack documentation:
- `process_token()` (line 191)
- `process_gap()` (line 275)
- `process_block_token()` (line 362)
- `process_delimiter_token()` (line 456)
- `write_indent()`, `push_char()`, `push_str()`, `push_newline()`

### NUFMT-009: Add doc comments to CLI functions

**File:** `crates/nufmt-cli/src/main.rs`

These functions lack documentation:
- `format_stdin()` (line 146)
- `format_file()` (line 165)
- `print_diff()` (line 182)
- `Error` enum and `Display` impl (lines 235-264)

### NUFMT-010: Clean up confusing comments in quote conversion

**File:** `crates/nufmt/src/format.rs:579-588`

Multiple contradictory explanation attempts:
```rust
// Convert escapes: \' -> ', and add \" for any " ...
// In Nushell single quotes, \' is literal backslash-quote...
// Actually single quotes are raw - no escapes...
```

Replace with a single clear explanation of Nushell's quote semantics.

### NUFMT-011: Add example config file

Create `.nufmt.example.toml` documenting all configuration options with comments explaining each setting.

---

## Testing

### NUFMT-012: Add CLI unit tests

**File:** `crates/nufmt-cli/src/main.rs`

Zero unit tests for CLI logic. Add tests for:
- `load_config()` - config file discovery and parsing
- `find_config_file()` - recursive directory search
- `format_stdin()` - stdin/stdout handling
- `format_file()` - file modification behavior
- `print_diff()` - diff output correctness

### NUFMT-013: Add edge case fixtures

**File:** `crates/nufmt/tests/fixtures/`

Current fixtures cover basic cases. Add fixtures for:
- Empty files
- Files with only comments
- Very long lines (exceeding max_width)
- Deeply nested structures
- Mixed quote styles with escapes
- UTF-8 special characters
- Windows line endings (CRLF)

---

## Robustness

### NUFMT-014: Add configuration validation

**File:** `crates/nufmt/src/config.rs`

Config accepts any `usize` values without validation. Nonsensical values like `indent_width=0` or `max_width=1` are accepted silently.

Add validation with meaningful error messages.

### NUFMT-015: Review diff algorithm edge cases

**File:** `crates/nufmt-cli/src/main.rs:192-226`

The custom diff algorithm has complex nested conditions. Verify correctness for:
- Empty files
- Single-line files
- Large insertions/deletions

Consider using a tested diff library.

### NUFMT-016: Add test cases for quote conversion edge cases

**File:** `crates/nufmt/src/format.rs:569-616`

The quote conversion logic assumes simple quote patterns. Add test cases for:
- Strings containing both quote types
- Escaped characters
- Empty strings
- Strings with only whitespace

---

## Performance

### NUFMT-017: Pre-allocate output string

**File:** `crates/nufmt/src/format.rs`

The formatter builds output character by character. Pre-allocating with an estimated capacity (e.g., source length) would reduce allocations.

### NUFMT-018: Consider parallel file processing

**File:** `crates/nufmt-cli/src/main.rs:75-87`

Files are processed sequentially. For large codebases, parallel processing with rayon could improve performance.

**Note:** Low priority unless users report slow batch formatting.

---

## Future Enhancements

### NUFMT-019: Glob pattern support

**File:** `crates/nufmt-cli/src/main.rs`

Currently requires exact file paths. Add glob pattern support for file discovery (already noted in plan.md as future work).

### NUFMT-020: Progress indication for batch operations

Add optional verbose output showing progress when formatting multiple files.
