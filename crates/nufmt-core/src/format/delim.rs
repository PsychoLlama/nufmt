//! Delimiter utilities for bracket matching.

/// Check if text starts with an opening brace.
pub fn starts_with_open_brace(text: &str) -> bool {
    text.starts_with('{')
}

/// Check if text ends with a closing brace.
pub fn ends_with_close_brace(text: &str) -> bool {
    text.ends_with('}')
}

/// Check if text starts with an opening paren.
pub fn starts_with_open_paren(text: &str) -> bool {
    text.starts_with('(')
}

/// Check if text ends with a closing paren.
pub fn ends_with_close_paren(text: &str) -> bool {
    text.ends_with(')')
}

/// Check if text is an opening bracket (brace or square bracket).
pub fn is_open_bracket(text: &str) -> bool {
    text == "{" || text == "["
}

/// Check if text is a closing bracket (brace or square bracket).
pub fn is_close_bracket(text: &str) -> bool {
    text == "}" || text == "]"
}

/// Check if text ends with a closing bracket (brace or square bracket).
pub fn ends_with_close_bracket(text: &str) -> bool {
    text.ends_with('}') || text.ends_with(']')
}

/// Count closing braces in text.
pub fn count_close_braces(text: &str) -> usize {
    text.chars().filter(|&c| c == '}').count()
}
