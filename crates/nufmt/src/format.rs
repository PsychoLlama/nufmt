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
            self.process_block_token(token);
            self.last_end = span.end;
            return;
        }

        // Check for newlines in the gap between tokens
        let gap = &self.source[self.last_end..span.start];
        let newline_count = gap.chars().filter(|&c| c == '\n').count();

        if newline_count > 0 {
            // Preserve at most one blank line
            let lines_to_add = newline_count.min(2);
            for _ in 0..lines_to_add {
                self.output.push('\n');
            }
            self.line_start = true;
        } else if !self.output.is_empty() && !self.line_start {
            // Add space between tokens on same line
            if self.needs_space_before(token) {
                self.output.push(' ');
            }
        }

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

    fn needs_space_before(&self, token: &str) -> bool {
        if token.is_empty() {
            return false;
        }

        let Some(last_char) = self.output.chars().last() else {
            return false;
        };

        // No space after opening brackets
        if matches!(last_char, '(' | '[' | '{') {
            return false;
        }

        // No space before closing brackets, comma, semicolon, or colon
        if matches!(
            token.chars().next(),
            Some(')' | ']' | '}' | ',' | ';' | ':')
        ) {
            return false;
        }

        // No space after colon in records (e.g., {a: 1})
        if last_char == ':' {
            return true;
        }

        true
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
}
