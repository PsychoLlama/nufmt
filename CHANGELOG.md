# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] - 2025-12-07

### Added

- `bracket_spacing` config option and `--bracket-spacing` CLI flag (spaced/compact)
  - `spaced` (default): `{ a: 1 }`, `[ 1, 2, 3 ]`
  - `compact`: `{a: 1}`, `[1, 2, 3]`

### Fixed

- Optional field accessors now stay attached to the field name (`name?` not `name ?`)
- Multiline parenthesized expressions now maintain proper indentation for pipelines

## [0.4.0] - 2025-12-07

### Added

- Nix overlay for easier integration into NixOS/home-manager configurations

### Fixed

- Formatting no longer fails on unknown commands (plugins, custom commands not available at parse time)
- Formatting no longer fails on type mismatches from unknown command output types

## [0.3.0] - 2025-12-07

### Added

- CLI flags for config options: `--indent-width`, `--max-width`, `--quote-style`

### Changed

- Improved error messages with source context, caret pointing to error location, and help text

### Fixed

- Formatting no longer fails on unresolved references (undefined variables, missing modules, etc.)
- `export def` signatures no longer break before `[`, which would produce invalid syntax

## [0.2.0] - 2025-12-07

### Added

- Directory formatting: pass a directory to recursively format all `.nu` files
- Colored output with `--color` flag (auto/always/never)
- Per-file status indicators (✓ formatted, ! would reformat, ✗ error)

## [0.1.0] - 2025-12-07

### Added

- Initial release of nufmt, a code formatter for Nushell
- Core formatting features:
  - Operator spacing (`a+b` → `a + b`)
  - Pipeline formatting with configurable line breaking
  - Block and closure indentation
  - Record and list formatting
  - Comment preservation
  - Trailing newline enforcement
  - Auto-breaking for long lines based on `max_width`:
    - Records and lists break to multiline when exceeding width
    - Closures and blocks break to multiline when exceeding width
    - Command arguments wrap with continuation indentation
- Configuration via `.nufmt.toml`:
  - `indent_width` (1-16, default: 2)
  - `max_width` (20-500, default: 100)
  - `quote_style` (preserve/double/single, default: double)
- CLI features:
  - Format files in-place: `nufmt *.nu`
  - Check mode: `nufmt --check`
  - Stdin/stdout: `nufmt --stdin`
  - Glob pattern support: `nufmt "**/*.nu"`
  - Config file discovery (searches parent directories)
  - Initialize config: `nufmt init`
  - Parallel file processing
  - Batch operation summary
- Parse error messages with source locations
- Nix flake for installation and development

[Unreleased]: https://github.com/psychollama/nufmt/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/psychollama/nufmt/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/psychollama/nufmt/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/psychollama/nufmt/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/psychollama/nufmt/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/psychollama/nufmt/releases/tag/v0.1.0
