//! fish emitter.

use std::path::PathBuf;

use shellcraft_shell_api::keys::{self, Key};
use shellcraft_shell_api::ShellKind;
use shellcraft_shell_api::{config_home, ShellEmitter};

/// Emitter for fish.
pub struct Fish;

impl ShellEmitter for Fish {
    fn kind(&self) -> ShellKind {
        ShellKind::Fish
    }

    fn rc_file(&self, env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
        Some(config_home(env)?.join("fish").join("config.fish"))
    }

    /// Slots in the order they appear in the init script.
    fn slots(&self) -> &'static [&'static str] {
        &[
            "header",
            "env",
            "tools",
            "history",
            "user_keys",
            "abbreviations",
            "aliases",
            "jump",
            "not_found",
            "plugin_snippets",
            "functions",
        ]
    }

    fn templates(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("fish/header", include_str!("../templates/header.j2")),
            ("fish/env", include_str!("../templates/env.j2")),
            ("fish/tools", include_str!("../templates/tools.j2")),
            ("fish/history", include_str!("../templates/history.j2")),
            ("fish/user_keys", include_str!("../templates/user_keys.j2")),
            (
                "fish/abbreviations",
                include_str!("../templates/abbreviations.j2"),
            ),
            ("fish/aliases", include_str!("../templates/aliases.j2")),
            ("fish/jump", include_str!("../templates/jump.j2")),
            ("fish/not_found", include_str!("../templates/not_found.j2")),
            (
                "fish/plugin_snippets",
                include_str!("../templates/plugin_snippets.j2"),
            ),
            ("fish/functions", include_str!("../templates/functions.j2")),
        ]
    }

    fn tool_init(&self, tool: &str) -> Option<&'static str> {
        match tool {
            "starship" => Some("init fish --print-full-init"),
            "zoxide" => Some("init fish"),
            "fzf" => Some("--fish"),
            "mise" => Some("activate fish"),
            "atuin" => Some("init fish --disable-up-arrow"),
            _ => None,
        }
    }

    fn key_sequence(&self, key: &str) -> Option<String> {
        Some(match keys::parse(key)? {
            Key::ShiftTab => r"\e\[Z".to_string(),
            Key::Ctrl(c) => format!(r"\c{c}"),
            Key::Alt(c) => format!(r"\e{c}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shellcraft_core::Config;
    use std::process::Command;

    fn sample() -> Config {
        Config::from_toml(
            r#"
            [prompt]
            builtin = false
            [tools]
            atuin = true
            starship = true
            [aliases]
            ll = "ls -la 'x'"
            [env]
            EDITOR = "nvim"
            [keys]
            "ctrl-g" = "kill-whole-line"
            "#,
        )
        .unwrap()
    }

    #[test]
    fn renders_config_values() {
        let out = Fish.emit(&sample()).unwrap();
        assert!(out.contains(r"set -gx EDITOR 'nvim'"));
        assert!(out.contains(r"alias ll 'ls -la \'x\''"));
        assert!(out.contains(r"bind \cg 'kill-whole-line'"));
        let starship = out
            .find("_sc_cached_init starship starship init fish")
            .unwrap();
        let atuin = out.find("_sc_cached_init atuin atuin init fish").unwrap();
        assert!(starship < atuin);
    }

    #[test]
    fn unsupported_key_errors() {
        let mut cfg = Config::default();
        cfg.keys.insert("super-x".into(), "foo".into());
        assert!(Fish.emit(&cfg).is_err());
    }

    /// Runs only where fish is installed; it was not available when this was written.
    #[test]
    fn generated_script_passes_fish_n() {
        let out = Fish.emit(&sample()).unwrap();
        let file =
            std::env::temp_dir().join(format!("shellcraft-fish-{}.fish", std::process::id()));
        std::fs::write(&file, out).unwrap();
        let res = Command::new("fish").arg("-n").arg(&file).output();
        let _ = std::fs::remove_file(&file);
        if let Ok(o) = res {
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        }
    }
}
