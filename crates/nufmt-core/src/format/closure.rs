//! Closure parameter parsing utilities.

/// Parse closure parameters from content after opening brace.
///
/// Returns `(Some(params), rest)` if params like `|x, y|` are found,
/// otherwise `(None, content)`.
pub fn parse_closure_params(content: &str) -> (Option<&str>, &str) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with('|') {
        return (None, content);
    }

    trimmed[1..].find('|').map_or((None, content), |close| {
        let params_end = close + 2;
        let params = &trimmed[..params_end];
        let rest = &trimmed[params_end..];
        (Some(params), rest)
    })
}
