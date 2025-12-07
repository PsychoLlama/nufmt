use std::sync::Arc;

use nu_cmd_lang::create_default_context;
use nu_parser::{FlatShape, flatten_block, parse};
use nu_protocol::{Span, engine::StateWorkingSet};

use crate::Config;

/// Errors that can occur during formatting.
#[derive(Debug)]
pub enum FormatError {
    /// The source code could not be parsed.
    ParseError(String),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseError(msg) => write!(f, "parse error: {msg}"),
        }
    }
}

impl std::error::Error for FormatError {}

/// Format Nushell source code.
///
/// Returns the formatted source code.
///
/// # Errors
///
/// Returns an error if the source code cannot be parsed.
pub fn format_source(source: &str, config: &Config) -> Result<String, FormatError> {
    let engine_state = create_default_context();
    let mut working_set = StateWorkingSet::new(&engine_state);

    let block = parse(&mut working_set, None, source.as_bytes(), false);

    // Check for parse errors
    if !working_set.parse_errors.is_empty() {
        let errors: Vec<String> = working_set
            .parse_errors
            .iter()
            .map(ToString::to_string)
            .collect();
        return Err(FormatError::ParseError(errors.join(", ")));
    }

    let formatted = format_block(&working_set, &block, source, config);
    Ok(formatted)
}

struct Formatter<'a> {
    source: &'a str,
    config: &'a Config,
    output: String,
    indent_level: usize,
    line_start: bool,
    last_end: usize,
    last_token: Option<&'a str>,
    current_line_len: usize,
}

impl<'a> Formatter<'a> {
    const fn new(source: &'a str, config: &'a Config) -> Self {
        Self {
            source,
            config,
            output: String::new(),
            indent_level: 0,
            line_start: true,
            last_end: 0,
            last_token: None,
            current_line_len: 0,
        }
    }

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

    fn process_token(&mut self, span: Span, shape: &FlatShape) {
        let token = &self.source[span.start..span.end];

        // Handle block/closure shapes specially - they include braces with whitespace
        if matches!(shape, FlatShape::Block | FlatShape::Closure) {
            self.process_gap(span.start);
            self.process_block_token(token);
            self.last_end = span.end;
            return;
        }

        // Handle pipe specially - add spaces around it and possibly break line
        if matches!(shape, FlatShape::Pipe) {
            // Don't process gap normally - we handle spacing ourselves
            let gap = &self.source[self.last_end..span.start];
            let has_newline = gap.contains('\n');

            // Check if we should break before the pipe
            // " | " adds 3 characters
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
            self.last_token = Some(token);
            return;
        }

        // Handle record/list delimiter tokens - normalize spacing
        if matches!(shape, FlatShape::Record | FlatShape::List) {
            self.process_gap(span.start);
            self.process_delimiter_token(token);
            self.last_end = span.end;
            self.last_token = Some(token);
            return;
        }

        // Process gap between last token and this one
        self.process_gap(span.start);

        // Write indentation if at line start
        if self.line_start {
            self.write_indent();
        }

        // Write the token
        self.push_str(token);
        self.line_start = false;
        self.last_end = span.end;
        self.last_token = Some(token);
    }

    fn process_gap(&mut self, next_start: usize) {
        let gap = &self.source[self.last_end..next_start];

        // Process each line in the gap to handle comments and newlines
        let mut lines = gap.split('\n').peekable();
        let mut first_line = true;

        while let Some(line) = lines.next() {
            let trimmed = line.trim();
            let has_more_lines = lines.peek().is_some();

            // Check for comment
            if let Some(comment_start) = trimmed.find('#') {
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
            } else if !trimmed.is_empty() {
                // Non-comment, non-whitespace content (like = in let)
                if !self.output.is_empty() && !self.line_start && !self.output.ends_with(' ') {
                    self.push_char(' ');
                }
                if self.line_start {
                    self.write_indent();
                    self.line_start = false;
                }
                self.push_str(trimmed);
                if line.ends_with(' ') || line.ends_with('\t') || has_more_lines {
                    self.push_char(' ');
                }
            } else if has_more_lines {
                // Empty line (just newline)
                self.push_newline();
            } else if !first_line {
                // Trailing part after last newline but no content
                // Don't add space, newline already happened
            } else if !gap.is_empty() && !self.line_start {
                // Just whitespace on single line - add space if not already present
                if !self.output.ends_with(' ') {
                    self.push_char(' ');
                }
            }

            first_line = false;
        }
    }

    fn process_block_token(&mut self, token: &str) {
        let trimmed = token.trim();
        let has_open = trimmed.starts_with('{');
        let has_close = trimmed.ends_with('}');

        // Extract content between braces
        let inner = if has_open && has_close {
            trimmed
                .strip_prefix('{')
                .and_then(|s| s.strip_suffix('}'))
                .unwrap_or("")
        } else if has_open {
            token.trim_start().strip_prefix('{').unwrap_or("")
        } else if has_close {
            token.trim_end().strip_suffix('}').unwrap_or("")
        } else {
            ""
        };

        // Opening brace
        if has_open {
            if !self.line_start && !self.output.ends_with(' ') {
                self.push_char(' ');
            }
            if self.line_start {
                self.write_indent();
            }
            self.push_char('{');
            self.indent_level += 1;

            // Check if there's meaningful content after the brace
            let first_line = inner.lines().next().unwrap_or("");
            if first_line.trim().is_empty() || first_line.trim().starts_with('#') {
                self.push_newline();
            } else {
                self.push_char(' ');
                self.line_start = false;
            }
        }

        // Process inner content (may contain comments)
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

        // Closing brace
        if has_close {
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
    }

    fn process_delimiter_token(&mut self, token: &str) {
        // Normalize spacing in record/list delimiters
        // e.g., ",  " -> ", " and ":   " -> ": "
        let trimmed = token.trim();

        if self.line_start {
            self.write_indent();
        }

        // Handle opening brackets
        if trimmed == "{" || trimmed == "[" {
            self.push_str(trimmed);
            self.line_start = false;
            return;
        }

        // Handle closing brackets
        if trimmed == "}" || trimmed == "]" {
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

        // Handle comma - normalize to ", "
        if trimmed == "," {
            self.push_str(", ");
            self.line_start = false;
            return;
        }

        // For multiline records, handle newlines
        if token.contains('\n') {
            for line in token.lines() {
                let line_trimmed = line.trim();
                if !line_trimmed.is_empty() {
                    self.push_str(line_trimmed);
                    if line_trimmed == ":" {
                        self.push_char(' ');
                    }
                }
            }
            if token.ends_with('\n') {
                self.push_newline();
            }
            return;
        }

        // Default: just write the trimmed token
        self.push_str(trimmed);
        if token.ends_with(' ') || token.ends_with('\t') {
            self.push_char(' ');
        }
        self.line_start = false;
    }

    fn write_indent(&mut self) {
        let indent_size = self.indent_level * self.config.indent_width;
        for _ in 0..indent_size {
            self.output.push(' ');
        }
        self.current_line_len = indent_size;
    }

    fn push_char(&mut self, c: char) {
        self.output.push(c);
        if c == '\n' {
            self.current_line_len = 0;
        } else {
            self.current_line_len += 1;
        }
    }

    fn push_str(&mut self, s: &str) {
        self.output.push_str(s);
        if let Some(last_newline) = s.rfind('\n') {
            self.current_line_len = s.len() - last_newline - 1;
        } else {
            self.current_line_len += s.len();
        }
    }

    fn push_newline(&mut self) {
        self.output.push('\n');
        self.current_line_len = 0;
        self.line_start = true;
    }
}

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
}
