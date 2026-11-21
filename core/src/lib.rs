//! Config schema, theme model and errors for shellcraft.

pub mod cache;
pub mod config;
pub mod error;
pub mod paths;
pub mod proc;
pub mod theme;

pub use config::{
    Config, InstalledPlugin, PluginSettings, PromptSection, RegistrySection, Shell, SlotPosition,
    SlotSnippet, SuggestSection, ThemeSection,
};
pub use error::ConfigError;

/// Name of the tool, used in messages and paths.
pub const NAME: &str = "shellcraft";

/// Commented default config written by `shellcraft init`.
pub const DEFAULT_CONFIG: &str = include_str!("default_config.toml");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_parses() {
        let cfg = Config::from_toml(DEFAULT_CONFIG).unwrap();
        assert!(cfg.tools.is_empty(), "no external tools by default");
        assert!(cfg.prompt.builtin);
        assert_eq!(cfg.shell.history_size, 50_000);
    }
}
