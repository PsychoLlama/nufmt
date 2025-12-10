//! Error types for the formatter.

use thiserror::Error;

/// A source location (line and column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    /// 1-indexed line number.
    pub line: usize,
    /// 1-indexed column number.
    pub column: usize,
}

/// Errors that can occur during formatting.
#[derive(Debug, Error)]
pub enum FormatError {
    /// The source code could not be parsed.
    #[error("{}", format_parse_error(.message, .help, .location, .source_line))]
    ParseError {
        /// The error message.
        message: String,
        /// Optional help text for fixing the error.
        help: Option<String>,
        /// Source location of the error.
        location: Option<SourceLocation>,
        /// The source line containing the error.
        source_line: Option<String>,
    },
}

impl FormatError {
    /// Create a parse error with context from the source code.
    pub(crate) fn from_parse_error(error: &nu_protocol::ParseError, source: &str) -> Self {
        use miette::Diagnostic;

        let span = error.span();
        let location = if span.start < source.len() {
            Some(offset_to_location(source, span.start))
        } else {
            None
        };

        let source_line =
            location.map(|loc| source.lines().nth(loc.line - 1).unwrap_or("").to_string());

        let help = error.help().map(|h| h.to_string());

        Self::ParseError {
            message: error.to_string(),
            help,
            location,
            source_line,
        }
    }
}

/// Format a parse error with source context for display.
#[allow(clippy::ref_option)]
fn format_parse_error(
    message: &str,
    help: &Option<String>,
    location: &Option<SourceLocation>,
    source_line: &Option<String>,
) -> String {
    use std::fmt::Write;
    let mut output = String::new();

    if let Some(loc) = location {
        let _ = writeln!(output, "{}:{}: {message}", loc.line, loc.column);
    } else {
        let _ = writeln!(output, "{message}");
    }

    if let (Some(line), Some(loc)) = (source_line, location) {
        let _ = writeln!(output, "  |");
        let _ = writeln!(output, "{:>3} | {line}", loc.line);
        let _ = writeln!(output, "  | {:>width$}^", "", width = loc.column - 1);
    }

    if let Some(help_text) = help {
        let _ = write!(output, "  = help: {help_text}");
    }

    output.trim_end().to_string()
}

/// Compute line and column from a byte offset in source.
fn offset_to_location(source: &str, offset: usize) -> SourceLocation {
    let mut line = 1;
    let mut col = 1;

    for (i, c) in source.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }

    SourceLocation { line, column: col }
}
