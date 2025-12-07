//! Nushell code formatting library.
//!
//! This library provides functionality to format Nushell source code.

mod config;
mod format;

pub use config::{BracketSpacing, Config, ConfigError, QuoteStyle, TrailingComma};
pub use format::{FormatError, SourceLocation, debug_tokens, format_source};
