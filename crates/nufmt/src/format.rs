use std::sync::Arc;

use nu_cmd_lang::create_default_context;
use nu_command::add_shell_command_context;
use nu_parser::{FlatShape, flatten_block, parse};
use nu_protocol::{Span, engine::StateWorkingSet};

use crate::{Config, QuoteStyle};

/// Create an engine state with all Nushell commands available for parsing.
fn create_engine_state() -> nu_protocol::engine::EngineState {
    let engine_state = create_default_context();
    add_shell_command_context(engine_state)
}

/// A source location (line and column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    /// 1-indexed line number.
    pub line: usize,
    /// 1-indexed column number.
    pub column: usize,
}

/// Errors that can occur during formatting.
#[derive(Debug)]
pub enum FormatError {
    /// The source code could not be parsed.
    ParseError {
        message: String,
        location: Option<SourceLocation>,
    },
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseError { message, location } => {
                if let Some(loc) = location {
                    write!(f, "{}:{}: {message}", loc.line, loc.column)
                } else {
                    write!(f, "{message}")
                }
            }
        }
    }
}

impl std::error::Error for FormatError {}

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

/// Debug token output for Nushell source code.
///
/// Returns a string showing how the parser tokenizes the source.
#[must_use]
pub fn debug_tokens(source: &str) -> String {
    use std::fmt::Write;

    let engine_state = create_engine_state();
    let mut working_set = StateWorkingSet::new(&engine_state);
    let block = parse(&mut working_set, None, source.as_bytes(), false);
    let flattened = flatten_block(&working_set, &block);

    let mut output = format!("Source: {source:?} (len={})\n\nTokens:\n", source.len());
    let mut last_end = 0;

    for (span, shape) in &flattened {
        if span.start > last_end && span.start <= source.len() {
            let gap = &source[last_end..span.start];
            if !gap.is_empty() {
                let _ = writeln!(output, "  GAP: {gap:?}");
            }
        }
        if span.start <= span.end && span.end <= source.len() {
            let token = &source[span.start..span.end];
            let _ = writeln!(
                output,
                "  {shape:?}: {token:?} ({}-{})",
                span.start, span.end
            );
        } else {
            let _ = writeln!(
                output,
                "  {shape:?}: <invalid span {}-{}>",
                span.start, span.end
            );
        }
        last_end = span.end;
    }

    if last_end < source.len() {
        let _ = writeln!(output, "  TRAILING: {:?}", &source[last_end..]);
    }

    output
}

/// Format Nushell source code.
///
/// Returns the formatted source code.
///
/// # Errors
///
/// Returns an error if the source code cannot be parsed.
pub fn format_source(source: &str, config: &Config) -> Result<String, FormatError> {
    let engine_state = create_engine_state();
    let mut working_set = StateWorkingSet::new(&engine_state);

    let block = parse(&mut working_set, None, source.as_bytes(), false);

    // Check for parse errors - report the first one with location
    if let Some(error) = working_set.parse_errors.first() {
        let span = error.span();
        let location = if span.start < source.len() {
            Some(offset_to_location(source, span.start))
        } else {
            None
        };
        return Err(FormatError::ParseError {
            message: error.to_string(),
            location,
        });
    }

    let formatted = format_block(&working_set, &block, source, config);
    Ok(formatted)
}

/// Stateful formatter that processes tokens and builds formatted output.
///
/// The formatter tracks indentation, line position, and processes tokens
/// sequentially to produce properly formatted Nushell source code.
struct Formatter<'a> {
    /// Original source code being formatted.
    source: &'a str,
    /// Formatting configuration.
    config: &'a Config,
    /// Accumulated output string.
    output: String,
    /// Current indentation level (number of indent units).
    indent_level: usize,
    /// Whether we're at the start of a line (need indentation).
    line_start: bool,
    /// Byte offset of the end of the last processed token.
    last_end: usize,
    /// Current line length in characters (for line breaking).
    current_line_len: usize,
}

impl<'a> Formatter<'a> {
    /// Create a new formatter for the given source code and configuration.
    ///
    /// Pre-allocates output buffer with estimated capacity based on source length.
    fn new(source: &'a str, config: &'a Config) -> Self {
        // Estimate output size: formatting typically adds ~10% for indentation
        let capacity = source.len() + source.len() / 10;
        Self {
            source,
            config,
            output: String::with_capacity(capacity),
            indent_level: 0,
            line_start: true,
            last_end: 0,
            current_line_len: 0,
        }
    }

    /// Format the source code using the provided flattened token list.
    ///
    /// Consumes the formatter and returns the formatted output string.
    fn format(mut self, flattened: &[(Span, FlatShape)], source_len: usize) -> String {
        for (span, shape) in flattened {
            self.process_token(*span, shape);
        }

        // Handle trailing content (comments after last token)
        if self.last_end < source_len {
            self.process_gap(source_len);
        }

        // Ensure trailing newline
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }

        self.output
    }

    /// Process a single token from the flattened AST.
    fn process_token(&mut self, span: Span, shape: &FlatShape) {
        if !self.is_valid_span(span) {
            return;
        }

        let token = &self.source[span.start..span.end];

        // Dispatch to specialized handlers based on token shape
        match shape {
            FlatShape::Block | FlatShape::Closure => {
                if self.try_process_block(token, span) {
                    return;
                }
                // Fall through to default handling for ( ) in interpolation
            }
            FlatShape::Pipe => {
                self.process_pipe_token(token, span);
                return;
            }
            FlatShape::Record | FlatShape::List => {
                self.process_gap(span.start);
                self.process_delimiter_token(token);
                self.last_end = span.end;
                return;
            }
            FlatShape::String => {
                self.process_gap(span.start);
                self.process_string_token(token, span);
                return;
            }
            _ => {}
        }

        // Default token handling
        self.process_gap(span.start);
        self.write_token(token, span);
    }

    /// Check if a span is valid and non-overlapping with previously processed content.
    const fn is_valid_span(&self, span: Span) -> bool {
        let is_overlapping = span.start < self.last_end;
        let is_invalid = span.start > span.end;
        !is_overlapping && !is_invalid
    }

    /// Try to process a block/closure token. Returns true if handled.
    fn try_process_block(&mut self, token: &'a str, span: Span) -> bool {
        let trimmed = token.trim();
        if trimmed.starts_with('{') || trimmed.ends_with('}') {
            self.process_gap(span.start);
            self.process_block_token(token);
            self.last_end = span.end;
            true
        } else {
            false
        }
    }

    /// Process a pipe token with proper spacing and line breaking.
    fn process_pipe_token(&mut self, token: &'a str, span: Span) {
        let gap = &self.source[self.last_end..span.start];
        let has_newline = gap.contains('\n');

        // " | " adds 3 characters - break line if needed
        let would_exceed = self.current_line_len + 3 > self.config.max_width;
        if (would_exceed || has_newline) && !self.line_start {
            self.push_newline();
        }

        if self.line_start {
            self.write_indent();
        } else if !self.output.ends_with(' ') {
            self.push_char(' ');
        }
        self.push_str(token);
        self.push_char(' ');
        self.line_start = false;
        self.last_end = span.end;
    }

    /// Process a string token with potential quote style conversion.
    fn process_string_token(&mut self, token: &str, span: Span) {
        if self.line_start {
            self.write_indent();
        }
        let converted = self.convert_string_quotes(token);
        self.push_str(&converted);
        self.line_start = false;
        self.last_end = span.end;
    }

    /// Write a token with standard formatting.
    fn write_token(&mut self, token: &str, span: Span) {
        if self.line_start {
            self.write_indent();
        }
        self.push_str(token);
        self.line_start = false;
        self.last_end = span.end;
    }

    /// Process the gap between the last token and the next token.
    ///
    /// Gaps may contain whitespace, comments, or punctuation like `=` or `;`.
    fn process_gap(&mut self, next_start: usize) {
        let gap = &self.source[self.last_end..next_start];
        let mut lines = gap.split('\n').peekable();
        let mut first_line = true;

        while let Some(line) = lines.next() {
            let trimmed = line.trim();
            let has_more_lines = lines.peek().is_some();

            if let Some(comment_start) = trimmed.find('#') {
                self.process_gap_comment(trimmed, comment_start, first_line, has_more_lines);
            } else if !trimmed.is_empty() {
                self.process_gap_content(line, trimmed, has_more_lines);
            } else {
                self.process_gap_empty(gap, first_line, has_more_lines);
            }

            first_line = false;
        }
    }

    /// Process a comment found in a gap.
    fn process_gap_comment(
        &mut self,
        trimmed: &str,
        comment_start: usize,
        first_line: bool,
        has_more_lines: bool,
    ) {
        let comment = &trimmed[comment_start..];

        if first_line && !self.line_start {
            // Inline comment on same line - add space before
            if !self.output.ends_with(' ') {
                self.push_char(' ');
            }
        } else if self.line_start {
            self.write_indent();
            self.line_start = false;
        }

        // Handle content before the comment (like = in let)
        let before_comment = trimmed[..comment_start].trim();
        if !before_comment.is_empty() {
            if !self.output.ends_with(' ') && !self.line_start {
                self.push_char(' ');
            }
            self.push_str(before_comment);
            self.push_char(' ');
        }

        self.push_str(comment);

        if has_more_lines {
            self.push_newline();
        }
    }

    /// Process non-comment content in a gap (e.g., `=`, `;`, `.`).
    fn process_gap_content(&mut self, line: &str, trimmed: &str, has_more_lines: bool) {
        // No space before punctuation that attaches to previous token
        let no_space_before =
            trimmed.starts_with(';') || trimmed.starts_with(',') || trimmed.starts_with('.');
        // No space after field access dots
        let no_space_after = trimmed == ".";

        if !no_space_before
            && !self.output.is_empty()
            && !self.line_start
            && !self.output.ends_with(' ')
        {
            self.push_char(' ');
        }
        if self.line_start {
            self.write_indent();
            self.line_start = false;
        }
        self.push_str(trimmed);

        // Add space after punctuation (except field access dots)
        if !no_space_after
            && (line.ends_with(' ') || line.ends_with('\t') || has_more_lines || trimmed == ";")
        {
            self.push_char(' ');
        }
    }

    /// Process an empty line in a gap (whitespace only).
    fn process_gap_empty(&mut self, gap: &str, first_line: bool, has_more_lines: bool) {
        if has_more_lines {
            // Empty line followed by more content
            self.push_newline();
        } else if first_line && !gap.is_empty() && !self.line_start {
            // Whitespace on a single line - add space if needed
            if !self.output.ends_with(' ') {
                self.push_char(' ');
            }
        }
        // Trailing empty part after last newline: do nothing (newline already emitted)
    }

    /// Process a block or closure token (contains `{` and/or `}`).
    fn process_block_token(&mut self, token: &str) {
        let trimmed = token.trim();
        let has_open = trimmed.starts_with('{');
        let has_close = trimmed.ends_with('}');

        let (params, inner) = parse_block_content(token, trimmed, has_open, has_close);

        if has_open {
            self.write_block_open(params, inner);
        }

        self.write_block_inner(inner);

        if has_close {
            self.write_block_close();
        }
    }

    /// Write the opening brace of a block with optional closure parameters.
    fn write_block_open(&mut self, params: Option<&str>, inner: &str) {
        if !self.line_start && !self.output.ends_with(' ') {
            self.push_char(' ');
        }
        if self.line_start {
            self.write_indent();
        }
        self.push_char('{');

        if let Some(p) = params {
            self.push_str(p);
        }

        self.indent_level += 1;

        // Check if there's meaningful content after the brace/params
        let first_line = inner.lines().next().unwrap_or("");
        let first_trimmed = first_line.trim();
        if first_trimmed.is_empty() || first_trimmed.starts_with('#') {
            self.push_newline();
        } else {
            self.push_char(' ');
            self.line_start = false;
        }
    }

    /// Write the inner content of a block.
    fn write_block_inner(&mut self, inner: &str) {
        for line in inner.lines() {
            let line_trimmed = line.trim();
            if line_trimmed.is_empty() {
                continue;
            }

            if self.line_start {
                self.write_indent();
            }
            self.push_str(line_trimmed);
            self.push_newline();
        }
    }

    /// Write the closing brace of a block.
    fn write_block_close(&mut self) {
        self.indent_level = self.indent_level.saturating_sub(1);

        if !self.output.ends_with('\n') {
            self.push_newline();
        }

        if self.line_start {
            self.write_indent();
        }
        self.push_char('}');
        self.line_start = false;
    }

    /// Process a record/list delimiter token and normalize its spacing.
    ///
    /// Handles brackets `{}[]`, colons `:`, and commas `,` with proper
    /// spacing normalization (e.g., `,  ` becomes `, `).
    fn process_delimiter_token(&mut self, token: &str) {
        let trimmed = token.trim();
        let has_newline = token.contains('\n');

        if self.line_start {
            self.write_indent();
        }

        // Handle opening brackets - may have trailing newline
        if trimmed == "{" || trimmed == "[" {
            self.push_str(trimmed);
            if has_newline {
                self.indent_level += 1;
                self.push_newline();
            } else {
                self.line_start = false;
            }
            return;
        }

        // Handle closing brackets - may have leading newline
        if trimmed == "}" || trimmed == "]" {
            if has_newline && !self.output.ends_with('\n') {
                self.indent_level = self.indent_level.saturating_sub(1);
                self.push_newline();
                self.write_indent();
            }
            self.push_str(trimmed);
            self.line_start = false;
            return;
        }

        // Handle colon in records - normalize to ": "
        if trimmed == ":" {
            self.push_str(": ");
            self.line_start = false;
            return;
        }

        // Handle comma - normalize to ", " or newline for multiline
        if trimmed == "," {
            if has_newline {
                self.push_newline();
            } else {
                self.push_str(", ");
                self.line_start = false;
            }
            return;
        }

        // Handle standalone newline (row separator in multiline records)
        if trimmed.is_empty() && has_newline {
            self.push_newline();
            return;
        }

        // Default: just write the trimmed token
        self.push_str(trimmed);
        if token.ends_with(' ') || token.ends_with('\t') {
            self.push_char(' ');
        }
        self.line_start = false;
    }

    /// Write indentation spaces for the current indent level.
    fn write_indent(&mut self) {
        let indent_size = self.indent_level * self.config.indent_width;
        for _ in 0..indent_size {
            self.output.push(' ');
        }
        self.current_line_len = indent_size;
    }

    /// Push a single character to the output, tracking line length.
    fn push_char(&mut self, c: char) {
        self.output.push(c);
        if c == '\n' {
            self.current_line_len = 0;
        } else {
            self.current_line_len += 1;
        }
    }

    /// Push a string to the output, tracking line length.
    fn push_str(&mut self, s: &str) {
        self.output.push_str(s);
        if let Some(last_newline) = s.rfind('\n') {
            self.current_line_len = s.len() - last_newline - 1;
        } else {
            self.current_line_len += s.len();
        }
    }

    /// Push a newline and reset line tracking state.
    fn push_newline(&mut self) {
        self.output.push('\n');
        self.current_line_len = 0;
        self.line_start = true;
    }

    /// Convert string quotes based on configured quote style.
    ///
    /// Returns the original string if:
    /// - Quote style is Preserve
    /// - String is already in preferred style
    /// - Conversion would require adding escapes
    fn convert_string_quotes(&self, token: &str) -> String {
        match self.config.quote_style {
            QuoteStyle::Preserve => token.to_string(),
            QuoteStyle::Double => to_double_quotes(token),
            QuoteStyle::Single => to_single_quotes(token),
        }
    }
}

/// Convert a string to double quotes if possible.
///
/// Returns the original string if already double-quoted, contains double quotes,
/// or contains backslashes (which would become escape sequences).
fn to_double_quotes(token: &str) -> String {
    // Already double-quoted
    if token.starts_with('"') {
        return token.to_string();
    }

    // Single-quoted string: 'content'
    // Nushell single quotes are raw strings with no escape sequences.
    if let Some(content) = token.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        // Can't convert if content contains double quotes
        if content.contains('"') {
            return token.to_string();
        }
        // Can't convert if content has backslashes (would become escapes in double quotes)
        if content.contains('\\') {
            return token.to_string();
        }
        return format!("\"{content}\"");
    }

    token.to_string()
}

/// Convert a string to single quotes if possible.
///
/// Returns the original string if already single-quoted, contains single quotes,
/// or contains escape sequences (which wouldn't work in single quotes).
fn to_single_quotes(token: &str) -> String {
    // Already single-quoted
    if token.starts_with('\'') {
        return token.to_string();
    }

    // Double-quoted string: "content"
    if let Some(content) = token.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        // Can't convert if content contains single quotes
        if content.contains('\'') {
            return token.to_string();
        }
        // Can't convert if content has escape sequences (they won't work in single quotes)
        if content.contains('\\') {
            return token.to_string();
        }
        return format!("'{content}'");
    }

    token.to_string()
}

/// Parse a block token to extract closure parameters and inner content.
///
/// For closures like `{|x, y| body}`, returns `(Some("|x, y|"), "body")`.
/// For regular blocks like `{ body }`, returns `(None, "body")`.
fn parse_block_content<'a>(
    token: &'a str,
    trimmed: &'a str,
    has_open: bool,
    has_close: bool,
) -> (Option<&'a str>, &'a str) {
    let (params, initial_inner) = if has_open {
        parse_closure_params(token)
    } else if has_close {
        (None, token.trim_end().strip_suffix('}').unwrap_or(""))
    } else {
        (None, "")
    };

    // For tokens with both braces, extract content between them
    let inner = if has_open && has_close {
        trimmed
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .unwrap_or("")
    } else {
        initial_inner
    };

    (params, inner)
}

/// Parse closure parameters from a block token.
///
/// Given `{|x, y| body`, returns `(Some("|x, y|"), " body")`.
/// Given `{ body`, returns `(None, " body")`.
fn parse_closure_params(token: &str) -> (Option<&str>, &str) {
    let after_brace = token.trim_start().strip_prefix('{').unwrap_or("");

    if !after_brace.trim_start().starts_with('|') {
        return (None, after_brace);
    }

    // Find the opening pipe
    let Some(first_pipe) = after_brace.find('|') else {
        return (None, after_brace);
    };

    // Find the closing pipe
    let rest = &after_brace[first_pipe + 1..];
    let Some(second_pipe) = rest.find('|') else {
        return (None, after_brace);
    };

    // Extract params: from first pipe to second pipe (inclusive)
    let params_end = first_pipe + 1 + second_pipe + 1;
    let params = &after_brace[..params_end];
    let inner = &after_brace[params_end..];

    (Some(params.trim()), inner)
}

/// Format a parsed block by flattening it to tokens and processing each one.
fn format_block(
    working_set: &StateWorkingSet,
    block: &Arc<nu_protocol::ast::Block>,
    source: &str,
    config: &Config,
) -> String {
    let flattened = flatten_block(working_set, block);
    let formatter = Formatter::new(source, config);
    formatter.format(&flattened, source.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_command() {
        let source = "ls";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "ls\n");
    }

    #[test]
    fn test_pipeline() {
        let source = "ls|sort-by name";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "ls | sort-by name\n");
    }

    #[test]
    fn test_trailing_newline() {
        let source = "echo hello";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert!(result.ends_with('\n'));
    }

    #[test]
    fn test_block_indentation() {
        let source = "if true {\necho hello\n}";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "if true {\n    echo hello\n}\n");
    }

    #[test]
    fn test_nested_blocks() {
        let source = "if true {\nif false {\necho nested\n}\n}";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert_eq!(
            result,
            "if true {\n    if false {\n        echo nested\n    }\n}\n"
        );
    }

    #[test]
    fn test_let_statement() {
        let source = "let x = 1 + 2";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "let x = 1 + 2\n");
    }

    #[test]
    fn test_operator_spacing() {
        // In Nushell, math expressions need spaces to be properly parsed
        // `1+2` is an external command, `1 + 2` is math
        let source = "let x = 1 + 2";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "let x = 1 + 2\n");
    }

    #[test]
    fn test_long_pipeline_break() {
        // Long pipeline should break at pipes when exceeding max_width
        let source = "ls | sort-by name | first 10 | reverse";
        let mut config = Config::default();
        config.max_width = 25;
        let result = format_source(source, &config).unwrap();
        // Should break into multiple lines (more than just trailing newline)
        let newline_count = result.chars().filter(|&c| c == '\n').count();
        assert!(
            newline_count > 1,
            "Expected multiple line breaks in: {result:?}"
        );
    }

    #[test]
    fn test_line_comment() {
        let source = "# this is a comment\nls";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "# this is a comment\nls\n");
    }

    #[test]
    fn test_inline_comment() {
        let source = "ls # list files";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "ls # list files\n");
    }

    #[test]
    fn test_comment_in_block() {
        let source = "if true {\n# inside block\necho hello\n}";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "if true {\n    # inside block\n    echo hello\n}\n");
    }

    #[test]
    fn test_record_spacing() {
        let source = "{a:1,  b:   2}";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "{a: 1, b: 2}\n");
    }

    #[test]
    fn test_list_spacing() {
        let source = "[1,  2,   3]";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "[1, 2, 3]\n");
    }

    #[test]
    fn test_multiline_record() {
        let source = "{\na: 1\nb: 2\n}";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "{\n    a: 1\n    b: 2\n}\n");
    }

    #[test]
    fn test_multiline_list() {
        let source = "[\n1\n2\n3\n]";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "[\n    1\n    2\n    3\n]\n");
    }

    #[test]
    fn test_closure_params() {
        let source = "{|x, y| $x + $y}";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert!(
            result.contains("|x, y|"),
            "Should preserve closure params: {result}"
        );
    }

    #[test]
    fn test_quote_style_single() {
        let source = r#"echo "hello""#;
        let mut config = Config::default();
        config.quote_style = QuoteStyle::Single;
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "echo 'hello'\n");
    }

    #[test]
    fn test_quote_style_double() {
        let source = "echo 'hello'";
        let mut config = Config::default();
        config.quote_style = QuoteStyle::Double;
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "echo \"hello\"\n");
    }

    #[test]
    fn test_quote_style_preserve_when_needed() {
        // Can't convert to single quotes if string contains single quote
        let source = r#"echo "it's""#;
        let mut config = Config::default();
        config.quote_style = QuoteStyle::Single;
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "echo \"it's\"\n");
    }

    // Quote conversion edge cases (NUFMT-016)

    #[test]
    fn test_quote_preserve_mode() {
        // Preserve mode should not change quotes
        let source = r#"echo "hello""#;
        let mut config = Config::default();
        config.quote_style = QuoteStyle::Preserve;
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "echo \"hello\"\n");
    }

    #[test]
    fn test_quote_preserve_single() {
        // Preserve mode should keep single quotes
        let source = "echo 'hello'";
        let mut config = Config::default();
        config.quote_style = QuoteStyle::Preserve;
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "echo 'hello'\n");
    }

    #[test]
    fn test_quote_double_with_double_quote_inside() {
        // Can't convert to double quotes if content contains double quotes
        let source = r#"echo 'say "hi"'"#;
        let mut config = Config::default();
        config.quote_style = QuoteStyle::Double;
        let result = format_source(source, &config).unwrap();
        // Should preserve single quotes
        assert_eq!(result, "echo 'say \"hi\"'\n");
    }

    #[test]
    fn test_quote_single_with_backslash() {
        // Can't convert to single if content has escapes
        let source = r#"echo "hello\nworld""#;
        let mut config = Config::default();
        config.quote_style = QuoteStyle::Single;
        let result = format_source(source, &config).unwrap();
        // Should preserve double quotes
        assert_eq!(result, "echo \"hello\\nworld\"\n");
    }

    #[test]
    fn test_quote_double_with_backslash() {
        // Can't convert to double if content has backslash (becomes escape)
        let source = r#"echo 'C:\path'"#;
        let mut config = Config::default();
        config.quote_style = QuoteStyle::Double;
        let result = format_source(source, &config).unwrap();
        // Should preserve single quotes
        assert_eq!(result, "echo 'C:\\path'\n");
    }

    #[test]
    fn test_quote_empty_string_double() {
        let source = r#"echo ''"#;
        let mut config = Config::default();
        config.quote_style = QuoteStyle::Double;
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "echo \"\"\n");
    }

    #[test]
    fn test_quote_empty_string_single() {
        let source = r#"echo """#;
        let mut config = Config::default();
        config.quote_style = QuoteStyle::Single;
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "echo ''\n");
    }

    #[test]
    fn test_quote_whitespace_only() {
        let source = r#"echo "   ""#;
        let mut config = Config::default();
        config.quote_style = QuoteStyle::Single;
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "echo '   '\n");
    }
}
