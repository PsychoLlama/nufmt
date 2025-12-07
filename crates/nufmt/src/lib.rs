//! Nushell code formatting library.
//!
//! This library provides functionality to format Nushell source code.

mod config;
mod format;

pub use config::Config;
pub use format::{FormatError, format_source};
