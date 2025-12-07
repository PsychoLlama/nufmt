# nufmt Implementation Plan

## Overview

A CLI tool that formats Nushell source files. Two modes: format in-place and check (exit non-zero if unformatted).

## Architecture

```
nufmt (library)
├── Parser integration (nu-parser)
├── AST traversal
├── Formatting rules
└── Output generation

nufmt-cli (binary)
├── File discovery
├── CLI arguments (clap)
└── Format/check modes
```

## Implementation Steps

### Phase 1: Core Library ✓

1. ✓ Add `nu-parser` dependency for parsing Nushell source
2. ✓ Create formatting context (tracks indentation, line width)
3. ✓ Implement AST visitor pattern for formatting nodes
4. ✓ Handle basic constructs:
   - Commands and pipelines
   - Blocks and closures
   - Lists and records
   - Comments

### Phase 2: Formatting Rules ✓

1. ✓ Indentation (spaces, configurable width)
2. ✓ Line breaking for long pipelines
3. ✓ Spacing around operators
4. ✓ Trailing newlines
5. ✓ Record/list spacing normalization

### Phase 3: CLI ✓

1. ✓ Argument parsing:
   - `nufmt <files...>` - format in place
   - `nufmt --check <files...>` - check only, exit 1 if changes needed
   - `--stdin` - read from stdin, write to stdout
2. File discovery (glob patterns, recursion) - future
3. ✓ Exit codes:
   - 0: success (or no changes needed)
   - 1: files would be reformatted (check mode)
   - 2: error (parse failure, IO error)

### Phase 4: Polish

1. ✓ Error messages with file/line context
2. ✓ Diff output in check mode
3. ✓ Config file support (`.nufmt.toml`)
4. ✓ Integration tests with fixture files

## Future Work

- Glob pattern support for file discovery

## Completed Beyond Original Plan

- ✓ Closure parameter handling (`{|x, y| ...}`)
- ✓ Multiline record/list formatting with indentation
- ✓ String interpolation preservation (`$"Hello, ($name)"`)
- ✓ Semicolon/comma spacing normalization
- ✓ Hidden `--debug-tokens` flag for development
- ✓ Full command support via `nu-command` (where, each, select, etc.)
- ✓ Field access dot spacing (`$env.PATH`)
