//! String quote conversion utilities.

use crate::QuoteStyle;

/// Convert string quotes based on configured style.
pub fn convert_string_quotes(token: &str, style: QuoteStyle) -> String {
    match style {
        QuoteStyle::Preserve => token.to_string(),
        QuoteStyle::Double => to_double_quotes(token),
        QuoteStyle::Single => to_single_quotes(token),
    }
}

/// Convert a string to double quotes if possible.
fn to_double_quotes(token: &str) -> String {
    if token.starts_with('"') {
        return token.to_string();
    }

    if let Some(content) = token.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        if content.contains('"') || content.contains('\\') {
            return token.to_string();
        }
        return format!("\"{content}\"");
    }

    token.to_string()
}

/// Convert a string to single quotes if possible.
fn to_single_quotes(token: &str) -> String {
    if token.starts_with('\'') {
        return token.to_string();
    }

    if let Some(content) = token.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        if content.contains('\'') || content.contains('\\') {
            return token.to_string();
        }
        return format!("'{content}'");
    }

    token.to_string()
}
