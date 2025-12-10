# nufmt

> [!CAUTION]
> **Abandoned.** Use the official Nushell formatter: [`nufmt`](https://github.com/nushell/nufmt).
> This project was an experiment to see what Claude Opus 4.5 could build.

A code formatter for [Nushell](https://www.nushell.sh/).

## Installation

```sh
nix shell github:psychollama/nufmt
```

Pre-built binaries are also available on the [releases page](https://github.com/psychollama/nufmt/releases).

## Usage

Format files in place:

```sh
nufmt **/*.nu
```

Check formatting without modifying files (useful for CI):

```sh
nufmt --check **/*.nu
```

Format stdin and write to stdout:

```sh
echo 'def main [] { print "hello" }' | nufmt --stdin
```

### Configuration

Create a config file in your project root:

```sh
nufmt init
```

This generates `.nufmt.toml` with the available options:

```toml
# Number of spaces per indentation level (1-16)
indent_width = 2

# Maximum line width before breaking pipelines (20-500)
max_width = 100

# Quote style: "preserve", "double", or "single"
quote_style = "double"

# Spacing inside brackets/braces: "spaced" or "compact"
# spaced: { a: 1 }, [ 1, 2, 3 ]
# compact: {a: 1}, [1, 2, 3]
bracket_spacing = "spaced"

# Trailing commas in multiline collections: "always" or "never"
trailing_comma = "always"
```

The formatter searches for `.nufmt.toml` in the current directory and its ancestors. You can also specify a config file explicitly:

```sh
nufmt -c path/to/.nufmt.toml **/*.nu
```

## CLI Reference

```
Usage: nufmt [OPTIONS] [PATTERNS]... [COMMAND]

Commands:
  init  Initialize a .nufmt.toml config file in the current directory
  help  Print this message or the help of the given subcommand(s)

Arguments:
  [PATTERNS]...  Files or glob patterns to format

Options:
      --check                          Check if files are formatted without modifying them
      --stdin                          Read from stdin, write to stdout
  -c, --config <CONFIG>                Path to config file
      --color <COLOR>                  When to use colored output [default: auto] [values: auto, always, never]
      --indent-width <INDENT_WIDTH>    Number of spaces per indentation level (1-16)
      --max-width <MAX_WIDTH>          Maximum line width before breaking (20-500)
      --quote-style <QUOTE_STYLE>      Preferred quote style [values: preserve, double, single]
      --bracket-spacing <SPACING>      Spacing inside brackets [values: spaced, compact]
      --trailing-comma <TRAILING_COMMA> Trailing commas in multiline collections [values: always, never]
  -h, --help                           Print help
  -V, --version                        Print version
```
