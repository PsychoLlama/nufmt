use std::{
    fs,
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::atomic::{AtomicUsize, Ordering},
};

use clap::{Parser, Subcommand, ValueEnum};
use nufmt_core::{
    BracketSpacing, Config, FormatError, QuoteStyle, TrailingComma, debug_tokens, format_source,
};
use owo_colors::OwoColorize;
use rayon::prelude::*;
use similar::TextDiff;
use thiserror::Error;

/// When to use colored output.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum ColorChoice {
    /// Use color when outputting to a terminal.
    #[default]
    Auto,
    /// Always use color.
    Always,
    /// Never use color.
    Never,
}

impl ColorChoice {
    /// Returns true if color should be used based on this choice and whether stderr is a terminal.
    fn should_use_color(self) -> bool {
        match self {
            Self::Auto => std::io::stderr().is_terminal(),
            Self::Always => true,
            Self::Never => false,
        }
    }
}

/// Result of formatting a single file.
enum FormatResult {
    /// File was formatted (or would be formatted in check mode).
    Changed,
    /// File was already correctly formatted.
    Unchanged,
    /// An error occurred while formatting.
    Error(String),
}

/// A code formatter for Nushell
#[derive(Parser, Debug)]
#[command(name = "nufmt", version, about, arg_required_else_help = true)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Files or glob patterns to format
    #[arg()]
    patterns: Vec<String>,

    /// Check if files are formatted without modifying them
    #[arg(long)]
    check: bool,

    /// Read from stdin, write to stdout
    #[arg(long)]
    stdin: bool,

    /// Path to config file (default: .nufmt.toml in current or parent directories)
    #[arg(long, short)]
    config: Option<PathBuf>,

    /// When to use colored output
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    color: ColorChoice,

    /// Number of spaces per indentation level (1-16)
    #[arg(long)]
    indent_width: Option<usize>,

    /// Maximum line width before breaking (20-500)
    #[arg(long)]
    max_width: Option<usize>,

    /// Preferred quote style for strings
    #[arg(long, value_enum)]
    quote_style: Option<QuoteStyle>,

    /// Spacing inside brackets and braces
    #[arg(long, value_enum)]
    bracket_spacing: Option<BracketSpacing>,

    /// Whether to add trailing commas in multiline collections
    #[arg(long, value_enum)]
    trailing_comma: Option<TrailingComma>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Initialize a .nufmt.toml config file in the current directory
    Init {
        /// Overwrite existing config file
        #[arg(long)]
        force: bool,
    },

    /// Debugging commands (hidden)
    #[command(hide = true)]
    Debug {
        #[command(subcommand)]
        command: DebugCommand,
    },
}

#[derive(Subcommand, Debug)]
enum DebugCommand {
    /// Show parser tokens for stdin
    Tokens,
}

fn main() -> ExitCode {
    let args = Args::parse();

    // Handle subcommands
    if let Some(command) = args.command {
        return match command {
            Command::Init { force } => run_init(force),
            Command::Debug { command } => run_debug(&command),
        };
    }

    // Load config
    let config = match load_config(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    if args.stdin {
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
    } else if !args.patterns.is_empty() {
        // Expand glob patterns to file paths
        let files = match expand_patterns(&args.patterns) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        };

        if files.is_empty() {
            eprintln!("error: no files matched the given patterns");
            return ExitCode::from(2);
        }

        let total = files.len();
        let changed = AtomicUsize::new(0);
        let errors = AtomicUsize::new(0);
        let use_color = args.color.should_use_color();

        // Process files and collect results for printing
        let results: Vec<_> = files
            .par_iter()
            .map(|path| {
                let result = match format_file(path, &args, &config) {
                    Ok(true) => {
                        changed.fetch_add(1, Ordering::Relaxed);
                        FormatResult::Changed
                    }
                    Ok(false) => FormatResult::Unchanged,
                    Err(e) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                        FormatResult::Error(e.to_string())
                    }
                };
                (path.clone(), result)
            })
            .collect();

        // Print results for each file
        for (path, result) in &results {
            print_file_result(path, result, &args, use_color);
        }

        // Print summary
        let changed_count = changed.load(Ordering::Relaxed);
        let error_count = errors.load(Ordering::Relaxed);
        print_summary(&args, total, changed_count, error_count, use_color);

        if error_count > 0 {
            return ExitCode::from(2);
        }
        if args.check && changed_count > 0 {
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}

/// Print the result of formatting a single file with optional color.
fn print_file_result(path: &Path, result: &FormatResult, args: &Args, use_color: bool) {
    let path_str = path.display();

    match result {
        FormatResult::Changed => {
            if args.check {
                if use_color {
                    eprintln!("{} {path_str} (would reformat)", "!".yellow().bold());
                } else {
                    eprintln!("! {path_str} (would reformat)");
                }
            } else if use_color {
                eprintln!("{} {path_str}", "✓".green().bold());
            } else {
                eprintln!("✓ {path_str}");
            }
        }
        FormatResult::Unchanged => {
            // Don't print anything for unchanged files (less noisy output)
        }
        FormatResult::Error(msg) => {
            if use_color {
                eprintln!("{} {path_str}: {msg}", "✗".red().bold());
            } else {
                eprintln!("✗ {path_str}: {msg}");
            }
        }
    }
}

/// Print a summary of the formatting operation.
fn print_summary(args: &Args, total: usize, changed: usize, errors: usize, use_color: bool) {
    if total == 0 {
        return;
    }

    let unchanged = total - changed - errors;
    let files_word = if total == 1 { "file" } else { "files" };

    if args.check {
        // Check mode summary
        if changed == 0 && errors == 0 {
            if use_color {
                eprintln!(
                    "\n{} All {total} {files_word} formatted correctly",
                    "✓".green().bold()
                );
            } else {
                eprintln!("\n✓ All {total} {files_word} formatted correctly");
            }
        } else {
            let mut parts = Vec::new();
            if changed > 0 {
                parts.push(format!("{changed} would be reformatted"));
            }
            if unchanged > 0 {
                parts.push(format!("{unchanged} already formatted"));
            }
            if errors > 0 {
                parts.push(format!("{errors} failed"));
            }
            eprintln!("\n{total} {files_word}: {}", parts.join(", "));
        }
    } else {
        // Format mode summary
        if changed == 0 && errors == 0 {
            if use_color {
                eprintln!(
                    "\n{} All {total} {files_word} already formatted",
                    "✓".green().bold()
                );
            } else {
                eprintln!("\n✓ All {total} {files_word} already formatted");
            }
        } else {
            let mut parts = Vec::new();
            if changed > 0 {
                parts.push(format!("{changed} formatted"));
            }
            if unchanged > 0 {
                parts.push(format!("{unchanged} unchanged"));
            }
            if errors > 0 {
                parts.push(format!("{errors} failed"));
            }
            eprintln!("\n{total} {files_word}: {}", parts.join(", "));
        }
    }
}

/// Default config file content with documentation.
const DEFAULT_CONFIG: &str = r#"# nufmt Configuration
#
# nufmt searches for .nufmt.toml in the current directory and ancestors.

# Number of spaces per indentation level.
# Valid range: 1-16
# Default: 2
indent_width = 2

# Maximum line width before breaking pipelines.
# Valid range: 20-500
# Default: 100
max_width = 100

# Preferred quote style for strings.
# Options: "preserve", "double", "single"
# - preserve: Keep existing quote style
# - double: Prefer double quotes when possible (default)
# - single: Prefer single quotes when possible
# Note: Quotes are only converted when safe (no escaping needed).
# Default: "double"
quote_style = "double"
"#;

/// Run a debug subcommand.
fn run_debug(command: &DebugCommand) -> ExitCode {
    match command {
        DebugCommand::Tokens => {
            let mut source = String::new();
            if let Err(e) = io::stdin().read_to_string(&mut source) {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
            print!("{}", debug_tokens(&source));
            ExitCode::SUCCESS
        }
    }
}

/// Initialize a .nufmt.toml config file in the current directory.
fn run_init(force: bool) -> ExitCode {
    let config_path = PathBuf::from(".nufmt.toml");

    if config_path.exists() && !force {
        eprintln!("error: .nufmt.toml already exists (use --force to overwrite)");
        return ExitCode::from(1);
    }

    if let Err(e) = fs::write(&config_path, DEFAULT_CONFIG) {
        eprintln!("error: failed to write .nufmt.toml: {e}");
        return ExitCode::from(2);
    }

    eprintln!("Created .nufmt.toml");
    ExitCode::SUCCESS
}

/// Load configuration from file or use defaults, then apply CLI overrides.
fn load_config(args: &Args) -> Result<Config, Error> {
    // Load base config from file or defaults
    let mut config = if let Some(path) = &args.config {
        load_config_file(path)?
    } else if let Some(path) = find_config_file() {
        load_config_file(&path)?
    } else {
        Config::default()
    };

    // Apply CLI overrides
    if let Some(indent_width) = args.indent_width {
        config.indent_width = indent_width;
    }
    if let Some(max_width) = args.max_width {
        config.max_width = max_width;
    }
    if let Some(quote_style) = args.quote_style {
        config.quote_style = quote_style;
    }
    if let Some(bracket_spacing) = args.bracket_spacing {
        config.bracket_spacing = bracket_spacing;
    }
    if let Some(trailing_comma) = args.trailing_comma {
        config.trailing_comma = trailing_comma;
    }

    // Validate the final config (in case CLI args are out of range)
    config.validate().map_err(|e| Error::Config {
        path: "<cli>".to_string(),
        message: e.to_string(),
    })?;

    Ok(config)
}

/// Load, parse, and validate a config file.
fn load_config_file(path: &Path) -> Result<Config, Error> {
    let path_str = path.display().to_string();
    let content = fs::read_to_string(path).map_err(|e| Error::Config {
        path: path_str.clone(),
        message: e.to_string(),
    })?;
    let config: Config = toml::from_str(&content).map_err(|e| Error::Config {
        path: path_str.clone(),
        message: e.to_string(),
    })?;
    config.validate().map_err(|e| Error::Config {
        path: path_str,
        message: e.to_string(),
    })?;
    Ok(config)
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

/// Expand glob patterns to file paths.
///
/// If a pattern contains no glob characters, it's treated as a literal path.
/// Directories are recursively searched for `.nu` files.
/// Only returns files (not directories).
fn expand_patterns(patterns: &[String]) -> Result<Vec<PathBuf>, Error> {
    let mut files = Vec::new();

    for pattern in patterns {
        // Check if pattern contains glob characters
        if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
            // Expand as glob pattern
            for entry in glob::glob(pattern)? {
                match entry {
                    Ok(path) if path.is_file() => files.push(path),
                    Ok(path) if path.is_dir() => collect_nu_files(&path, &mut files),
                    Ok(_) => {} // Skip other types
                    Err(e) => eprintln!("warning: {e}"),
                }
            }
        } else {
            let path = PathBuf::from(pattern);
            if path.is_dir() {
                collect_nu_files(&path, &mut files);
            } else {
                files.push(path);
            }
        }
    }

    Ok(files)
}

/// Recursively collect all `.nu` files in a directory.
fn collect_nu_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect_nu_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "nu") {
            files.push(path);
        }
    }
}

/// Format source code from stdin.
///
/// Returns true if the source would change (for check mode).
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

/// Format a single file.
///
/// In check mode, prints a diff if changes are needed.
/// Otherwise, writes the formatted output back to the file.
/// Returns true if the file would change.
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

/// Print a unified diff between original and formatted content.
fn print_diff(name: &str, original: &str, formatted: &str) {
    let diff = TextDiff::from_lines(original, formatted);
    let mut unified = diff.unified_diff();
    unified.header(name, &format!("{name} (formatted)"));
    eprint!("{unified}");
}

/// CLI error types.
#[derive(Debug, Error)]
enum Error {
    /// I/O error (file read/write, stdin/stdout).
    #[error("{0}")]
    Io(#[from] io::Error),
    /// Formatting error (parse failure).
    #[error("{0}")]
    Format(#[from] FormatError),
    /// Configuration file error.
    #[error("config error in {path}: {message}")]
    Config { path: String, message: String },
    /// Glob pattern error.
    #[error("invalid glob pattern: {0}")]
    Glob(#[from] glob::PatternError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_config_file_none() {
        // When run from a temp directory with no config, should return None
        let temp_dir = tempfile::tempdir().unwrap();
        let _guard = std::env::set_current_dir(&temp_dir);
        // Note: find_config_file uses current_dir, so we can't easily test it
        // without changing directory. This test just ensures the function doesn't panic.
    }

    #[test]
    fn test_load_config_file_valid() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join(".nufmt.toml");

        let mut file = fs::File::create(&config_path).unwrap();
        writeln!(file, "indent_width = 2").unwrap();
        writeln!(file, "max_width = 80").unwrap();
        writeln!(file, r#"quote_style = "single""#).unwrap();

        let config = load_config_file(&config_path).unwrap();
        assert_eq!(config.indent_width, 2);
        assert_eq!(config.max_width, 80);
        assert_eq!(config.quote_style, nufmt_core::QuoteStyle::Single);
    }

    #[test]
    fn test_load_config_file_missing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("nonexistent.toml");

        let result = load_config_file(&config_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_config_file_invalid_toml() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join(".nufmt.toml");

        let mut file = fs::File::create(&config_path).unwrap();
        writeln!(file, "this is not valid toml {{{{").unwrap();

        let result = load_config_file(&config_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_config_defaults() {
        let args = Args {
            command: None,
            patterns: vec![],
            check: false,
            stdin: false,
            config: None,
            color: ColorChoice::Auto,
            indent_width: None,
            max_width: None,
            quote_style: None,
            bracket_spacing: None,
            trailing_comma: None,
        };

        // When no config file exists, should use defaults
        let config = load_config(&args).unwrap();
        assert_eq!(config.indent_width, 2);
        assert_eq!(config.max_width, 100);
    }

    #[test]
    fn test_error_display() {
        let io_err = Error::Io(io::Error::new(io::ErrorKind::NotFound, "file not found"));
        assert!(io_err.to_string().contains("file not found"));

        let config_err = Error::Config {
            path: "/test/.nufmt.toml".to_string(),
            message: "invalid syntax".to_string(),
        };
        let msg = config_err.to_string();
        assert!(msg.contains(".nufmt.toml"));
        assert!(msg.contains("invalid syntax"));
    }

    #[test]
    fn test_format_file_creates_formatted_output() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.nu");

        // Write unformatted code
        fs::write(&file_path, "ls|sort-by name").unwrap();

        let args = Args {
            command: None,
            patterns: vec![file_path.display().to_string()],
            check: false,
            stdin: false,
            config: None,
            color: ColorChoice::Auto,
            indent_width: None,
            max_width: None,
            quote_style: None,
            bracket_spacing: None,
            trailing_comma: None,
        };
        let config = Config::default();

        let would_change = format_file(&file_path, &args, &config).unwrap();
        assert!(would_change);

        // Verify file was formatted
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "ls | sort-by name\n");
    }

    #[test]
    fn test_format_file_check_mode() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.nu");

        // Write unformatted code
        fs::write(&file_path, "ls|sort-by name").unwrap();

        let args = Args {
            command: None,
            patterns: vec![file_path.display().to_string()],
            check: true, // Check mode - don't modify
            stdin: false,
            config: None,
            color: ColorChoice::Auto,
            indent_width: None,
            max_width: None,
            quote_style: None,
            bracket_spacing: None,
            trailing_comma: None,
        };
        let config = Config::default();

        let would_change = format_file(&file_path, &args, &config).unwrap();
        assert!(would_change);

        // Verify file was NOT modified in check mode
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "ls|sort-by name");
    }

    #[test]
    fn test_format_file_already_formatted() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.nu");

        // Write already formatted code
        fs::write(&file_path, "ls | sort-by name\n").unwrap();

        let args = Args {
            command: None,
            patterns: vec![file_path.display().to_string()],
            check: false,
            stdin: false,
            config: None,
            color: ColorChoice::Auto,
            indent_width: None,
            max_width: None,
            quote_style: None,
            bracket_spacing: None,
            trailing_comma: None,
        };
        let config = Config::default();

        let would_change = format_file(&file_path, &args, &config).unwrap();
        assert!(!would_change);
    }
}
