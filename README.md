# nufmt

A code formatter for [Nushell](https://www.nushell.sh/).

> [!WARNING]
> This project was developed with AI assistance. Correctness is best-effort.
> It's a personal tool built to scratch my own itch, not a polished upstream
> contribution. Expect rough edges and shortcuts.

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
      --check            Check if files are formatted without modifying them
      --stdin            Read from stdin, write to stdout
  -c, --config <CONFIG>  Path to config file
  -h, --help             Print help
  -V, --version          Print version
```
