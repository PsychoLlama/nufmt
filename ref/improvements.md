# nufmt Improvement Tickets

Documented code quality issues and improvement opportunities.

---

## NUFMT-001: Split `format.rs` into modules

**Category:** Maintainability
**File:** `crates/nufmt-core/src/format.rs`
**Lines:** 1291

The formatter is a single large file handling:
- Error types and display (`FormatError`, `SourceLocation`)
- Token preprocessing (`preprocess_tokens`, `Token` struct)
- Gap formatting (`format_gap`, `format_structural_gap`)
- Block formatting (`format_block_token`, `format_brace_block`, etc.)
- Collection formatting (`format_collection_token`, `format_collection_open`, etc.)
- String handling (`convert_string_quotes`, `to_double_quotes`, `to_single_quotes`)
- Utility functions (`parse_closure_params`, `offset_to_location`)

**Suggested structure:**
```
format/
  mod.rs         # Public API (format_source, debug_tokens)
  error.rs       # FormatError, SourceLocation
  token.rs       # Token struct, preprocess_tokens
  formatter.rs   # Formatter struct and core logic
  gap.rs         # Gap formatting (format_gap, format_structural_gap)
  block.rs       # Block/closure formatting
  collection.rs  # Record/list formatting
  string.rs      # Quote conversion utilities
```

---

## NUFMT-002: Duplicate enum definitions in CLI

**Category:** DRY Violation
**Files:** `crates/nufmt-cli/src/main.rs:37-91`, `crates/nufmt-core/src/config.rs:4-36`

The CLI defines wrapper enums that mirror core enums:
- `QuoteStyleArg` mirrors `QuoteStyle`
- `BracketSpacingArg` mirrors `BracketSpacing`
- `TrailingCommaArg` mirrors `TrailingComma`

Each has a `From` impl to convert to the core type. This duplication exists because clap's `ValueEnum` derive has different requirements than serde's `Deserialize`.

**Options:**
1. Use `clap(value_enum)` directly on core enums (requires adding clap as core dependency)
2. Keep current approach (explicit separation of CLI concerns from library)
3. Create a shared derive macro for both

Current approach is arguably correct (keeps CLI concerns out of library), but the boilerplate is notable.

---

## NUFMT-003: Duplication between `estimate_block_length` and `estimate_collection_length`

**Category:** DRY Violation
**File:** `crates/nufmt-core/src/format.rs:820-857, 1052-1097`

These two functions share nearly identical logic for lookahead length estimation:
- Both track depth with open/close delimiters
- Both scan tokens until depth reaches 0
- Both accumulate length and check for newlines

**Suggested fix:** Extract a generic `estimate_delimited_length` function parameterized by:
- Open/close shape matchers
- Per-token length calculation

---

## NUFMT-004: Complex gap processing logic

**Category:** Complexity
**File:** `crates/nufmt-core/src/format.rs:331-574`

`format_gap` (~130 lines) and `format_structural_gap` (~100 lines) handle many edge cases:
- Comments in gaps
- Blank line preservation
- Structural delimiters (`{`, `}`, `,`)
- Leading/trailing whitespace normalization

The code is correct but dense. Consider:
1. Adding more comments explaining each case
2. Breaking into smaller helper functions
3. Creating a `GapContent` enum to represent parsed gap types

---

## NUFMT-005: Manual indent tracking workaround

**Category:** Technical Debt
**File:** `crates/nufmt-core/src/format.rs:287, 303-305`

The `indent_level` field and `indent_str()` method exist because the `pretty` crate's automatic indentation doesn't work well with token-based formatting. The formatter manually tracks indentation and emits explicit indent strings.

This is a known limitation. Ideally, the pretty crate's `nest()` combinator would handle this, but it doesn't integrate cleanly with the token-gap model.

**Options:**
1. Accept as necessary complexity
2. Research alternative pretty printing libraries
3. Contribute upstream improvements to `pretty` crate

---

## NUFMT-006: Custom diff algorithm

**Category:** Maintainability
**File:** `crates/nufmt-cli/src/main.rs:611-661`

`print_diff` implements a custom diff algorithm (~50 lines). While functional, it:
- Doesn't produce standard unified diff format
- Has edge cases tested but may miss others
- Duplicates functionality available in crates like `similar` or `diff`

**Suggested fix:** Replace with `similar` crate for proper unified diff output.

---

## NUFMT-007: Parser limitation with string content

**Category:** Known Issue (Upstream)
**Impact:** Idempotency

The nu-parser tokenizes content inside strings, causing issues with:
- ANSI escape sequences: `"48;2;0;0;"` - semicolons tokenized as separators
- Any string containing Nushell syntax

**Example:**
```nu
ansi -e '48;2;0;0;'
```
The `;` inside the string may be incorrectly spaced.

**Status:** Parser limitation. Would need upstream fix in nu-parser.

---

## NUFMT-008: Fixture test boilerplate

**Category:** DRY Violation
**File:** `crates/nufmt-core/tests/fixtures.rs`

Each fixture test is a separate function that calls `test_fixture(name)`:
```rust
#[test]
fn test_fixture_simple() { test_fixture("simple"); }
#[test]
fn test_fixture_blocks() { test_fixture("blocks"); }
// ... 9 total
```

**Suggested fix:** Use a test macro or data-driven test approach:
```rust
macro_rules! fixture_test {
    ($($name:ident),* $(,)?) => {
        $(
            #[test]
            fn $name() { test_fixture(stringify!($name).trim_start_matches("test_fixture_")); }
        )*
    };
}
fixture_test!(simple, blocks, variables, comments, complex, empty, comments_only, deeply_nested, unicode);
```

---

## NUFMT-009: Engine state recreation

**Category:** Performance
**File:** `crates/nufmt-core/src/format.rs:19-22, 152-154, 197-198`

`create_engine_state()` is called for each format/debug operation, creating a new engine with all Nushell commands. This is expensive.

**Options:**
1. Use `OnceCell`/`LazyLock` for singleton engine state
2. Pass engine state as parameter (allows reuse across files)
3. Accept current behavior (correctness over performance)

Note: Engine state may not be thread-safe, which complicates caching.

---

## NUFMT-010: Stringly-typed bracket matching

**Category:** Code Smell
**File:** `crates/nufmt-core/src/format.rs`

Bracket matching uses string comparisons throughout:
```rust
if trimmed == "{" || trimmed == "[" { ... }
if trimmed.ends_with('}') || trimmed.ends_with(']') { ... }
```

Consider introducing an enum:
```rust
enum Delimiter { OpenBrace, CloseBrace, OpenBracket, CloseBracket }
```

---

## NUFMT-011: Error type could use `thiserror`

**Category:** Ergonomics
**Files:** `crates/nufmt-core/src/format.rs:34-106`, `crates/nufmt-cli/src/main.rs:665-706`

Both crates manually implement `Display` and `Error` for their error types. The `thiserror` crate would reduce boilerplate:

```rust
#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("{message}")]
    ParseError { message: String, ... }
}
```

---

## NUFMT-012: Color handling could use a crate

**Category:** Ergonomics
**File:** `crates/nufmt-cli/src/main.rs:93-100`

Manual ANSI codes:
```rust
mod color {
    pub const RESET: &str = "\x1b[0m";
    pub const GREEN: &str = "\x1b[32m";
    // ...
}
```

Consider `owo-colors` or `colored` for cleaner color handling and Windows support.

---

## NUFMT-013: `is_in_multiline_collection` lookback

**Category:** Complexity
**File:** `crates/nufmt-core/src/format.rs:955-974`

This function walks backward through all previous tokens to determine multiline context. This is O(n) for each comma in a collection.

**Options:**
1. Track multiline state in a stack during forward traversal
2. Accept current behavior (n is typically small)

---

## NUFMT-014: Test coverage for edge cases

**Category:** Testing
**Files:** Various

Areas with limited test coverage:
- `format_multi_close` with various brace combinations
- `format_structural_gap` with edge cases
- Error paths in CLI (glob failures, permission errors)
- Config file edge cases (unicode, empty file)

---

## NUFMT-015: Diff algorithm edge cases

**Category:** Bug Risk
**File:** `crates/nufmt-cli/src/main.rs:611-661`

The custom diff algorithm has tests for basic edge cases but may produce unexpected output for:
- Lines that appear multiple times
- Large differences where context overlaps
- Files with only whitespace differences

Tests added in main.rs:883-937 cover panic prevention but not output correctness.

---

## Priority Suggestions

**High impact, low effort:**
- NUFMT-006: Replace diff with `similar` crate
- NUFMT-008: Fixture test macro
- NUFMT-011: Add `thiserror`

**High impact, medium effort:**
- NUFMT-001: Split format.rs into modules
- NUFMT-003: Extract common estimation logic

**Low priority (working as designed):**
- NUFMT-002: CLI enum duplication (intentional separation)
- NUFMT-005: Manual indent tracking (necessary workaround)
- NUFMT-007: Parser limitation (upstream issue)
