use serde::Deserialize;

/// Formatting configuration options.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Number of spaces per indentation level.
    pub indent_width: usize,
    /// Maximum line width before breaking.
    pub max_width: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            indent_width: 4,
            max_width: 100,
        }
    }
}
