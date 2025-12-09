use std::sync::Arc;

use nu_cmd_lang::create_default_context;
use nu_command::add_shell_command_context;
use nu_parser::{FlatShape, flatten_block, parse};
use nu_protocol::{ParseError, Span, engine::StateWorkingSet};

use crate::{BracketSpacing, Config, QuoteStyle, TrailingComma};

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
    fn from_parse_error(error: &ParseError, source: &str) -> Self {
        use miette::Diagnostic;

        let span = error.span();
        let location = if span.start < source.len() {
            Some(offset_to_location(source, span.start))
        } else {
            None
        };

        // Extract the source line containing the error
        let source_line =
            location.map(|loc| source.lines().nth(loc.line - 1).unwrap_or("").to_string());

        // Get help text from the diagnostic if available
        let help = error.help().map(|h| h.to_string());

        Self::ParseError {
            message: error.to_string(),
            help,
            location,
            source_line,
        }
    }
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseError {
                message,
                help,
                location,
                source_line,
            } => {
                // Print error message with location
                if let Some(loc) = location {
                    writeln!(f, "{}:{}: {message}", loc.line, loc.column)?;
                } else {
                    writeln!(f, "{message}")?;
                }

                // Print source line with caret pointing to error
                if let (Some(line), Some(loc)) = (source_line, location) {
                    writeln!(f, "  |")?;
                    writeln!(f, "{:>3} | {line}", loc.line)?;
                    writeln!(f, "  | {:>width$}^", "", width = loc.column - 1)?;
                }

                // Print help text if available
                if let Some(help_text) = help {
                    write!(f, "  = help: {help_text}")?;
                }

                Ok(())
            }
        }
    }
}

impl std::error::Error for FormatError {}

/// Check if a parse error is a resolution error (module/file/command not found).
///
/// These errors don't indicate invalid syntax, just that a dependency
/// couldn't be resolved at parse time. The formatter can still process
/// the code since it's syntactically valid.
///
/// This includes:
/// - Module/file resolution errors (use, source, etc.)
/// - Unknown commands (plugins, custom commands not available at parse time)
/// - Extra positional arguments (subcommands like `from jsonl` when plugin unavailable)
/// - Type mismatches from unknown command output types (e.g., `| where` after unknown command)
const fn is_resolution_error(error: &ParseError) -> bool {
    matches!(
        error,
        ParseError::VariableNotFound(..)
            | ParseError::ModuleNotFound(..)
            | ParseError::ModuleOrOverlayNotFound(..)
            | ParseError::ActiveOverlayNotFound(..)
            | ParseError::ExportNotFound(..)
            | ParseError::FileNotFound(..)
            | ParseError::SourcedFileNotFound(..)
            | ParseError::RegisteredFileNotFound(..)
            | ParseError::PluginNotFound { .. }
            | ParseError::UnknownCommand(..)
            | ParseError::ExtraPositional(..)
            | ParseError::InputMismatch(..)
    )
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

    // Check for parse errors - report the first syntax error with location
    // Filter out module resolution errors since the code is still syntactically valid
    let syntax_error = working_set
        .parse_errors
        .iter()
        .find(|e| !is_resolution_error(e));
    if let Some(error) = syntax_error {
        return Err(FormatError::from_parse_error(error, source));
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
    /// Stack tracking whether each nested collection should be multiline.
    /// Each entry corresponds to a `{` or `[` we've opened.
    collection_multiline_stack: Vec<bool>,
    /// Stack tracking whether each nested block/closure should be multiline.
    block_multiline_stack: Vec<bool>,
    /// Stack tracking gap blocks (braces in gaps, like match blocks).
    /// Each entry is true if the block should be multiline.
    gap_block_stack: Vec<bool>,
    /// Depth of string interpolation nesting (don't break lines inside).
    string_interpolation_depth: usize,
    /// True if we just opened a multiline block (for inline comment handling).
    just_opened_multiline_block: bool,
    /// The flattened token list for lookahead.
    tokens: &'a [(Span, FlatShape)],
    /// Current token index.
    token_index: usize,
}

impl<'a> Formatter<'a> {
    /// Create a new formatter for the given source code and configuration.
    ///
    /// Pre-allocates output buffer with estimated capacity based on source length.
    fn new(source: &'a str, config: &'a Config, tokens: &'a [(Span, FlatShape)]) -> Self {
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
            collection_multiline_stack: Vec::new(),
            block_multiline_stack: Vec::new(),
            gap_block_stack: Vec::new(),
            string_interpolation_depth: 0,
            just_opened_multiline_block: false,
            tokens,
            token_index: 0,
        }
    }

    /// Format the source code using the provided flattened token list.
    ///
    /// Consumes the formatter and returns the formatted output string.
    fn format(mut self, source_len: usize) -> String {
        while self.token_index < self.tokens.len() {
            let (span, ref shape) = self.tokens[self.token_index];
            self.token_index += 1;
            self.process_token(span, shape);
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
            FlatShape::StringInterpolation => {
                // Track entry/exit from string interpolation
                // StringInterpolation tokens mark the start ($', $") and end (', ") of interpolations
                self.process_gap(span.start);
                if token.starts_with('$') {
                    // Starting an interpolation
                    self.string_interpolation_depth += 1;
                } else {
                    // Ending an interpolation
                    self.string_interpolation_depth =
                        self.string_interpolation_depth.saturating_sub(1);
                }
                self.write_token(token, span);
                return;
            }
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
                self.maybe_break_line_for_token(token, shape);
                self.process_string_token(token, span);
                return;
            }
            _ => {}
        }

        // Default token handling
        self.process_gap(span.start);
        // Check if we need to break the line before this token
        self.maybe_break_line_for_token(token, shape);
        self.write_token(token, span);
    }

    /// Check if we should break the line before writing a token.
    ///
    /// This is used for long command lines to wrap arguments when
    /// the line would exceed `max_width`.
    fn maybe_break_line_for_token(&mut self, token: &str, shape: &FlatShape) {
        // Only break if we're not at line start and would exceed max_width
        if self.line_start {
            return;
        }

        // Don't break inside string interpolations - would corrupt the string
        if self.string_interpolation_depth > 0 {
            return;
        }

        // Don't break before operators (looks weird)
        if matches!(shape, FlatShape::Operator) {
            return;
        }

        // Don't break before signatures - they must stay with the def command
        if matches!(shape, FlatShape::Signature) {
            return;
        }

        // Token length (space already added by process_gap)
        let token_len = token.len();

        // Would adding this token exceed max_width?
        if self.current_line_len + token_len > self.config.max_width {
            // Remove the trailing space before breaking
            if self.output.ends_with(' ') {
                self.output.pop();
                self.current_line_len = self.current_line_len.saturating_sub(1);
            }
            // Break the line and add continuation indentation
            self.push_newline();
            // Use double indent for continuation
            self.indent_level += 1;
            self.write_indent();
            self.indent_level -= 1;
            self.line_start = false;
        }
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
        } else if trimmed.starts_with('(') || trimmed.ends_with(')') {
            // Handle parenthesized expressions (e.g., spread expressions, subexpressions)
            self.process_gap(span.start);
            self.process_paren_block_token(token);
            self.last_end = span.end;
            true
        } else {
            false
        }
    }

    /// Process a parenthesized block token (subexpression or spread).
    fn process_paren_block_token(&mut self, token: &str) {
        let trimmed = token.trim();
        let has_open = trimmed.starts_with('(');
        let has_close = trimmed.ends_with(')');
        let has_newline = token.contains('\n');

        if has_open {
            if self.line_start {
                self.write_indent();
            }
            self.push_char('(');
            self.line_start = false;
            if has_newline {
                self.indent_level += 1;
                self.push_newline();
            }
        }

        if has_close {
            if has_newline && !self.output.ends_with('\n') {
                self.indent_level = self.indent_level.saturating_sub(1);
                self.push_newline();
                self.write_indent();
            } else if has_open && has_newline {
                // Single-token block like "(\n)" - already dedented
                self.indent_level = self.indent_level.saturating_sub(1);
                self.write_indent();
            }
            self.push_char(')');
            self.line_start = false;
        }
    }

    /// Process a pipe token with proper spacing and line breaking.
    fn process_pipe_token(&mut self, token: &'a str, span: Span) {
        let gap = &self.source[self.last_end..span.start];

        // Check for non-whitespace content before the pipe (e.g., "?" in $env.VAR?)
        // This content must be preserved before we handle the pipe.
        let gap_content: String = gap
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        if !gap_content.is_empty() {
            // Write any gap content (like "?") attached to the previous token
            self.push_str(&gap_content);
        }

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

        // Reset the flag after processing gap (only relevant for first line)
        self.just_opened_multiline_block = false;
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

        // Check if this is an inline comment that should stay on the block opening line
        // e.g., `{|| # comment` should stay as `{|| # comment` not become `{||\n  # comment`
        let keep_inline = first_line && self.just_opened_multiline_block;
        if keep_inline {
            // Undo the newline we pushed and put comment inline instead
            if self.output.ends_with('\n') {
                self.output.pop();
                self.current_line_len = self
                    .output
                    .rfind('\n')
                    .map_or(self.output.len(), |pos| self.output.len() - pos - 1);
                self.line_start = false;
            }
            self.push_char(' ');
            self.just_opened_multiline_block = false;
        } else if first_line && !self.line_start {
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

    /// Process non-comment content in a gap (e.g., `=`, `;`, `.`, `{`, `}`).
    fn process_gap_content(&mut self, line: &str, trimmed: &str, has_more_lines: bool) {
        // Handle opening brace in gap (like match blocks)
        if trimmed == "{" {
            if !self.output.is_empty() && !self.line_start && !self.output.ends_with(' ') {
                self.push_char(' ');
            }
            if self.line_start {
                self.write_indent();
            }
            self.push_char('{');
            // Track this as a gap block - multiline if followed by more content
            self.gap_block_stack.push(has_more_lines);
            if has_more_lines {
                self.indent_level += 1;
                self.push_newline();
            } else {
                self.push_char(' ');
                self.line_start = false;
            }
            return;
        }

        // Handle closing brace in gap
        if trimmed == "}" || trimmed == ",}" || trimmed.ends_with('}') {
            // Count closing braces in this token
            let close_count = trimmed.chars().filter(|&c| c == '}').count();
            // Handle any content before the closing brace(s)
            let before_braces = trimmed.trim_end_matches('}').trim_end_matches(',').trim();
            if !before_braces.is_empty() {
                if !self.output.is_empty() && !self.line_start && !self.output.ends_with(' ') {
                    self.push_char(' ');
                }
                if self.line_start {
                    self.write_indent();
                }
                self.push_str(before_braces);
            }
            // Handle trailing comma before closing brace
            if trimmed.contains(',')
                && self.config.trailing_comma == TrailingComma::Always
                && !self.output.ends_with(',')
            {
                self.push_char(',');
            }
            // Close each brace
            for _ in 0..close_count {
                let is_multiline = self.gap_block_stack.pop().unwrap_or(false);
                if is_multiline {
                    self.indent_level = self.indent_level.saturating_sub(1);
                    if !self.output.ends_with('\n') {
                        self.push_newline();
                    }
                    if self.line_start {
                        self.write_indent();
                    }
                } else if !self.output.ends_with(' ') {
                    self.push_char(' ');
                }
                self.push_char('}');
                self.line_start = false;
            }
            if has_more_lines {
                self.push_newline();
            }
            return;
        }

        // Handle comma in gap when inside multiline gap block
        if trimmed == "," && has_more_lines && self.is_in_multiline_gap_block() {
            self.push_char(',');
            self.push_newline();
            return;
        }

        // No space before punctuation that attaches to previous token
        let no_space_before = trimmed.starts_with(';')
            || trimmed.starts_with(',')
            || trimmed.starts_with('.')
            || trimmed.starts_with('?'); // Optional accessor (name?)
        // No space after field access dots (but ? should have space after for next argument)
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

    /// Check if we're inside a multiline gap block.
    fn is_in_multiline_gap_block(&self) -> bool {
        self.gap_block_stack.last().copied().unwrap_or(false)
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

        // Check if the raw token contains a newline (indicates multiline intent)
        let token_has_newline = token.contains('\n');

        let (params, inner) = parse_block_content(token, trimmed, has_open, has_close);

        let skip_first_line = if has_open {
            self.write_block_open(params, inner, has_close, token_has_newline)
        } else {
            false
        };

        // If we already wrote the first line comment inline, skip it in write_block_inner
        let inner_to_write = if skip_first_line {
            inner.lines().skip(1).collect::<Vec<_>>().join("\n")
        } else {
            inner.to_string()
        };
        self.write_block_inner(&inner_to_write);

        if has_close {
            self.write_block_close();
        }
    }

    /// Calculate the single-line length of a block/closure.
    fn calculate_block_length(params: Option<&str>, inner: &str) -> usize {
        let mut length = 2; // "{ " and " }"

        // Add params length if present
        if let Some(p) = params {
            length += p.len();
        }

        // Calculate inner content on single line (spaces between elements)
        let inner_content: String = inner
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        length += inner_content.len();

        length
    }

    /// Calculate the single-line length of a block/closure by looking ahead in tokens.
    ///
    /// This is needed because Nushell's parser splits closures into multiple tokens:
    /// `{|x| $x + 1}` becomes: `{|x| `, `$x`, `+`, `1`, `}`
    fn calculate_block_length_from_tokens(&self) -> usize {
        let mut length = 0;
        let mut depth = 1;
        let mut idx = self.token_index;
        let mut prev_end = self.last_end;

        while idx < self.tokens.len() && depth > 0 {
            let (span, ref shape) = self.tokens[idx];
            if span.start <= span.end && span.end <= self.source.len() {
                // Add gap length (normalized to single space)
                if span.start > prev_end {
                    let gap = self.source[prev_end..span.start].trim();
                    if gap.is_empty() {
                        length += 1; // just a space
                    } else {
                        length += gap.len() + 1; // content + space
                    }
                }

                let token = self.source[span.start..span.end].trim();

                match shape {
                    FlatShape::Block | FlatShape::Closure => {
                        if token.starts_with('{') {
                            depth += 1;
                        }
                        if token.ends_with('}') {
                            depth -= 1;
                        }
                    }
                    _ => {}
                }

                length += token.len();
                prev_end = span.end;
            }
            idx += 1;
        }

        length + 1 // Add space before closing brace
    }

    /// Write the opening of a block/closure.
    ///
    /// Returns true if the first line (a comment) was written inline.
    fn write_block_open(
        &mut self,
        params: Option<&str>,
        inner: &str,
        has_close: bool,
        token_has_newline: bool,
    ) -> bool {
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

        // Check if content already has newlines (was multiline in source)
        let source_is_multiline = inner.contains('\n') && inner.lines().count() > 1;

        // Calculate if single-line would exceed max_width
        // For complete blocks (has_close=true), calculate from inner content
        // For split blocks (has_close=false), look ahead in token stream
        let block_length = if has_close {
            Self::calculate_block_length(params, inner)
        } else {
            // Closure split across tokens - look ahead
            self.calculate_block_length_from_tokens()
        };
        let would_exceed = self.current_line_len + block_length > self.config.max_width;

        // Check if first line is an inline comment (should stay on same line as brace)
        let first_line_is_comment = first_trimmed.starts_with('#');

        // For split blocks, empty inner is expected (content comes later)
        // So we don't use first_trimmed.is_empty() as a signal for multiline
        // But we DO check if the token itself contains a newline (like "{\n")
        let force_multiline = if has_close {
            first_trimmed.is_empty() || source_is_multiline || would_exceed
        } else {
            // Split block - check token newline, or would exceed
            token_has_newline || source_is_multiline || would_exceed
        };

        // Push to stack for tracking
        self.block_multiline_stack.push(force_multiline);

        if first_line_is_comment && source_is_multiline {
            // First line is a comment and there's more content - keep comment inline
            self.push_char(' ');
            self.push_str(first_trimmed);
            self.push_newline();
            self.line_start = true;
            return true; // Signal that we already wrote the first line
        } else if force_multiline {
            // Track that we just opened a multiline block - next gap's inline comment should stay inline
            self.just_opened_multiline_block = true;
            self.push_newline();
        } else {
            self.push_char(' ');
            self.line_start = false;
        }
        false
    }

    /// Write the inner content of a block.
    fn write_block_inner(&mut self, inner: &str) {
        for line in inner.lines() {
            let line_trimmed = line.trim();
            if line_trimmed.is_empty() {
                continue;
            }

            // Handle closing brace that belongs to a gap block
            if line_trimmed == "}" && !self.gap_block_stack.is_empty() {
                let is_multiline = self.gap_block_stack.pop().unwrap_or(false);
                if is_multiline {
                    self.indent_level = self.indent_level.saturating_sub(1);
                    if !self.output.ends_with('\n') {
                        self.push_newline();
                    }
                    if self.line_start {
                        self.write_indent();
                    }
                } else if !self.output.ends_with(' ') {
                    self.push_char(' ');
                }
                self.push_char('}');
                self.push_newline();
                continue;
            }

            if self.line_start {
                self.write_indent();
            }
            self.push_str(line_trimmed);

            // Don't emit newline after external command operator (^)
            // It must stay attached to the command name
            if line_trimmed == "^" {
                self.line_start = false;
            } else {
                self.push_newline();
            }
        }
    }

    /// Write the closing brace of a block.
    fn write_block_close(&mut self) {
        self.indent_level = self.indent_level.saturating_sub(1);

        // Check if this block was marked as multiline from split block tracking
        let was_multiline = self.block_multiline_stack.pop().unwrap_or(true);

        if was_multiline {
            if !self.output.ends_with('\n') {
                self.push_newline();
            }

            if self.line_start {
                self.write_indent();
            }
        } else {
            // Single-line block - just add space before closing brace
            if !self.output.ends_with(' ') {
                self.push_char(' ');
            }
        }
        self.push_char('}');
        self.line_start = false;
    }

    /// Calculate the single-line length of a collection starting at the current position.
    ///
    /// Scans from the current token index to find the matching closing bracket
    /// and estimates the single-line formatted length.
    fn calculate_collection_length(&self, _open_bracket: &str) -> usize {
        let mut length = 1; // Opening bracket
        let mut depth = 1;
        let mut idx = self.token_index;
        let mut prev_was_colon = false;
        let mut prev_was_value = false;

        while idx < self.tokens.len() && depth > 0 {
            let (span, shape) = &self.tokens[idx];
            if span.start <= span.end && span.end <= self.source.len() {
                let token = self.source[span.start..span.end].trim();

                match *shape {
                    FlatShape::Record | FlatShape::List => {
                        if token == "{" || token == "[" {
                            depth += 1;
                            length += 1;
                            prev_was_value = false;
                        } else if token == "}" || token == "]" {
                            depth -= 1;
                            if depth > 0 {
                                length += 1;
                            }
                            prev_was_value = true;
                        } else if token == ":" {
                            length += 2; // ": "
                            prev_was_colon = true;
                            prev_was_value = false;
                        } else if token == "," {
                            length += 2; // ", "
                            prev_was_value = false;
                        } else if !token.is_empty() {
                            // A value
                            if prev_was_value && !prev_was_colon {
                                length += 1; // space between values
                            }
                            length += token.len();
                            prev_was_colon = false;
                            prev_was_value = true;
                        }
                    }
                    _ => {
                        // Other tokens (values)
                        if prev_was_value && !prev_was_colon {
                            length += 1; // space between values
                        }
                        length += token.len();
                        prev_was_colon = false;
                        prev_was_value = true;
                    }
                }
            }
            idx += 1;
        }

        length + 1 // Closing bracket
    }

    /// Check if the current collection should be multiline.
    fn should_be_multiline(&self) -> bool {
        self.collection_multiline_stack
            .last()
            .copied()
            .unwrap_or(false)
    }

    /// Process a record/list delimiter token and normalize its spacing.
    ///
    /// Handles brackets `{}[]`, colons `:`, and commas `,` with proper
    /// spacing normalization (e.g., `,  ` becomes `, `).
    #[allow(clippy::too_many_lines)]
    fn process_delimiter_token(&mut self, token: &str) {
        let trimmed = token.trim();
        let has_newline = token.contains('\n');

        if self.line_start {
            self.write_indent();
        }

        // Handle opening brackets - may have trailing newline
        if trimmed == "{" || trimmed == "[" {
            // Calculate if this collection should be multiline
            let collection_len = self.calculate_collection_length(trimmed);
            let would_exceed = self.current_line_len + collection_len > self.config.max_width;
            let force_multiline = has_newline || would_exceed;

            self.collection_multiline_stack.push(force_multiline);
            self.push_str(trimmed);

            if force_multiline {
                self.indent_level += 1;
                self.push_newline();
            } else if self.config.bracket_spacing == BracketSpacing::Spaced {
                self.push_char(' ');
                self.line_start = false;
            } else {
                self.line_start = false;
            }
            return;
        }

        // Handle closing brackets - may have leading newline
        if trimmed == "}" || trimmed == "]" {
            let was_multiline = self.collection_multiline_stack.pop().unwrap_or(false);
            if was_multiline {
                // Handle trailing comma for multiline collections
                self.maybe_add_trailing_comma();

                if !self.output.ends_with('\n') {
                    self.indent_level = self.indent_level.saturating_sub(1);
                    self.push_newline();
                    self.write_indent();
                }
            } else if self.config.bracket_spacing == BracketSpacing::Spaced {
                // Add space before closing bracket for single-line collections
                if !self.output.ends_with(' ') {
                    self.push_char(' ');
                }
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
        // Note: token may be ",\n..." (comma with trailing newline) - we handle
        // the comma here and let the newline handling below take care of the rest.
        if trimmed == "," {
            if self.should_be_multiline() {
                // In multiline mode, write comma then newline
                self.push_char(',');
                self.push_newline();
            } else {
                self.push_str(", ");
                self.line_start = false;
            }
            return;
        }

        // Handle newline row separators (may include comments)
        if has_newline {
            // Check if there's a comment in the token
            if let Some(comment_start) = token.find('#') {
                let comment_end = token[comment_start..]
                    .find('\n')
                    .map_or(token.len(), |p| comment_start + p);
                let comment = token[comment_start..comment_end].trim_end();

                if self.should_be_multiline() {
                    // Add trailing comma before comment if configured
                    if self.config.trailing_comma == TrailingComma::Always {
                        let output_trimmed = self.output.trim_end();
                        if !output_trimmed.ends_with(',')
                            && !output_trimmed.ends_with('[')
                            && !output_trimmed.ends_with('{')
                        {
                            self.push_char(',');
                        }
                    }
                    // Add space before comment if needed
                    if !self.output.ends_with(' ') {
                        self.push_char(' ');
                    }
                    self.push_str(comment);
                    self.push_newline();
                }
                return;
            }

            // Standalone newline without comment
            if trimmed.is_empty() {
                if self.should_be_multiline() {
                    // Add trailing comma after each item if configured
                    if self.config.trailing_comma == TrailingComma::Always {
                        let output_trimmed = self.output.trim_end();
                        if !output_trimmed.ends_with(',')
                            && !output_trimmed.ends_with('[')
                            && !output_trimmed.ends_with('{')
                        {
                            self.push_char(',');
                        }
                    }
                    self.push_newline();
                }
                return;
            }
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

    /// Add a trailing comma if configured and not already present.
    ///
    /// This handles the case where the output might end with a comment,
    /// inserting the comma before the comment if needed.
    fn maybe_add_trailing_comma(&mut self) {
        if self.config.trailing_comma == TrailingComma::Never {
            return;
        }

        // Find the last non-whitespace, non-comment content
        let trimmed = self.output.trim_end();

        // Already has a trailing comma
        if trimmed.ends_with(',') {
            return;
        }

        // Check if the line ends with a comment
        if let Some(last_newline) = self.output.rfind('\n') {
            let last_line = &self.output[last_newline + 1..];
            if let Some(comment_start) = last_line.find('#') {
                // Insert comma before the comment
                let insert_pos = last_newline + 1 + comment_start;
                // Find where to insert (skip any whitespace before comment)
                let before_comment = &self.output[..insert_pos];
                let trimmed_before = before_comment.trim_end();
                if !trimmed_before.ends_with(',') {
                    let comma_pos = trimmed_before.len();
                    self.output.insert(comma_pos, ',');
                    self.current_line_len += 1;
                }
                return;
            }
        }

        // No comment, just append comma at the end (before any trailing whitespace)
        let trimmed_len = trimmed.len();
        self.output.truncate(trimmed_len);
        self.output.push(',');
        self.current_line_len = self
            .output
            .rfind('\n')
            .map_or(self.output.len(), |pos| self.output.len() - pos - 1);
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
    let formatter = Formatter::new(source, config, &flattened);
    formatter.format(source.len())
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
        assert_eq!(result, "if true {\n  echo hello\n}\n");
    }

    #[test]
    fn test_nested_blocks() {
        let source = "if true {\nif false {\necho nested\n}\n}";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "if true {\n  if false {\n    echo nested\n  }\n}\n");
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
        assert_eq!(result, "if true {\n  # inside block\n  echo hello\n}\n");
    }

    #[test]
    fn test_record_spacing() {
        let source = "{a:1,  b:   2}";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "{ a: 1, b: 2 }\n");
    }

    #[test]
    fn test_list_spacing() {
        let source = "[1,  2,   3]";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "[ 1, 2, 3 ]\n");
    }

    #[test]
    fn test_record_compact_spacing() {
        let source = "{ a: 1, b: 2 }";
        let mut config = Config::default();
        config.bracket_spacing = BracketSpacing::Compact;
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "{a: 1, b: 2}\n");
    }

    #[test]
    fn test_list_compact_spacing() {
        let source = "[ 1, 2, 3 ]";
        let mut config = Config::default();
        config.bracket_spacing = BracketSpacing::Compact;
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "[1, 2, 3]\n");
    }

    #[test]
    fn test_multiline_record() {
        let source = "{\na: 1\nb: 2\n}";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "{\n  a: 1,\n  b: 2,\n}\n");
    }

    #[test]
    fn test_multiline_list() {
        let source = "[\n1\n2\n3\n]";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "[\n  1,\n  2,\n  3,\n]\n");
    }

    #[test]
    fn test_multiline_list_no_trailing_comma() {
        let source = "[\n1\n2\n3\n]";
        let config = Config {
            trailing_comma: TrailingComma::Never,
            ..Default::default()
        };
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "[\n  1\n  2\n  3\n]\n");
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

    // Auto-breaking tests for records and lists

    #[test]
    fn test_long_record_auto_break() {
        // Long record should break to multiline when exceeding max_width
        let source = "{name: \"test\", value: 42, description: \"a long description\"}";
        let mut config = Config::default();
        config.max_width = 40;
        let result = format_source(source, &config).unwrap();
        // Should break into multiple lines
        let newline_count = result.chars().filter(|&c| c == '\n').count();
        assert!(
            newline_count > 1,
            "Expected multiline record in: {result:?}"
        );
        // Should have proper indentation
        assert!(
            result.contains("\n  "),
            "Expected indentation in: {result:?}"
        );
    }

    #[test]
    fn test_short_record_stays_single_line() {
        // Short record should stay on one line
        let source = "{a: 1, b: 2}";
        let config = Config::default(); // max_width = 100
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "{ a: 1, b: 2 }\n");
    }

    #[test]
    fn test_long_list_auto_break() {
        // Long list should break to multiline when exceeding max_width
        let source = "[\"item1\", \"item2\", \"item3\", \"item4\", \"item5\", \"item6\"]";
        let mut config = Config::default();
        config.max_width = 30;
        let result = format_source(source, &config).unwrap();
        // Should break into multiple lines
        let newline_count = result.chars().filter(|&c| c == '\n').count();
        assert!(newline_count > 1, "Expected multiline list in: {result:?}");
    }

    #[test]
    fn test_short_list_stays_single_line() {
        // Short list should stay on one line
        let source = "[1, 2, 3]";
        let config = Config::default(); // max_width = 100
        let result = format_source(source, &config).unwrap();
        assert_eq!(result, "[ 1, 2, 3 ]\n");
    }

    #[test]
    fn test_nested_record_auto_break() {
        // Nested record where outer exceeds max_width
        let source = "{outer: {inner: \"value\", num: 42}, other: \"data\"}";
        let mut config = Config::default();
        config.max_width = 35;
        let result = format_source(source, &config).unwrap();
        // Should break outer record
        let newline_count = result.chars().filter(|&c| c == '\n').count();
        assert!(
            newline_count > 1,
            "Expected multiline record in: {result:?}"
        );
    }

    #[test]
    fn test_long_closure_auto_break() {
        // Long closure should break to multiline when exceeding max_width
        let source = "{|x, y| $x + $y + $x * $y + $x - $y}";
        let mut config = Config::default();
        config.max_width = 25;
        let result = format_source(source, &config).unwrap();
        // Should break into multiple lines
        let newline_count = result.chars().filter(|&c| c == '\n').count();
        assert!(
            newline_count > 1,
            "Expected multiline closure in: {result:?}"
        );
    }

    #[test]
    fn test_short_closure_stays_single_line() {
        // Short closure should stay on one line
        let source = "{|x| $x + 1}";
        let config = Config::default(); // max_width = 100
        let result = format_source(source, &config).unwrap();
        // Should be on one line (only trailing newline)
        let newline_count = result.chars().filter(|&c| c == '\n').count();
        assert_eq!(
            newline_count, 1,
            "Expected single line closure in: {result:?}"
        );
    }

    #[test]
    fn test_long_block_auto_break() {
        // Long if block should break when exceeding max_width
        let source =
            "if true { echo \"this is a really long message that should cause wrapping\" }";
        let mut config = Config::default();
        config.max_width = 40;
        let result = format_source(source, &config).unwrap();
        // Should break into multiple lines
        let newline_count = result.chars().filter(|&c| c == '\n').count();
        assert!(newline_count > 1, "Expected multiline block in: {result:?}");
    }

    #[test]
    fn test_long_command_parameter_break() {
        // Long command should break when parameters exceed max_width
        let source = "open some-file.txt | get column1 column2 column3 column4 column5";
        let mut config = Config::default();
        config.max_width = 40;
        let result = format_source(source, &config).unwrap();
        // Should break into multiple lines
        let newline_count = result.chars().filter(|&c| c == '\n').count();
        assert!(
            newline_count > 1,
            "Expected multiline command in: {result:?}"
        );
    }

    #[test]
    fn test_short_command_stays_single_line() {
        // Short command should stay on one line
        let source = "ls -la";
        let config = Config::default(); // max_width = 100
        let result = format_source(source, &config).unwrap();
        // Should be on one line (only trailing newline)
        let newline_count = result.chars().filter(|&c| c == '\n').count();
        assert_eq!(
            newline_count, 1,
            "Expected single line command in: {result:?}"
        );
    }

    // Idempotency tests - formatting twice should yield the same result

    /// Helper to assert idempotency: format once, then format again, results should match.
    fn assert_idempotent(source: &str, config: &Config) {
        let first = format_source(source, config).unwrap();
        let second = format_source(&first, config).unwrap();
        assert_eq!(
            first, second,
            "Formatting is not idempotent!\nFirst pass:\n{first}\nSecond pass:\n{second}"
        );
    }

    #[test]
    fn test_idempotent_multiline_record_with_commas() {
        let source = "{\n  a: 1\n  b: 2\n}";
        let config = Config::default();
        assert_idempotent(source, &config);
    }

    #[test]
    fn test_idempotent_multiline_record_already_formatted() {
        let source = "{\n  a: 1,\n  b: 2,\n}";
        let config = Config::default();
        assert_idempotent(source, &config);
    }

    #[test]
    fn test_idempotent_multiline_list() {
        let source = "[\n  1\n  2\n  3\n]";
        let config = Config::default();
        assert_idempotent(source, &config);
    }

    #[test]
    fn test_idempotent_pipeline_with_optional_env() {
        let source = "$x\n  | default $env.VAR?\n  | default ~/fallback";
        let config = Config::default();
        assert_idempotent(source, &config);
    }

    #[test]
    fn test_idempotent_complex_function() {
        let source = r#"export def "repo root" [
  suggested_root?: string
] {
  $suggested_root
    | default $env.REPO_ROOT?
    | default ~/projects
    | path expand
}"#;
        let config = Config::default();
        assert_idempotent(source, &config);
    }

    #[test]
    fn test_idempotent_nested_records() {
        let source = "{\n  outer: {\n    inner: 1\n  }\n}";
        let config = Config::default();
        assert_idempotent(source, &config);
    }

    #[test]
    fn test_idempotent_closure() {
        let source = "{|x, y| $x + $y}";
        let config = Config::default();
        assert_idempotent(source, &config);
    }

    #[test]
    fn test_idempotent_multiline_closure() {
        let source = "{|x|\n  $x + 1\n}";
        let config = Config::default();
        assert_idempotent(source, &config);
    }

    #[test]
    fn test_idempotent_single_line_record() {
        let source = "{a: 1, b: 2}";
        let config = Config::default();
        assert_idempotent(source, &config);
    }

    #[test]
    fn test_idempotent_with_comments() {
        let source = "{\n  a: 1 # comment\n  b: 2\n}";
        let config = Config::default();
        assert_idempotent(source, &config);
    }
}
