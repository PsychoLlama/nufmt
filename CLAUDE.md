# nufmt

Nushell code formatter. See `ref/plan.md` for implementation plan.

## Commands

- `just` - list all commands
- `just check` - run all checks (fmt, lint, test)
- `just fmt` - check formatting (uses treefmt for Rust and Nix)
- `just lint` - run clippy
- `just test` - run tests

## Rules

- Run `just check` before every commit. All checks must pass.
- Update `CHANGELOG.md` when adding features or fixing bugs.

## Debugging

Hidden subcommands for debugging the formatter:

- `echo 'code' | nufmt debug tokens` - show parser tokens for stdin
