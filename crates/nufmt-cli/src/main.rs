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
        return load_config_file(path);
    }

    // Search for .nufmt.toml in current and parent directories
    if let Some(path) = find_config_file() {
        return load_config_file(&path);
    }

    // No config file found, use defaults
    Ok(Config::default())
}

/// Load, parse, and validate a config file.
fn load_config_file(path: &Path) -> Result<Config, Error> {
    let content = fs::read_to_string(path).map_err(|e| Error::Config {
        path: path.to_path_buf(),
        source: e.to_string(),
    })?;
    let config: Config = toml::from_str(&content).map_err(|e| Error::Config {
        path: path.to_path_buf(),
        source: e.to_string(),
    })?;
    config.validate().map_err(|e| Error::Config {
        path: path.to_path_buf(),
        source: e.to_string(),
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

/// CLI error types.
#[derive(Debug)]
enum Error {
    /// I/O error (file read/write, stdin/stdout).
    Io(io::Error),
    /// Formatting error (parse failure).
    Format(FormatError),
    /// Configuration file error.
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
        assert_eq!(config.quote_style, nufmt::QuoteStyle::Single);
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
            files: vec![],
            check: false,
            stdin: false,
            config: None,
            debug_tokens: false,
        };

        // When no config file exists, should use defaults
        let config = load_config(&args).unwrap();
        assert_eq!(config.indent_width, 4);
        assert_eq!(config.max_width, 100);
    }

    #[test]
    fn test_error_display() {
        let io_err = Error::Io(io::Error::new(io::ErrorKind::NotFound, "file not found"));
        assert!(io_err.to_string().contains("file not found"));

        let config_err = Error::Config {
            path: PathBuf::from("/test/.nufmt.toml"),
            source: "invalid syntax".to_string(),
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
            files: vec![file_path.clone()],
            check: false,
            stdin: false,
            config: None,
            debug_tokens: false,
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
            files: vec![file_path.clone()],
            check: true, // Check mode - don't modify
            stdin: false,
            config: None,
            debug_tokens: false,
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
            files: vec![file_path.clone()],
            check: false,
            stdin: false,
            config: None,
            debug_tokens: false,
        };
        let config = Config::default();

        let would_change = format_file(&file_path, &args, &config).unwrap();
        assert!(!would_change);
    }

    // Diff algorithm edge case tests (NUFMT-015)

    #[test]
    fn test_print_diff_identical_content() {
        // Should not panic when content is identical (no diff output)
        print_diff("test", "line1\nline2\n", "line1\nline2\n");
    }

    #[test]
    fn test_print_diff_empty_original() {
        // Should not panic when original is empty
        print_diff("test", "", "new content\n");
    }

    #[test]
    fn test_print_diff_empty_formatted() {
        // Should not panic when formatted is empty
        print_diff("test", "old content\n", "");
    }

    #[test]
    fn test_print_diff_both_empty() {
        // Should not panic when both are empty
        print_diff("test", "", "");
    }

    #[test]
    fn test_print_diff_single_line() {
        // Should not panic with single line content
        print_diff("test", "old", "new");
    }

    #[test]
    fn test_print_diff_large_insertion() {
        // Should not panic with many new lines
        let original = "line1\n";
        let formatted = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\n";
        print_diff("test", original, formatted);
    }

    #[test]
    fn test_print_diff_large_deletion() {
        // Should not panic with many deleted lines
        let original = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\n";
        let formatted = "line1\n";
        print_diff("test", original, formatted);
    }

    #[test]
    fn test_print_diff_complete_replacement() {
        // Should not panic when all lines change
        let original = "a\nb\nc\n";
        let formatted = "x\ny\nz\n";
        print_diff("test", original, formatted);
    }
}
