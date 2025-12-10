//! Token preprocessing for the formatter.

use nu_parser::FlatShape;
use nu_protocol::Span;

/// A token with its span and shape, plus the gap content before it.
#[derive(Debug, Clone)]
pub struct Token<'a> {
    /// The text of the token.
    pub text: &'a str,
    /// The shape of the token.
    pub shape: FlatShape,
    /// The gap (whitespace/comments) before this token.
    pub gap_before: &'a str,
}

/// Preprocess flattened tokens into a more convenient format.
pub fn preprocess_tokens<'a>(source: &'a str, flattened: &[(Span, FlatShape)]) -> Vec<Token<'a>> {
    let mut tokens = Vec::with_capacity(flattened.len());
    let mut last_end = 0;

    for (span, shape) in flattened {
        if span.start < last_end || span.start > span.end || span.end > source.len() {
            continue;
        }

        let gap_before = &source[last_end..span.start];
        let text = &source[span.start..span.end];

        tokens.push(Token {
            text,
            shape: shape.clone(),
            gap_before,
        });

        last_end = span.end;
    }

    // Add a final synthetic token to capture trailing content
    if last_end < source.len() {
        tokens.push(Token {
            text: "",
            shape: FlatShape::Nothing,
            gap_before: &source[last_end..],
        });
    }

    tokens
}
