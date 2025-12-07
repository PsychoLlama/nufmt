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
        }
    }

    fn format(mut self, flattened: &[(Span, FlatShape)]) -> String {
        for (span, shape) in flattened {
            self.process_token(*span, shape);
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

        // Handle pipe specially - always add spaces around it
        if matches!(shape, FlatShape::Pipe) {
            self.process_gap(span.start);
            if !self.output.is_empty() && !self.line_start && !self.output.ends_with(' ') {
                self.output.push(' ');
            }
            if self.line_start {
                self.write_indent();
            }
            self.output.push_str(token);
            self.output.push(' ');
            self.line_start = false;
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
        self.output.push_str(token);
        self.line_start = false;
        self.last_end = span.end;
        self.last_token = Some(token);
    }

    fn process_gap(&mut self, next_start: usize) {
        let gap = &self.source[self.last_end..next_start];

        // Check for newlines first
        let newline_count = gap.chars().filter(|&c| c == '\n').count();

        if newline_count > 0 {
            // Preserve at most one blank line
            let lines_to_add = newline_count.min(2);
            for _ in 0..lines_to_add {
                self.output.push('\n');
            }
            self.line_start = true;
            return;
        }

        // Check for non-whitespace content in gap (e.g., = in let statements)
        let gap_content = gap.trim();
        if !gap_content.is_empty() {
            // There's meaningful content in the gap - preserve it with spacing
            if !self.output.is_empty() && !self.line_start && !self.output.ends_with(' ') {
                self.output.push(' ');
            }
            if self.line_start {
                self.write_indent();
                self.line_start = false;
            }
            self.output.push_str(gap_content);
            // Add space after gap content if there was whitespace after it originally
            if gap.ends_with(' ') || gap.ends_with('\t') {
                self.output.push(' ');
            }
            return;
        }

        // Just whitespace - add single space if not at line start
        if !self.output.is_empty() && !self.line_start && !gap.is_empty() {
            self.output.push(' ');
        }
    }

    fn process_block_token(&mut self, token: &str) {
        let trimmed = token.trim();

        // Opening brace
        if trimmed.starts_with('{') {
            // Add space before brace if not at line start
            if !self.line_start && !self.output.ends_with(' ') {
                self.output.push(' ');
            }
            if self.line_start {
                self.write_indent();
            }
            self.output.push('{');
            self.indent_level += 1;

            // Check if there's content after the brace on the same line
            let after_brace = token.trim_start().strip_prefix('{').unwrap_or("");
            if after_brace.contains('\n') || after_brace.trim().is_empty() {
                self.output.push('\n');
                self.line_start = true;
            } else {
                self.output.push(' ');
                self.line_start = false;
            }
        }

        // Closing brace
        if trimmed.ends_with('}') {
            self.indent_level = self.indent_level.saturating_sub(1);

            // Check if there's content before the brace
            let before_brace = token.trim_end().strip_suffix('}').unwrap_or("");
            if before_brace.contains('\n') || self.line_start {
                if !self.output.ends_with('\n') {
                    self.output.push('\n');
                }
                self.line_start = true;
            }

            if self.line_start {
                self.write_indent();
            }
            self.output.push('}');
            self.line_start = false;
        }
    }

    fn write_indent(&mut self) {
        for _ in 0..(self.indent_level * self.config.indent_width) {
            self.output.push(' ');
        }
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
    formatter.format(&flattened)
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
}
