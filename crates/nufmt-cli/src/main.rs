use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::Parser;
use nufmt::{Config, FormatError, debug_tokens, format_source};

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

    /// Path to config file (default: .nufmt.toml in current or parent directories)
    #[arg(long, short)]
    config: Option<PathBuf>,

    /// Debug: show parser tokens instead of formatting
    #[arg(long, hide = true)]
    debug_tokens: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();

    // Handle debug tokens mode
    if args.debug_tokens {
        let mut source = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut source) {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
        print!("{}", debug_tokens(&source));
        return ExitCode::SUCCESS;
    }

    // Load config
    let config = match load_config(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    if args.stdin || args.files.is_empty() {
        match format_stdin(&args, &config) {
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
            match format_file(path, &args, &config) {
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

/// Load configuration from file or use defaults.
fn load_config(args: &Args) -> Result<Config, Error> {
    // If explicit config path provided, use it
    if let Some(path) = &args.config {
        let content = fs::read_to_string(path).map_err(|e| Error::Config {
            path: path.clone(),
            source: e.to_string(),
        })?;
        let config: Config = toml::from_str(&content).map_err(|e| Error::Config {
            path: path.clone(),
            source: e.to_string(),
        })?;
        return Ok(config);
    }

    // Search for .nufmt.toml in current and parent directories
    if let Some(path) = find_config_file() {
        let content = fs::read_to_string(&path).map_err(|e| Error::Config {
            path: path.clone(),
            source: e.to_string(),
        })?;
        let config: Config = toml::from_str(&content).map_err(|e| Error::Config {
            path,
            source: e.to_string(),
        })?;
        return Ok(config);
    }

    // No config file found, use defaults
    Ok(Config::default())
}

/// Search for .nufmt.toml in current directory and ancestors.
fn find_config_file() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let config_path = dir.join(".nufmt.toml");
        if config_path.exists() {
            return Some(config_path);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn format_stdin(args: &Args, config: &Config) -> Result<bool, Error> {
    let mut source = String::new();
    io::stdin().read_to_string(&mut source)?;

    let formatted = format_source(&source, config)?;

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

fn format_file(path: &Path, args: &Args, config: &Config) -> Result<bool, Error> {
    let source = fs::read_to_string(path)?;
    let formatted = format_source(&source, config)?;

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
    Config { path: PathBuf, source: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Format(e) => write!(f, "{e}"),
            Self::Config { path, source } => {
                write!(f, "config error in {}: {source}", path.display())
            }
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
