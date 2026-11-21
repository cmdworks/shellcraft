//! nushell emitter (experimental).

use std::path::PathBuf;

use shellcraft_shell_api::ShellKind;
use shellcraft_shell_api::{config_home, ShellEmitter};

/// Emitter for nushell. Key bindings are not supported through `[keys]`.
pub struct Nushell;

impl ShellEmitter for Nushell {
    fn kind(&self) -> ShellKind {
        ShellKind::Nushell
    }

    fn rc_file(&self, env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
        Some(config_home(env)?.join("nushell").join("config.nu"))
    }

    /// Slots in the order they appear in the init script.
    fn slots(&self) -> &'static [&'static str] {
        &["header", "builtins", "plugin_snippets", "aliases"]
    }

    fn templates(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("nu/header", include_str!("../templates/header.j2")),
            ("nu/builtins", include_str!("../templates/builtins.j2")),
            (
                "nu/plugin_snippets",
                include_str!("../templates/plugin_snippets.j2"),
            ),
            ("nu/aliases", include_str!("../templates/aliases.j2")),
        ]
    }
}
