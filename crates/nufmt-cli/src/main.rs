use std::{
    fs,
    io::{self, Read, Write},
    path::PathBuf,
    process::ExitCode,
};

use clap::Parser;
use nufmt::{Config, FormatError, format_source};

/// A code formatter for Nushell
#[derive(Parser, Debug)]
#[command(name = "nufmt", version, about)]
struct Args {
    /// Files to format (reads from stdin if none provided)
    #[arg()]
    files: Vec<PathBuf>,

    /// Check if files are formatted without modifying them
    #[arg(long)]
    check: bool,

    /// Read from stdin, write to stdout
    #[arg(long)]
    stdin: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();

    if args.stdin || args.files.is_empty() {
        match format_stdin(&args) {
            Ok(needs_formatting) => {
                if args.check && needs_formatting {
                    return ExitCode::from(1);
                }
            }
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        let mut any_would_change = false;
        let mut any_error = false;

        for path in &args.files {
            match format_file(path, &args) {
                Ok(would_change) => {
                    if would_change {
                        any_would_change = true;
                    }
                }
                Err(e) => {
                    eprintln!("{}: {e}", path.display());
                    any_error = true;
                }
            }
        }

        if any_error {
            return ExitCode::from(2);
        }
        if args.check && any_would_change {
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}

fn format_stdin(args: &Args) -> Result<bool, Error> {
    let mut source = String::new();
    io::stdin().read_to_string(&mut source)?;

    let config = Config::default();
    let formatted = format_source(&source, &config)?;

    let would_change = source != formatted;

    if args.check {
        if would_change {
            print_diff("<stdin>", &source, &formatted);
        }
    } else {
        io::stdout().write_all(formatted.as_bytes())?;
    }

    Ok(would_change)
}

fn format_file(path: &PathBuf, args: &Args) -> Result<bool, Error> {
    let source = fs::read_to_string(path)?;
    let config = Config::default();
    let formatted = format_source(&source, &config)?;

    let would_change = source != formatted;

    if args.check {
        if would_change {
            print_diff(&path.display().to_string(), &source, &formatted);
        }
    } else if would_change {
        fs::write(path, &formatted)?;
    }

    Ok(would_change)
}

fn print_diff(name: &str, original: &str, formatted: &str) {
    eprintln!("--- {name}");
    eprintln!("+++ {name} (formatted)");

    let orig_lines: Vec<&str> = original.lines().collect();
    let fmt_lines: Vec<&str> = formatted.lines().collect();

    let mut i = 0;
    let mut j = 0;

    while i < orig_lines.len() || j < fmt_lines.len() {
        // Find next difference
        if i < orig_lines.len() && j < fmt_lines.len() && orig_lines[i] == fmt_lines[j] {
            i += 1;
            j += 1;
            continue;
        }

        // Found a difference - print context
        let context_start = i.saturating_sub(1);

        // Print context line before if available
        if context_start < i && context_start < orig_lines.len() {
            eprintln!("@@ -{} +{} @@", context_start + 1, j.saturating_sub(1) + 1);
            eprintln!(" {}", orig_lines[context_start]);
        } else {
            eprintln!("@@ -{} +{} @@", i + 1, j + 1);
        }

        // Print differing lines
        while i < orig_lines.len()
            && (j >= fmt_lines.len() || orig_lines[i] != fmt_lines[j])
            && (j >= fmt_lines.len()
                || i + 1 >= orig_lines.len()
                || orig_lines[i + 1] != fmt_lines[j])
        {
            eprintln!("-{}", orig_lines[i]);
            i += 1;
        }

        while j < fmt_lines.len()
            && (i >= orig_lines.len() || orig_lines[i] != fmt_lines[j])
            && (i >= orig_lines.len()
                || j + 1 >= fmt_lines.len()
                || orig_lines[i] != fmt_lines[j + 1])
        {
            eprintln!("+{}", fmt_lines[j]);
            j += 1;
        }
    }
    eprintln!();
}

#[derive(Debug)]
enum Error {
    Io(io::Error),
    Format(FormatError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Format(e) => write!(f, "{e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<FormatError> for Error {
    fn from(e: FormatError) -> Self {
        Self::Format(e)
    }
}
