//! `shellcraft.toml` schema, parsing and validation.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

/// Shells shellcraft can target. `nu` and `pwsh` are experimental.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    Zsh,
    Bash,
    Fish,
    #[serde(rename = "nu")]
    Nushell,
    Pwsh,
}

impl Shell {
    /// Parse a shell name (`zsh`, `bash`, `fish`, `nu`, `pwsh`).
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "zsh" => Some(Self::Zsh),
            "bash" => Some(Self::Bash),
            "fish" => Some(Self::Fish),
            "nu" => Some(Self::Nushell),
            "pwsh" => Some(Self::Pwsh),
            _ => None,
        }
    }

    /// File extension for generated init scripts.
    pub fn file_ext(self) -> &'static str {
        match self {
            Self::Pwsh => "ps1",
            other => other.name(),
        }
    }

    /// Lowercase name as used in config and on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Self::Zsh => "zsh",
            Self::Bash => "bash",
            Self::Fish => "fish",
            Self::Nushell => "nu",
            Self::Pwsh => "pwsh",
        }
    }
}

/// `[shell]` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ShellSection {
    /// Shell to generate for when none is given on the command line.
    pub default: Option<Shell>,
    /// Lines of history kept in memory and on disk.
    pub history_size: u32,
}

impl Default for ShellSection {
    fn default() -> Self {
        Self {
            default: None,
            history_size: 50_000,
        }
    }
}

/// `[plugins]` section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PluginsSection {
    /// Plugins as `owner/repo`.
    pub list: Vec<String>,
}

/// `[prompt]` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PromptSection {
    /// Use shellcraft's own prompt (`shellcraft prompt`); set false to keep starship.
    pub builtin: bool,
    /// Text before the cursor.
    pub symbol: String,
    /// Show the duration of the last command when it took at least this long (ms).
    pub min_duration_ms: u64,
    /// Plugin segments shown after the directory, in order (`shellcraft plugin list`).
    pub segments: Vec<String>,
}

impl Default for PromptSection {
    fn default() -> Self {
        Self {
            builtin: true,
            symbol: "\u{276f}".to_string(),
            min_duration_ms: 2000,
            segments: vec!["git".to_string(), "versions".to_string()],
        }
    }
}

/// `[theme]` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ThemeSection {
    /// One of [`crate::theme::NAMES`].
    pub name: String,
}

impl Default for ThemeSection {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
        }
    }
}

/// `[suggest]` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SuggestSection {
    /// Show the most likely completion of what you type as dim text after the cursor.
    pub inline: bool,
    /// Colour the command line while you type (zsh): commands, options, strings, variables.
    pub highlight: bool,
}

impl Default for SuggestSection {
    fn default() -> Self {
        Self {
            inline: true,
            highlight: true,
        }
    }
}

/// `[plugin.<name>]` section: settings for one plugin (bundled or outside).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PluginSettings {
    /// A disabled plugin shows no segments and adds no shell code.
    pub enabled: bool,
    /// Outside plugin: executable that prints the segment (see the plugin plan).
    pub segment: Option<String>,
    /// Outside plugin: how long its output is reused (milliseconds).
    pub ttl_ms: u64,
    /// Free-form options for the plugin, e.g. `ecosystems = "node,rust"` for versions.
    pub options: BTreeMap<String, String>,
}

impl Default for PluginSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            segment: None,
            ttl_ms: 10_000,
            options: BTreeMap::new(),
        }
    }
}

/// Where, relative to a slot's own text, a plugin's code goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlotPosition {
    Before,
    #[default]
    After,
}

/// Code a plugin attaches to a named slot of one shell's init script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotSnippet {
    pub slot: String,
    pub position: SlotPosition,
    pub text: String,
}

/// Shell code from an installed plugin, attached by `apply` (not part of the TOML or the cache).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstalledPlugin {
    pub name: String,
    /// Shell name (`zsh`, `bash`, ...) -> snippet text.
    pub snippets: BTreeMap<String, String>,
    /// Code attached to named slots of the shell being generated (`[[slot]]` in the manifest).
    pub slots: Vec<SlotSnippet>,
}

/// `[registry]` section: where `plugin add <name>` and `plugin search` look.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct RegistrySection {
    /// Registries, tried in order: a local folder, `owner/repo`, or a git URL. A registry is a
    /// git repository (or folder) holding an `index.toml`.
    pub sources: Vec<String>,
    /// Trusted registry signing keys (64 hex digits, see `shellcraft registry keygen`). When this
    /// is not empty every registry must carry an `index.toml.sig` that one of them verifies.
    pub keys: Vec<String>,
}

/// `[completion]` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct CompletionSection {
    /// Learn flags and subcommands of commands without their own completion by running
    /// `<command> --help` once (cached for a week).
    pub help: bool,
    /// Commands whose `--help` is never run.
    pub skip: Vec<String>,
}

impl Default for CompletionSection {
    fn default() -> Self {
        Self {
            help: true,
            skip: Vec::new(),
        }
    }
}

/// Whole `shellcraft.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub shell: ShellSection,
    /// Tool name -> enabled, e.g. `starship = true`.
    pub tools: BTreeMap<String, bool>,
    pub plugins: PluginsSection,
    pub prompt: PromptSection,
    pub theme: ThemeSection,
    pub suggest: SuggestSection,
    pub completion: CompletionSection,
    pub registry: RegistrySection,
    /// Per-plugin settings, `[plugin.<name>]`.
    pub plugin: BTreeMap<String, PluginSettings>,
    /// Installed plugins' shell code; filled in by `apply`, never read from the config file.
    #[serde(skip)]
    pub installed: Vec<InstalledPlugin>,
    /// Hook events (`cd`, `post`) that installed plugins listen to; filled in by `apply`.
    #[serde(skip)]
    pub hook_events: Vec<String>,
    /// Themes from installed theme plugins; filled in by `apply` (it is part of the cache the
    /// prompt reads), not meant to be written by hand.
    pub theme_data: BTreeMap<String, crate::theme::ThemeColors>,
    /// Alias name -> expansion.
    pub aliases: BTreeMap<String, String>,
    /// Abbreviation -> expansion, expanded in place when you press space (zsh, fish).
    pub abbr: BTreeMap<String, String>,
    /// Environment variable name -> value.
    pub env: BTreeMap<String, String>,
    /// Key sequence -> command, e.g. `"ctrl-r" = "history-search"`.
    pub keys: BTreeMap<String, String>,
}

impl Config {
    /// Palette of the configured theme.
    pub fn palette(&self) -> crate::theme::Palette {
        match self.theme_data.get(&self.theme.name) {
            Some(t) => t.palette(),
            None => {
                crate::theme::palette(&self.theme.name).unwrap_or(crate::theme::Palette::DEFAULT)
            }
        }
    }

    /// Parse and validate a config from TOML text.
    pub fn from_toml(text: &str) -> Result<Self, ConfigError> {
        let cfg: Self = toml::from_str(text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Check values the schema cannot express.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.shell.history_size == 0 {
            return Err(ConfigError::invalid(
                "shell.history_size",
                "must be greater than 0",
            ));
        }
        if self.prompt.symbol.is_empty() || self.prompt.symbol.chars().any(char::is_control) {
            return Err(ConfigError::invalid(
                "prompt.symbol",
                "must be non-empty and contain no control characters",
            ));
        }
        // Installed theme plugins are not known here; `apply` warns when the theme is missing.
        if !is_word(&self.theme.name) {
            return Err(ConfigError::invalid(
                "theme.name",
                format!(
                    "`{}`: use letters, digits, `-`, `_` or `.`",
                    self.theme.name
                ),
            ));
        }
        for k in &self.registry.keys {
            if k.len() != 64 || !k.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(ConfigError::invalid(
                    "registry.keys",
                    format!("`{k}` is not a 64-digit hex public key"),
                ));
            }
        }
        for seg in &self.prompt.segments {
            if !is_word(seg) {
                return Err(ConfigError::invalid(
                    "prompt.segments",
                    format!("`{seg}` is not a plugin name (letters, digits, `-`, `_`, `.`)"),
                ));
            }
        }
        for (name, p) in &self.plugin {
            if !is_word(name) {
                return Err(ConfigError::invalid(
                    format!("plugin.{name}"),
                    "plugin names use letters, digits, `-`, `_` or `.`",
                ));
            }
            if p.segment.as_deref().is_some_and(|s| s.trim().is_empty()) {
                return Err(ConfigError::invalid(
                    format!("plugin.{name}.segment"),
                    "cannot be empty",
                ));
            }
        }
        if let Some(bad) = self
            .completion
            .skip
            .iter()
            .find(|c| c.is_empty() || c.contains(['/', ' ']))
        {
            return Err(ConfigError::invalid(
                "completion.skip",
                format!("`{bad}` is not a command name"),
            ));
        }
        if let Some(bad) = self.registry.sources.iter().find(|s| s.trim().is_empty()) {
            return Err(ConfigError::invalid(
                "registry.sources",
                format!("`{bad}` is not a registry location"),
            ));
        }
        for name in self.tools.keys() {
            if !is_word(name) {
                return Err(ConfigError::invalid(
                    format!("tools.{name}"),
                    "tool names use letters, digits, `-`, `_` or `.`",
                ));
            }
        }
        for plugin in &self.plugins.list {
            if !is_repo(plugin) {
                return Err(ConfigError::invalid(
                    "plugins.list",
                    format!("`{plugin}` is not in `owner/repo` form"),
                ));
            }
        }
        for (section, map) in [("aliases", &self.aliases), ("abbr", &self.abbr)] {
            for (name, value) in map {
                if name.is_empty()
                    || name.contains(|c: char| c.is_whitespace() || "=/$'\"\\".contains(c))
                {
                    return Err(ConfigError::invalid(
                        format!("{section}.{name}"),
                        "names cannot be empty or contain whitespace, `=`, `/`, `$`, quotes or `\\`",
                    ));
                }
                if value.trim().is_empty() {
                    return Err(ConfigError::invalid(
                        format!("{section}.{name}"),
                        "expansion cannot be empty",
                    ));
                }
            }
        }
        for name in self.env.keys() {
            if !is_env_name(name) {
                return Err(ConfigError::invalid(
                    format!("env.{name}"),
                    "variable names match [A-Za-z_][A-Za-z0-9_]*",
                ));
            }
        }
        for (key, cmd) in &self.keys {
            if key.trim().is_empty() || cmd.trim().is_empty() {
                return Err(ConfigError::invalid(
                    format!("keys.{key}"),
                    "key and command cannot be empty",
                ));
            }
        }
        Ok(())
    }
}

pub fn is_word(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn is_repo(s: &str) -> bool {
    match s.split_once('/') {
        Some((owner, repo)) => is_word(owner) && is_word(repo),
        None => false,
    }
}

fn is_env_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_parse_roundtrip() {
        for s in [
            Shell::Zsh,
            Shell::Bash,
            Shell::Fish,
            Shell::Nushell,
            Shell::Pwsh,
        ] {
            assert_eq!(Shell::parse(s.name()), Some(s));
        }
        assert_eq!(Shell::parse("csh"), None);
    }

    #[test]
    fn empty_is_default() {
        assert_eq!(Config::from_toml("").unwrap(), Config::default());
    }

    #[test]
    fn full_example() {
        let cfg = Config::from_toml(
            r#"
            [shell]
            default = "nu"
            history_size = 1000
            [tools]
            starship = true
            zoxide = false
            [plugins]
            list = ["zsh-users/zsh-autosuggestions"]
            [aliases]
            ll = "ls -la"
            [env]
            EDITOR = "nvim"
            [keys]
            "ctrl-r" = "history-search"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.shell.default, Some(Shell::Nushell));
        assert_eq!(cfg.shell.history_size, 1000);
        assert!(!cfg.tools["zoxide"]);
        assert_eq!(cfg.aliases["ll"], "ls -la");
        assert_eq!(cfg.env["EDITOR"], "nvim");
    }

    #[test]
    fn unknown_key_reports_location() {
        let err = Config::from_toml("[shell]\nshel = 1\n").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("shel"), "{msg}");
        assert!(msg.contains("line 2"), "{msg}");
    }

    #[test]
    fn unknown_section_rejected() {
        assert!(matches!(
            Config::from_toml("[nope]\n"),
            Err(ConfigError::Parse(_))
        ));
    }

    #[test]
    fn bad_shell_rejected() {
        assert!(Config::from_toml("[shell]\ndefault = \"csh\"\n").is_err());
    }

    #[test]
    fn zero_history_rejected() {
        let err = Config::from_toml("[shell]\nhistory_size = 0\n").unwrap_err();
        assert!(err.to_string().contains("shell.history_size"));
    }

    #[test]
    fn bad_env_name_rejected() {
        let err = Config::from_toml("[env]\n\"1X\" = \"a\"\n").unwrap_err();
        assert!(err.to_string().contains("env.1X"));
    }

    #[test]
    fn bad_alias_rejected() {
        assert!(Config::from_toml("[aliases]\n\"a b\" = \"ls\"\n").is_err());
        assert!(Config::from_toml("[aliases]\nll = \"  \"\n").is_err());
        assert!(Config::from_toml("[abbr]\n\"a b\" = \"ls\"\n").is_err());
        assert!(Config::from_toml("[abbr]\ngco = \"git checkout\"\n").is_ok());
    }

    #[test]
    fn bad_plugin_rejected() {
        let err = Config::from_toml("[plugins]\nlist = [\"noslash\"]\n").unwrap_err();
        assert!(err.to_string().contains("owner/repo"));
    }

    #[test]
    fn bad_tool_name_rejected() {
        assert!(Config::from_toml("[tools]\n\"a b\" = true\n").is_err());
    }

    #[test]
    fn wrong_type_rejected() {
        assert!(Config::from_toml("[tools]\nstarship = \"yes\"\n").is_err());
    }
}
