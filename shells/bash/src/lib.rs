//! bash emitter.

use std::path::PathBuf;

use shellcraft_shell_api::keys::{self, Key};
use shellcraft_shell_api::ShellKind;
use shellcraft_shell_api::{var, ShellEmitter};

/// Emitter for bash.
pub struct Bash;

impl ShellEmitter for Bash {
    fn kind(&self) -> ShellKind {
        ShellKind::Bash
    }

    fn rc_file(&self, env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
        Some(PathBuf::from(var(env, "HOME")?).join(".bashrc"))
    }

    /// Slots in the order they appear in the init script.
    fn slots(&self) -> &'static [&'static str] {
        &[
            "header",
            "options",
            "env",
            "completion",
            "keybindings",
            "tools",
            "history",
            "user_keys",
            "aliases",
            "jump",
            "not_found",
            "plugin_snippets",
            "functions",
        ]
    }

    fn templates(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("bash/header", include_str!("../templates/header.j2")),
            ("bash/options", include_str!("../templates/options.j2")),
            ("bash/env", include_str!("../templates/env.j2")),
            (
                "bash/completion",
                include_str!("../templates/completion.j2"),
            ),
            (
                "bash/keybindings",
                include_str!("../templates/keybindings.j2"),
            ),
            ("bash/tools", include_str!("../templates/tools.j2")),
            ("bash/history", include_str!("../templates/history.j2")),
            ("bash/user_keys", include_str!("../templates/user_keys.j2")),
            ("bash/aliases", include_str!("../templates/aliases.j2")),
            ("bash/jump", include_str!("../templates/jump.j2")),
            ("bash/not_found", include_str!("../templates/not_found.j2")),
            (
                "bash/plugin_snippets",
                include_str!("../templates/plugin_snippets.j2"),
            ),
            ("bash/functions", include_str!("../templates/functions.j2")),
        ]
    }

    fn tool_init(&self, tool: &str) -> Option<&'static str> {
        match tool {
            "starship" => Some("init bash --print-full-init"),
            "zoxide" => Some("init bash"),
            "fzf" => Some("--bash"),
            "mise" => Some("activate bash"),
            "atuin" => Some("init bash --disable-up-arrow"),
            _ => None,
        }
    }

    fn key_sequence(&self, key: &str) -> Option<String> {
        readline_sequence(key)
    }
}

/// `ctrl-x`, `alt-x` or `shift-tab` as a readline key sequence (`\C-x`, `\ex`, `\e[Z`).
fn readline_sequence(key: &str) -> Option<String> {
    Some(match keys::parse(key)? {
        Key::ShiftTab => r"\e[Z".to_string(),
        Key::Ctrl(c) => format!(r"\C-{c}"),
        Key::Alt(c) => format!(r"\e{c}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shellcraft_core::Config;
    use std::process::Command;

    fn sample() -> Config {
        Config::from_toml(
            r#"
            [shell]
            history_size = 777
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
        let out = Bash.emit(&sample()).unwrap();
        assert!(out.contains("HISTSIZE=777"));
        assert!(out.contains("export EDITOR='nvim'"));
        assert!(out.contains(r"alias ll='ls -la '\''x'\'''"));
        assert!(out.contains(r#"bind '"\C-g": kill-whole-line'"#));
        let starship = out
            .find("_sc_cached_init starship starship init bash")
            .unwrap();
        let atuin = out.find("_sc_cached_init atuin atuin init bash").unwrap();
        assert!(starship < atuin);
    }

    #[test]
    fn readline_sequences() {
        assert_eq!(readline_sequence("ctrl-r").as_deref(), Some(r"\C-r"));
        assert_eq!(readline_sequence("alt-f").as_deref(), Some(r"\ef"));
        assert_eq!(readline_sequence("shift-tab").as_deref(), Some(r"\e[Z"));
        assert_eq!(readline_sequence("super-x"), None);
    }

    #[test]
    fn generated_script_passes_bash_n_and_loads() {
        let out = Bash.emit(&sample()).unwrap();
        let dir = std::env::temp_dir().join(format!("shellcraft-bash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("init.bash");
        std::fs::write(&file, out).unwrap();
        let n = Command::new("bash").arg("-n").arg(&file).output().unwrap();
        assert!(n.status.success(), "{}", String::from_utf8_lossy(&n.stderr));
        let run = Command::new("bash")
            .args(["--norc", "--noprofile", "-i", "-c"])
            .arg(format!(
                "export HOME={h} XDG_CACHE_HOME={h}/cache; . {f}; echo \"$EDITOR $HISTSIZE\"; alias ll; type mkcd >/dev/null && echo fn-ok",
                h = dir.display(),
                f = file.display()
            ))
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&run.stdout);
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(stdout.contains("nvim 777"), "{stdout} {stderr}");
        assert!(stdout.contains(r"ll='ls -la '\''x'\'''"), "{stdout}");
        assert!(stdout.contains("fn-ok"), "{stdout}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
