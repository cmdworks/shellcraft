//! Error type for config parsing and validation.

use thiserror::Error;

/// Errors produced while reading or validating `shellcraft.toml`.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    /// The TOML text could not be parsed or does not match the schema.
    /// The message includes line and column when the parser knows them.
    #[error("invalid shellcraft.toml: {0}")]
    Parse(String),
    /// A value parsed but is not acceptable.
    #[error("invalid value for `{key}`: {reason}")]
    Invalid {
        /// Dotted key path, e.g. `env.MY VAR`.
        key: String,
        /// Human readable explanation.
        reason: String,
    },
}

impl ConfigError {
    pub(crate) fn invalid(key: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Invalid {
            key: key.into(),
            reason: reason.into(),
        }
    }
}
