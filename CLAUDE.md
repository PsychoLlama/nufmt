# nufmt

Nushell code formatter. See `ref/plan.md` for implementation plan.

## Commands

- `just` - list all commands
- `just check` - run all checks (fmt, lint, test)
- `just fmt` - check formatting (uses treefmt for Rust and Nix)
- `just lint` - run clippy
- `just test` - run tests
- `bin/release <version>` - create a release (e.g., `bin/release 0.2.0`)

## Rules

- Run `just check` before every commit. All checks must pass.
- Update `CHANGELOG.md` when adding features or fixing bugs.

## Releasing

1. Update `CHANGELOG.md`: move items from `[Unreleased]` to `[X.Y.Z] - YYYY-MM-DD`
2. Add comparison link at bottom: `[X.Y.Z]: https://github.com/psychollama/nufmt/compare/vPREV...vX.Y.Z`
3. Update `[Unreleased]` link to compare against new version
4. Bump version in `Cargo.toml` (`workspace.package.version`)
5. Commit: `git commit -am "Bump version to X.Y.Z"`
6. Push: `git push origin main`
7. Run: `bin/release X.Y.Z`

## Debugging

Hidden subcommands for debugging the formatter:

- `echo 'code' | nufmt debug tokens` - show parser tokens for stdin
