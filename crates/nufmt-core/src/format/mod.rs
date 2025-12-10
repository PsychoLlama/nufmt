//! Nushell code formatter using the Wadler-Lindig pretty printing algorithm.
//!
//! This module uses the `pretty` crate to format Nushell source code. The algorithm
//! automatically chooses between single-line and multiline layouts based on the
//! configured `max_width`.

mod closure;
mod error;
mod string;
mod token;

pub use error::{FormatError, SourceLocation};

use std::sync::LazyLock;

use nu_cmd_lang::create_default_context;
use nu_command::add_shell_command_context;
use nu_parser::{FlatShape, flatten_block, parse};
use nu_protocol::{
    ParseError,
    engine::{EngineState, StateWorkingSet},
};
use pretty::{Arena, DocAllocator, DocBuilder};

use crate::{BracketSpacing, Config, TrailingComma};
use closure::parse_closure_params;
use string::convert_string_quotes;
use token::{Token, preprocess_tokens};

/// Type alias for our document builder.
type Doc<'a> = DocBuilder<'a, Arena<'a>>;

/// Cached engine state with all Nushell commands for parsing.
/// Creating this is expensive (~10ms), so we cache it globally.
static ENGINE_STATE: LazyLock<EngineState> = LazyLock::new(|| {
    let engine_state = create_default_context();
    add_shell_command_context(engine_state)
});

/// Check if a parse error is a resolution error (module/file/command not found).
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

/// Debug token output for Nushell source code.
#[must_use]
pub fn debug_tokens(source: &str) -> String {
    use std::fmt::Write;

    let mut working_set = StateWorkingSet::new(&ENGINE_STATE);
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
/// # Errors
///
/// Returns an error if the source code cannot be parsed.
pub fn format_source(source: &str, config: &Config) -> Result<String, FormatError> {
    let mut working_set = StateWorkingSet::new(&ENGINE_STATE);

    let block = parse(&mut working_set, None, source.as_bytes(), false);

    let syntax_error = working_set
        .parse_errors
        .iter()
        .find(|e| !is_resolution_error(e));
    if let Some(error) = syntax_error {
        return Err(FormatError::from_parse_error(error, source));
    }

    let flattened = flatten_block(&working_set, &block);
    let formatted = format_tokens(source, &flattened, config);
    Ok(formatted)
}

/// Format tokens into a string using the pretty printing algorithm.
fn format_tokens(
    source: &str,
    flattened: &[(nu_protocol::Span, FlatShape)],
    config: &Config,
) -> String {
    let tokens = preprocess_tokens(source, flattened);
    let arena = Arena::new();
    let mut formatter = Formatter::new(&arena, &tokens, config);
    let doc = formatter.format_all();

    let mut output = String::new();
    doc.render_fmt(config.max_width, &mut output).unwrap();

    // Ensure trailing newline
    if !output.ends_with('\n') {
        output.push('\n');
    }

    output
}

/// The formatter state.
struct Formatter<'a> {
    arena: &'a Arena<'a>,
    tokens: &'a [Token<'a>],
    config: &'a Config,
    index: usize,
    /// Track string interpolation depth.
    interp_depth: usize,
    /// Current indentation level.
    indent_level: usize,
}

impl<'a> Formatter<'a> {
    const fn new(arena: &'a Arena<'a>, tokens: &'a [Token<'a>], config: &'a Config) -> Self {
        Self {
            arena,
            tokens,
            config,
            index: 0,
            interp_depth: 0,
            indent_level: 0,
        }
    }

    /// Create an indentation string for the current level.
    fn indent_str(&self) -> String {
        " ".repeat(self.indent_level * self.config.indent_width)
    }

    /// Format all tokens into a document.
    fn format_all(&mut self) -> Doc<'a> {
        let mut docs: Vec<Doc<'a>> = Vec::new();

        while self.index < self.tokens.len() {
            let doc = self.format_next();
            docs.push(doc);
        }

        self.arena.concat(docs)
    }

    /// Format the next token and its preceding gap.
    fn format_next(&mut self) -> Doc<'a> {
        let token = &self.tokens[self.index].clone();
        self.index += 1;

        let gap_doc = self.format_gap(token.gap_before);
        let token_doc = self.format_token(token);

        gap_doc.append(token_doc)
    }

    /// Format a gap (whitespace and comments between tokens).
    fn format_gap(&mut self, gap: &'a str) -> Doc<'a> {
        if gap.is_empty() {
            return self.arena.nil();
        }

        let has_newline = gap.contains('\n');
        let gap_trimmed = gap.trim();

        // Handle simple cases first
        if !has_newline {
            // No newlines - check if there's content other than whitespace
            if gap_trimmed.is_empty() {
                // Pure whitespace - becomes a single space
                return self.arena.space();
            }
            // Content like "=" - wrap with spaces
            return self
                .arena
                .space()
                .append(self.arena.text(gap_trimmed))
                .append(self.arena.space());
        }

        // Check if gap contains structural delimiters (e.g., match expression braces)
        // or commas followed by newlines (match arms)
        let has_open_brace = gap.contains('{');
        let has_close_brace = gap.contains('}');
        let has_comma_newline = gap.contains(",\n") || gap.contains(",\r\n");

        // If gap contains braces or comma+newline, preserve the structure
        if has_open_brace || has_close_brace || has_comma_newline {
            return self.format_structural_gap(gap);
        }

        // Count newlines to detect blank lines
        let newline_count = gap.chars().filter(|&c| c == '\n').count();

        let mut docs: Vec<Doc<'a>> = Vec::new();
        let mut lines = gap.split('\n').peekable();
        let mut first_line = true;
        let mut emitted_newline = false;

        while let Some(line) = lines.next() {
            let trimmed = line.trim();
            let has_more = lines.peek().is_some();

            if let Some(comment_start) = trimmed.find('#') {
                // Handle content before comment (like = or ;)
                let before = trimmed[..comment_start].trim();
                if !before.is_empty() {
                    if !first_line && !emitted_newline {
                        docs.push(self.arena.hardline());
                        docs.push(self.arena.text(self.indent_str()));
                        emitted_newline = true;
                    } else if first_line {
                        docs.push(self.arena.space());
                    }
                    docs.push(self.arena.text(before));
                    docs.push(self.arena.space());
                }

                // Add comment
                let comment = &trimmed[comment_start..];
                if before.is_empty() {
                    if first_line {
                        // Check if original line had leading space (inline comment case)
                        let original_had_leading_space = line.starts_with(char::is_whitespace);
                        if original_had_leading_space {
                            docs.push(self.arena.space());
                        }
                    } else if !emitted_newline {
                        docs.push(self.arena.hardline());
                        docs.push(self.arena.text(self.indent_str()));
                        emitted_newline = true;
                    }
                }
                docs.push(self.arena.text(comment));

                if has_more {
                    docs.push(self.arena.hardline());
                    docs.push(self.arena.text(self.indent_str()));
                    emitted_newline = true;
                }
            } else if !trimmed.is_empty() {
                // Non-comment content in gap (like =, ;, ., etc.)
                if !first_line && !emitted_newline {
                    docs.push(self.arena.hardline());
                    docs.push(self.arena.text(self.indent_str()));
                    emitted_newline = true;
                } else if first_line {
                    // First line with content - check if original had leading space
                    let original_had_leading_space = line.starts_with(char::is_whitespace);
                    if original_had_leading_space {
                        docs.push(self.arena.space());
                    }
                }
                docs.push(self.arena.text(trimmed));

                // Determine spacing after content
                if has_more {
                    let next_line_trimmed = lines.peek().map_or("", |l| l.trim());
                    if !trimmed.ends_with('.') && !next_line_trimmed.is_empty() {
                        docs.push(self.arena.space());
                    }
                } else if !trimmed.ends_with('.') {
                    // Check if original had trailing space
                    let original_had_trailing_space = line.ends_with(char::is_whitespace);
                    if original_had_trailing_space {
                        docs.push(self.arena.space());
                    }
                }
            } else if has_more && !first_line {
                // Empty line (not first) followed by more - potential blank line separator
                // We'll emit this blank line if there's content following
                if !emitted_newline {
                    docs.push(self.arena.hardline());
                    emitted_newline = true;
                }
                // Check if this is a blank line separator (two newlines in a row)
                if newline_count > 1 {
                    docs.push(self.arena.hardline());
                }
            }

            first_line = false;
        }

        // If gap had newlines but we haven't emitted anything, emit a single newline
        if docs.is_empty() && has_newline {
            docs.push(self.arena.hardline());
            docs.push(self.arena.text(self.indent_str()));
        }

        self.arena.concat(docs)
    }

    /// Format a gap that contains structural delimiters like { or } or comma+newline.
    /// These appear in match expressions where braces aren't separate tokens.
    fn format_structural_gap(&mut self, gap: &'a str) -> Doc<'a> {
        let has_newline = gap.contains('\n');
        let mut docs: Vec<Doc<'a>> = Vec::new();

        // Track brace depth changes in this gap
        let opens = gap.chars().filter(|&c| c == '{').count();
        let closes = gap.chars().filter(|&c| c == '}').count();

        if !has_newline {
            // Single-line: just emit with spaces
            let trimmed = gap.trim();
            docs.push(self.arena.space());
            if !trimmed.is_empty() {
                docs.push(self.arena.text(trimmed));
                if !trimmed.ends_with('{') {
                    docs.push(self.arena.space());
                }
            }
            // Adjust indent level for single-line structural gaps
            self.indent_level += opens;
            self.indent_level = self.indent_level.saturating_sub(closes);
            return self.arena.concat(docs);
        }

        // Multiline: preserve structure with proper indentation
        let lines: Vec<&str> = gap.split('\n').collect();
        let mut need_newline_before_next = false;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            let is_first = i == 0;
            let is_last = i == lines.len() - 1;

            if trimmed.is_empty() {
                if !is_first && !is_last {
                    // Blank line in the middle - preserve it
                    need_newline_before_next = true;
                }
                continue;
            }

            // Count braces in this line for indent tracking
            let line_opens = trimmed.chars().filter(|&c| c == '{').count();
            let line_closes = trimmed.chars().filter(|&c| c == '}').count();

            // Handle content
            if is_first {
                // First line content comes after a token
                if trimmed == "," {
                    // Just a comma - append it directly
                    docs.push(self.arena.text(","));
                    need_newline_before_next = true;
                } else if trimmed.ends_with('{') {
                    docs.push(self.arena.space());
                    docs.push(self.arena.text(trimmed));
                    self.indent_level += line_opens;
                    // Already emitting newline+indent, don't need another at end
                    docs.push(self.arena.hardline());
                    docs.push(self.arena.text(self.indent_str()));
                    need_newline_before_next = false;
                } else {
                    docs.push(self.arena.space());
                    docs.push(self.arena.text(trimmed));
                    self.indent_level += line_opens;
                    self.indent_level = self.indent_level.saturating_sub(line_closes);
                    need_newline_before_next = true;
                }
            } else {
                // Subsequent lines
                if need_newline_before_next {
                    docs.push(self.arena.hardline());
                    need_newline_before_next = false;
                }

                if trimmed == "}" {
                    // Closing brace - decrement indent first, then emit
                    self.indent_level = self.indent_level.saturating_sub(1);
                    docs.push(self.arena.text(self.indent_str()));
                    docs.push(self.arena.text(trimmed));
                } else {
                    docs.push(self.arena.text(self.indent_str()));
                    docs.push(self.arena.text(trimmed));
                    self.indent_level += line_opens;
                    self.indent_level = self.indent_level.saturating_sub(line_closes);
                }

                if !is_last {
                    need_newline_before_next = true;
                }
            }
        }

        // If we still need a newline (gap ends with newline after content like comma)
        if need_newline_before_next {
            docs.push(self.arena.hardline());
            docs.push(self.arena.text(self.indent_str()));
        } else {
            // If the gap ends with content (not a newline), add trailing space
            let last_char = gap.chars().last();
            if last_char.is_some_and(|c| c != '\n' && !c.is_whitespace() && c != '{') {
                docs.push(self.arena.space());
            }
        }

        self.arena.concat(docs)
    }

    /// Format a single token based on its shape.
    fn format_token(&mut self, token: &Token<'a>) -> Doc<'a> {
        // Handle synthetic end token
        if token.text.is_empty() && matches!(token.shape, FlatShape::Nothing) {
            return self.arena.nil();
        }

        match token.shape {
            FlatShape::StringInterpolation => {
                if token.text.starts_with('$') {
                    self.interp_depth += 1;
                } else {
                    self.interp_depth = self.interp_depth.saturating_sub(1);
                }
                self.arena.text(token.text)
            }
            FlatShape::Block | FlatShape::Closure => self.format_block_token(token),
            FlatShape::Pipe => self.format_pipe_token(token),
            FlatShape::Record | FlatShape::List => self.format_collection_token(token),
            FlatShape::String => self.format_string_token(token),
            _ => self.arena.text(token.text),
        }
    }

    /// Format a block or closure token.
    fn format_block_token(&mut self, token: &Token<'a>) -> Doc<'a> {
        let trimmed = token.text.trim();

        if trimmed.starts_with('{') || trimmed.ends_with('}') {
            self.format_brace_block(token)
        } else if trimmed.starts_with('(') || trimmed.ends_with(')') {
            self.format_paren_block(token)
        } else {
            self.arena.text(token.text)
        }
    }

    /// Format a brace-delimited block or closure.
    fn format_brace_block(&mut self, token: &Token<'a>) -> Doc<'a> {
        let trimmed = token.text.trim();
        let has_open = trimmed.starts_with('{');
        let has_close = trimmed.ends_with('}');
        let has_newline = token.text.contains('\n');

        // Count braces to detect multi-brace tokens (e.g., ",\n}\n}" from match inside def)
        let close_count = trimmed.chars().filter(|&c| c == '}').count();

        // Handle multi-close tokens (common with match expressions)
        if !has_open && close_count > 1 {
            return self.format_multi_close(token.text);
        }

        if has_open && has_close {
            self.format_complete_block(trimmed, has_newline)
        } else if has_open {
            self.format_block_open(trimmed, has_newline)
        } else if has_close {
            self.format_block_close(has_newline)
        } else {
            self.arena.text(token.text)
        }
    }

    /// Format a token that contains multiple closing braces (e.g., match inside a def).
    fn format_multi_close(&mut self, text: &'a str) -> Doc<'a> {
        let mut docs: Vec<Doc<'a>> = Vec::new();
        let has_newline = text.contains('\n');

        // Process the text line by line, handling each } properly
        let lines: Vec<&str> = text.split('\n').collect();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            let is_first = i == 0;

            if trimmed.is_empty() {
                continue;
            }

            // Handle content before braces (like commas)
            // Find where the braces start
            if let Some(brace_pos) = trimmed.find('}') {
                let before = trimmed[..brace_pos].trim();
                if !before.is_empty() {
                    docs.push(self.arena.text(before));
                }
            }

            // Count and emit closing braces in this line
            let brace_count = trimmed.chars().filter(|&c| c == '}').count();
            for j in 0..brace_count {
                self.indent_level = self.indent_level.saturating_sub(1);
                if has_newline && (j > 0 || !is_first) {
                    let indent = self.indent_str();
                    docs.push(self.arena.hardline());
                    docs.push(self.arena.text(indent));
                }
                docs.push(self.arena.text("}"));
            }
        }

        self.arena.concat(docs)
    }

    /// Format a complete block `{ ... }` that's in a single token.
    fn format_complete_block(&mut self, trimmed: &'a str, source_multiline: bool) -> Doc<'a> {
        let inner = trimmed
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .unwrap_or("");

        let (params, body) = parse_closure_params(inner);
        let body_trimmed = body.trim();

        if body_trimmed.is_empty() {
            if let Some(p) = params {
                return self
                    .arena
                    .text("{")
                    .append(self.arena.text(p))
                    .append(self.arena.space())
                    .append(self.arena.text("}"));
            }
            return self
                .arena
                .text("{")
                .append(self.arena.space())
                .append(self.arena.text("}"));
        }

        let force_multiline = source_multiline || body.contains('\n');

        let open = self.arena.text("{");
        let close = self.arena.text("}");

        let open_with_params = if let Some(p) = params {
            open.append(self.arena.text(p))
        } else {
            open
        };

        if force_multiline {
            self.indent_level += 1;
            let body_doc = {
                let lines: Vec<&str> = body
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .collect();
                let indent = self.indent_str();
                let line_docs: Vec<Doc<'a>> = lines
                    .iter()
                    .map(|l| self.arena.text(indent.clone()).append(self.arena.text(*l)))
                    .collect();
                self.arena.intersperse(line_docs, self.arena.hardline())
            };
            self.indent_level -= 1;
            let close_indent = self.indent_str();

            open_with_params
                .append(self.arena.hardline())
                .append(body_doc)
                .append(self.arena.hardline())
                .append(self.arena.text(close_indent))
                .append(close)
        } else {
            // Single line with space around content
            open_with_params
                .append(self.arena.space())
                .append(self.arena.text(body_trimmed))
                .append(self.arena.space())
                .append(close)
        }
    }

    /// Format an opening brace `{` or `{|params|`.
    fn format_block_open(&mut self, trimmed: &'a str, source_multiline: bool) -> Doc<'a> {
        let (estimated_len, inner_has_newline) = self.estimate_block_length();
        let force_multiline =
            source_multiline || inner_has_newline || estimated_len > self.config.max_width;

        let after_brace = trimmed.strip_prefix('{').unwrap_or(trimmed);
        let (params, rest) = parse_closure_params(after_brace);

        let open = self.arena.text("{");
        let open_with_params = if let Some(p) = params {
            open.append(self.arena.text(p))
        } else {
            open
        };

        self.indent_level += 1;

        // Check if there's content after the opening brace (e.g., comments)
        let rest_trimmed = if params.is_some() {
            rest.trim()
        } else {
            after_brace.trim()
        };

        if force_multiline {
            let indent = self.indent_str();
            if rest_trimmed.is_empty() {
                open_with_params
                    .append(self.arena.hardline())
                    .append(self.arena.text(indent))
            } else {
                // Content (like comments) embedded in the block opening token
                let content_lines: Vec<&str> = rest_trimmed
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .collect();
                let content_docs: Vec<Doc<'a>> = content_lines
                    .iter()
                    .map(|l| self.arena.text(indent.clone()).append(self.arena.text(*l)))
                    .collect();
                open_with_params
                    .append(self.arena.hardline())
                    .append(self.arena.intersperse(content_docs, self.arena.hardline()))
                    .append(self.arena.hardline())
                    .append(self.arena.text(indent))
            }
        } else {
            open_with_params.append(self.arena.space())
        }
    }

    /// Format a closing brace `}`.
    fn format_block_close(&mut self, has_newline: bool) -> Doc<'a> {
        self.indent_level = self.indent_level.saturating_sub(1);

        if has_newline {
            let indent = self.indent_str();
            self.arena
                .hardline()
                .append(self.arena.text(indent))
                .append(self.arena.text("}"))
        } else {
            self.arena.space().append(self.arena.text("}"))
        }
    }

    /// Estimate the length of a block by looking ahead.
    fn estimate_block_length(&self) -> (usize, bool) {
        self.estimate_delimited_length(
            |shape| matches!(shape, FlatShape::Block | FlatShape::Closure),
            |trimmed| trimmed.starts_with('{'),
            |trimmed| trimmed.ends_with('}'),
            |trimmed, _depth| trimmed.len(),
        )
    }

    /// Format a parenthesized block.
    fn format_paren_block(&mut self, token: &Token<'a>) -> Doc<'a> {
        let trimmed = token.text.trim();
        let has_newline = token.text.contains('\n');

        if trimmed == "(" {
            if has_newline {
                self.indent_level += 1;
                let indent = self.indent_str();
                self.arena
                    .text("(")
                    .append(self.arena.hardline())
                    .append(self.arena.text(indent))
            } else {
                self.arena.text("(")
            }
        } else if trimmed == ")" {
            if has_newline {
                self.indent_level = self.indent_level.saturating_sub(1);
                let indent = self.indent_str();
                self.arena
                    .hardline()
                    .append(self.arena.text(indent))
                    .append(self.arena.text(")"))
            } else {
                self.arena.text(")")
            }
        } else {
            self.arena.text(token.text)
        }
    }

    /// Format a pipe token with proper spacing.
    fn format_pipe_token(&self, token: &Token<'a>) -> Doc<'a> {
        // Check if the next token has a non-empty gap that will provide trailing space
        let next_gap_will_add_space =
            self.index < self.tokens.len() && !self.tokens[self.index].gap_before.is_empty();

        // Check if this token's gap provided leading space
        let gap_provided_leading = !token.gap_before.is_empty();

        let mut doc = self.arena.nil();

        // Add leading space if gap didn't provide it
        if !gap_provided_leading {
            doc = doc.append(self.arena.space());
        }

        doc = doc.append(self.arena.text("|"));

        // Add trailing space if next gap won't provide it
        if !next_gap_will_add_space {
            doc = doc.append(self.arena.space());
        }

        doc
    }

    /// Format a collection token (record or list).
    fn format_collection_token(&mut self, token: &Token<'a>) -> Doc<'a> {
        let trimmed = token.text.trim();
        let has_newline = token.text.contains('\n');

        match trimmed {
            "{" | "[" => self.format_collection_open(trimmed, has_newline),
            "}" | "]" => self.format_collection_close(trimmed, has_newline),
            ":" => self.arena.text(":").append(self.arena.space()),
            "," => {
                // Check if we're in multiline mode
                let is_multiline = self.is_in_multiline_collection();
                if is_multiline {
                    let indent = self.indent_str();
                    self.arena
                        .text(",")
                        .append(self.arena.hardline())
                        .append(self.arena.text(indent))
                } else {
                    self.arena.text(",").append(self.arena.space())
                }
            }
            _ => {
                if trimmed.ends_with('}') || trimmed.ends_with(']') {
                    self.format_collection_close_complex(trimmed, has_newline)
                } else if has_newline && trimmed.is_empty() {
                    self.format_newline_separator()
                } else if has_newline {
                    self.format_token_with_newline(token)
                } else {
                    self.arena.text(trimmed)
                }
            }
        }
    }

    /// Check if we're inside a multiline collection by looking at context.
    fn is_in_multiline_collection(&self) -> bool {
        // Look back for the opening bracket and check if it had a newline
        let mut depth = 0;
        for i in (0..self.index).rev() {
            let t = &self.tokens[i];
            let trimmed = t.text.trim();

            if matches!(t.shape, FlatShape::Record | FlatShape::List) {
                if trimmed == "}" || trimmed == "]" {
                    depth += 1;
                } else if trimmed == "{" || trimmed == "[" {
                    if depth == 0 {
                        return t.text.contains('\n') || t.gap_before.contains('\n');
                    }
                    depth -= 1;
                }
            }
        }
        false
    }

    /// Format opening bracket for a collection.
    fn format_collection_open(&mut self, bracket: &'a str, source_multiline: bool) -> Doc<'a> {
        let (estimated_len, inner_has_newline) = self.estimate_collection_length();
        let force_multiline =
            source_multiline || inner_has_newline || estimated_len > self.config.max_width;

        let open = self.arena.text(bracket);

        if force_multiline {
            self.indent_level += 1;
            let indent = self.indent_str();
            open.append(self.arena.hardline())
                .append(self.arena.text(indent))
        } else if self.config.bracket_spacing == BracketSpacing::Spaced {
            open.append(self.arena.space())
        } else {
            open
        }
    }

    /// Format closing bracket for a collection.
    fn format_collection_close(&mut self, bracket: &'a str, source_multiline: bool) -> Doc<'a> {
        if source_multiline {
            self.indent_level = self.indent_level.saturating_sub(1);
            let indent = self.indent_str();
            let trailing = if self.config.trailing_comma == TrailingComma::Always {
                self.arena.text(",")
            } else {
                self.arena.nil()
            };
            trailing
                .append(self.arena.hardline())
                .append(self.arena.text(indent))
                .append(self.arena.text(bracket))
        } else if self.config.bracket_spacing == BracketSpacing::Spaced {
            self.arena.space().append(self.arena.text(bracket))
        } else {
            self.arena.text(bracket)
        }
    }

    /// Format complex closing like "?}" or content followed by closing bracket.
    fn format_collection_close_complex(
        &mut self,
        trimmed: &'a str,
        source_multiline: bool,
    ) -> Doc<'a> {
        let bracket = if trimmed.contains('}') { '}' } else { ']' };
        let prefix = trimmed.trim_end_matches(bracket);

        let mut doc = self.arena.nil();
        if !prefix.is_empty() {
            doc = doc.append(self.arena.text(prefix));
        }

        if source_multiline {
            self.indent_level = self.indent_level.saturating_sub(1);
            let indent = self.indent_str();
            let trailing = if self.config.trailing_comma == TrailingComma::Always {
                self.arena.text(",")
            } else {
                self.arena.nil()
            };
            doc.append(trailing)
                .append(self.arena.hardline())
                .append(self.arena.text(indent))
                .append(self.arena.text(if bracket == '}' { "}" } else { "]" }))
        } else if self.config.bracket_spacing == BracketSpacing::Spaced {
            doc.append(self.arena.space())
                .append(self.arena.text(if bracket == '}' { "}" } else { "]" }))
        } else {
            doc.append(self.arena.text(if bracket == '}' { "}" } else { "]" }))
        }
    }

    /// Estimate collection length by looking ahead.
    fn estimate_collection_length(&self) -> (usize, bool) {
        self.estimate_delimited_length(
            |shape| matches!(shape, FlatShape::Record | FlatShape::List),
            |trimmed| trimmed == "{" || trimmed == "[",
            |trimmed| trimmed == "}" || trimmed == "]",
            |trimmed, depth| {
                if trimmed == "{" || trimmed == "[" {
                    1
                } else if trimmed == "}" || trimmed == "]" {
                    usize::from(depth > 1)
                } else if trimmed == ":" || trimmed == "," {
                    2
                } else {
                    trimmed.len()
                }
            },
        )
    }

    /// Generic estimation of delimited content length.
    ///
    /// Walks through tokens, tracking depth via open/close predicates,
    /// and accumulates length via the `token_len` callback.
    fn estimate_delimited_length<F, O, C, L>(
        &self,
        shape_matches: F,
        is_open: O,
        is_close: C,
        token_len: L,
    ) -> (usize, bool)
    where
        F: Fn(&FlatShape) -> bool,
        O: Fn(&str) -> bool,
        C: Fn(&str) -> bool,
        L: Fn(&str, usize) -> usize,
    {
        let mut length = 2;
        let mut depth = 1;
        let mut idx = self.index;
        let mut has_newline = false;

        while idx < self.tokens.len() && depth > 0 {
            let t = &self.tokens[idx];

            if t.gap_before.contains('\n') {
                has_newline = true;
            }

            let gap_trimmed = t.gap_before.trim();
            if !gap_trimmed.is_empty() {
                length += gap_trimmed.len() + 1;
            } else if !t.gap_before.is_empty() {
                length += 1;
            }

            let trimmed = t.text.trim();
            if shape_matches(&t.shape) {
                if is_open(trimmed) {
                    depth += 1;
                }
                if is_close(trimmed) {
                    depth -= 1;
                }
            }

            length += token_len(trimmed, depth);
            idx += 1;
        }

        (length, has_newline)
    }

    /// Format a newline separator in a collection (acts like comma).
    fn format_newline_separator(&self) -> Doc<'a> {
        let indent = self.indent_str();
        if self.config.trailing_comma == TrailingComma::Always {
            self.arena
                .text(",")
                .append(self.arena.hardline())
                .append(self.arena.text(indent))
        } else {
            self.arena.hardline().append(self.arena.text(indent))
        }
    }

    /// Format a token that contains a newline.
    fn format_token_with_newline(&self, token: &Token<'a>) -> Doc<'a> {
        if let Some(comment_start) = token.text.find('#') {
            let comment = token.text[comment_start..].trim_end();
            let indent = self.indent_str();

            let mut doc = self.arena.nil();
            if self.config.trailing_comma == TrailingComma::Always {
                doc = doc.append(self.arena.text(","));
            }
            doc = doc
                .append(self.arena.space())
                .append(self.arena.text(comment))
                .append(self.arena.hardline())
                .append(self.arena.text(indent));
            return doc;
        }

        let indent = self.indent_str();
        self.arena.hardline().append(self.arena.text(indent))
    }

    /// Format a string token with quote conversion.
    fn format_string_token(&self, token: &Token<'a>) -> Doc<'a> {
        let converted = convert_string_quotes(token.text, self.config.quote_style);
        self.arena.text(converted)
    }
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
    fn test_closure_params() {
        let source = "{|x, y| $x + $y}";
        let config = Config::default();
        let result = format_source(source, &config).unwrap();
        assert!(
            result.contains("|x, y|"),
            "Should preserve closure params: {result}"
        );
    }
}
