/// Formatting configuration options.
#[derive(Debug, Clone)]
pub struct Config {
    /// Number of spaces per indentation level.
    pub indent_width: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self { indent_width: 4 }
    }
}
