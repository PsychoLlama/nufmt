# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
- Configuration via `.nufmt.toml`:
  - `indent_width` (1-16, default: 4)
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

[Unreleased]: https://github.com/psychollama/nufmt/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/psychollama/nufmt/releases/tag/v0.1.0
