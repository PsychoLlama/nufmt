use std::sync::Arc;

use nu_parser::{FlatShape, flatten_block, parse};
use nu_protocol::{
    Span,
    engine::{EngineState, StateWorkingSet},
};

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
    let engine_state = EngineState::new();
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

fn format_block(
    working_set: &StateWorkingSet,
    block: &Arc<nu_protocol::ast::Block>,
    source: &str,
    config: &Config,
) -> String {
    let flattened = flatten_block(working_set, block);
    let mut output = String::new();
    let mut indent_level: usize = 0;
    let mut line_start = true;
    let mut last_end: usize = 0;

    for (span, shape) in &flattened {
        let token = span_to_str(source, *span);

        // Handle indentation changes
        match shape {
            FlatShape::Block | FlatShape::Closure => {
                if token == "{" {
                    if line_start {
                        write_indent(&mut output, indent_level, config);
                    }
                    output.push_str(token);
                    indent_level += 1;
                    line_start = false;
                } else if token == "}" {
                    indent_level = indent_level.saturating_sub(1);
                    if line_start {
                        write_indent(&mut output, indent_level, config);
                    }
                    output.push_str(token);
                    line_start = false;
                } else {
                    write_token(&mut output, token, &mut line_start, indent_level, config);
                }
            }
            _ => {
                // Check for newlines in the original source between tokens
                let gap = &source[last_end..span.start];
                let newline_count = gap.chars().filter(|&c| c == '\n').count();

                if newline_count > 0 {
                    // Preserve at most one blank line
                    let lines_to_add = newline_count.min(2);
                    for _ in 0..lines_to_add {
                        output.push('\n');
                    }
                    line_start = true;
                } else if !output.is_empty() && !line_start {
                    // Add space between tokens on same line
                    let needs_space = needs_space_before(token, &output);
                    if needs_space {
                        output.push(' ');
                    }
                }

                write_token(&mut output, token, &mut line_start, indent_level, config);
            }
        }

        last_end = span.end;
    }

    // Ensure trailing newline
    if !output.ends_with('\n') {
        output.push('\n');
    }

    output
}

fn write_token(
    output: &mut String,
    token: &str,
    line_start: &mut bool,
    indent_level: usize,
    config: &Config,
) {
    if *line_start {
        write_indent(output, indent_level, config);
    }
    output.push_str(token);
    *line_start = false;
}

fn write_indent(output: &mut String, level: usize, config: &Config) {
    for _ in 0..(level * config.indent_width) {
        output.push(' ');
    }
}

fn needs_space_before(token: &str, output: &str) -> bool {
    if token.is_empty() || output.is_empty() {
        return false;
    }

    let last_char = output.chars().last().unwrap_or(' ');

    // No space after opening brackets or before closing brackets
    if matches!(last_char, '(' | '[' | '{') {
        return false;
    }
    if matches!(token.chars().next(), Some(')' | ']' | '}' | ',' | ';')) {
        return false;
    }

    // No space before/after certain operators when adjacent
    if last_char == '|' || token == "|" {
        return true;
    }

    true
}

fn span_to_str(source: &str, span: Span) -> &str {
    &source[span.start..span.end]
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
}
