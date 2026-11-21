//! PowerShell emitter (experimental).

use std::path::PathBuf;

use shellcraft_shell_api::keys::{self, Key};
use shellcraft_shell_api::ShellKind;
use shellcraft_shell_api::{config_home, ShellEmitter};

/// Emitter for PowerShell.
pub struct Pwsh;

impl ShellEmitter for Pwsh {
    fn kind(&self) -> ShellKind {
        ShellKind::Pwsh
    }

    fn rc_file(&self, env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
        Some(
            config_home(env)?
                .join("powershell")
                .join("Microsoft.PowerShell_profile.ps1"),
        )
    }

    /// Slots in the order they appear in the init script.
    fn slots(&self) -> &'static [&'static str] {
        &["header", "builtins", "plugin_snippets", "aliases"]
    }

    fn templates(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("pwsh/header", include_str!("../templates/header.j2")),
            ("pwsh/builtins", include_str!("../templates/builtins.j2")),
            (
                "pwsh/plugin_snippets",
                include_str!("../templates/plugin_snippets.j2"),
            ),
            ("pwsh/aliases", include_str!("../templates/aliases.j2")),
        ]
    }

    fn tool_init(&self, tool: &str) -> Option<&'static str> {
        match tool {
            "starship" => Some("init powershell --print-full-init"),
            "zoxide" => Some("init powershell"),
            "mise" => Some("activate pwsh"),
            "atuin" => Some("init powershell --disable-up-arrow"),
            _ => None,
        }
    }

    fn key_sequence(&self, key: &str) -> Option<String> {
        Some(match keys::parse(key)? {
            Key::ShiftTab => "Shift+Tab".to_string(),
            Key::Ctrl(c) => format!("Ctrl+{c}"),
            Key::Alt(c) => format!("Alt+{c}"),
        })
    }
}
