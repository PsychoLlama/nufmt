use serde::Deserialize;

/// Preferred quote style for strings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuoteStyle {
    /// Preserve existing quote style (default).
    #[default]
    Preserve,
    /// Prefer double quotes when possible.
    Double,
    /// Prefer single quotes when possible.
    Single,
}

/// Formatting configuration options.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Number of spaces per indentation level.
    pub indent_width: usize,
    /// Maximum line width before breaking.
    pub max_width: usize,
    /// Preferred quote style for strings.
    pub quote_style: QuoteStyle,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            indent_width: 4,
            max_width: 100,
            quote_style: QuoteStyle::default(),
        }
    }
}
