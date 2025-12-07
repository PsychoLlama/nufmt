use serde::Deserialize;

/// Preferred quote style for strings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuoteStyle {
    /// Preserve existing quote style.
    Preserve,
    /// Prefer double quotes when possible (default).
    #[default]
    Double,
    /// Prefer single quotes when possible.
    Single,
}

/// Configuration validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    /// Description of the validation error.
    pub message: String,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ConfigError {}

/// Formatting configuration options.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Number of spaces per indentation level (1-16).
    pub indent_width: usize,
    /// Maximum line width before breaking (20-500).
    pub max_width: usize,
    /// Preferred quote style for strings.
    pub quote_style: QuoteStyle,
}

impl Config {
    /// Validate configuration values.
    ///
    /// # Errors
    ///
    /// Returns an error if any configuration value is out of acceptable range.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.indent_width == 0 || self.indent_width > 16 {
            return Err(ConfigError {
                message: format!(
                    "indent_width must be between 1 and 16, got {}",
                    self.indent_width
                ),
            });
        }
        if self.max_width < 20 || self.max_width > 500 {
            return Err(ConfigError {
                message: format!(
                    "max_width must be between 20 and 500, got {}",
                    self.max_width
                ),
            });
        }
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_indent_width_zero_invalid() {
        let config = Config {
            indent_width: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_indent_width_too_large_invalid() {
        let config = Config {
            indent_width: 17,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_max_width_too_small_invalid() {
        let config = Config {
            max_width: 10,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_max_width_too_large_invalid() {
        let config = Config {
            max_width: 501,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_valid_custom_config() {
        let config = Config {
            indent_width: 2,
            max_width: 80,
            quote_style: QuoteStyle::Single,
        };
        assert!(config.validate().is_ok());
    }
}
