//! zsh emitter.

use std::path::PathBuf;

use shellcraft_shell_api::keys::{self, Key};
use shellcraft_shell_api::ShellKind;
use shellcraft_shell_api::{var, ShellEmitter};

/// Emitter for zsh.
pub struct Zsh;

impl ShellEmitter for Zsh {
    fn kind(&self) -> ShellKind {
        ShellKind::Zsh
    }

    fn rc_file(&self, env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
        let dir = var(env, "ZDOTDIR").or_else(|| var(env, "HOME"))?;
        Some(PathBuf::from(dir).join(".zshrc"))
    }

    /// Slots in the order they appear in the init script.
    fn slots(&self) -> &'static [&'static str] {
        &[
            "header",
            "options",
            "env",
            "completion",
            "plugins",
            "keybindings",
            "tools",
            "history",
            "suggest",
            "prompt",
            "widgets",
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
            ("zsh/header", include_str!("../templates/header.j2")),
            ("zsh/options", include_str!("../templates/options.j2")),
            ("zsh/env", include_str!("../templates/env.j2")),
            ("zsh/completion", include_str!("../templates/completion.j2")),
            ("zsh/plugins", include_str!("../templates/plugins.j2")),
            (
                "zsh/keybindings",
                include_str!("../templates/keybindings.j2"),
            ),
            ("zsh/tools", include_str!("../templates/tools.j2")),
            ("zsh/history", include_str!("../templates/history.j2")),
            ("zsh/suggest", include_str!("../templates/suggest.j2")),
            ("zsh/prompt", include_str!("../templates/prompt.j2")),
            ("zsh/widgets", include_str!("../templates/widgets.j2")),
            ("zsh/user_keys", include_str!("../templates/user_keys.j2")),
            (
                "zsh/abbreviations",
                include_str!("../templates/abbreviations.j2"),
            ),
            ("zsh/aliases", include_str!("../templates/aliases.j2")),
            ("zsh/jump", include_str!("../templates/jump.j2")),
            ("zsh/not_found", include_str!("../templates/not_found.j2")),
            (
                "zsh/plugin_snippets",
                include_str!("../templates/plugin_snippets.j2"),
            ),
            ("zsh/functions", include_str!("../templates/functions.j2")),
        ]
    }

    fn tool_init(&self, tool: &str) -> Option<&'static str> {
        match tool {
            "starship" => Some("init zsh --print-full-init"),
            "zoxide" => Some("init zsh"),
            "fzf" => Some("--zsh"),
            "mise" => Some("activate zsh"),
            "atuin" => Some("init zsh --disable-up-arrow"),
            _ => None,
        }
    }

    fn key_sequence(&self, key: &str) -> Option<String> {
        caret_sequence(key)
    }
}

/// `ctrl-x`, `alt-x` or `shift-tab` as a zsh `bindkey` sequence (`^X`, `^[x`, `^[[Z`).
fn caret_sequence(key: &str) -> Option<String> {
    Some(match keys::parse(key)? {
        Key::ShiftTab => "^[[Z".to_string(),
        Key::Ctrl(c) => format!("^{}", c.to_ascii_uppercase()),
        Key::Alt(c) => format!("^[{c}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shellcraft_core::Config;
    use shellcraft_shell_api::EmitError;
    use std::process::Command;

    fn sample() -> Config {
        Config::from_toml(
            r#"
            [shell]
            history_size = 1234
            [prompt]
            builtin = false
            [tools]
            atuin = true
            starship = true
            fzf = false
            weird = true
            [aliases]
            ll = "ls -la 'x'"
            [env]
            EDITOR = "nvim"
            [keys]
            "ctrl-g" = "push-line"
            "#,
        )
        .unwrap()
    }

    #[test]
    fn renders_config_values() {
        let out = Zsh.emit(&sample()).unwrap();
        assert!(out.contains("HISTSIZE=1234"));
        assert!(out.contains("export EDITOR='nvim'"));
        assert!(out.contains("alias ll='ls -la '\\''x'\\'''"));
        assert!(out.contains("bindkey '^G' push-line"));
        assert!(out.contains("# unknown tool ignored: weird"));
        let starship = out
            .find("_sc_cached_init starship starship init zsh")
            .unwrap();
        let atuin = out.find("_sc_cached_init atuin atuin init zsh").unwrap();
        assert!(starship < atuin, "atuin must load last");
        assert!(!out.contains("_sc_cached_init fzf"));
    }

    #[test]
    fn unsupported_key_is_an_error() {
        let mut cfg = Config::default();
        cfg.keys.insert("super-x".into(), "foo".into());
        assert!(matches!(Zsh.emit(&cfg), Err(EmitError::Key { .. })));
    }

    #[test]
    fn key_sequences() {
        assert_eq!(caret_sequence("ctrl-r").as_deref(), Some("^R"));
        assert_eq!(caret_sequence("Alt-f").as_deref(), Some("^[f"));
        assert_eq!(caret_sequence("shift-tab").as_deref(), Some("^[[Z"));
        assert_eq!(caret_sequence("ctrl-1"), None);
        assert_eq!(caret_sequence("ctrl-ab"), None);
    }

    fn zsh_available() -> bool {
        Command::new("zsh").arg("--version").output().is_ok()
    }

    #[test]
    fn generated_script_passes_zsh_n_and_loads() {
        if !zsh_available() {
            return;
        }
        let out = Zsh.emit(&sample()).unwrap();
        let dir = std::env::temp_dir().join(format!("shellcraft-zsh-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("init.zsh");
        std::fs::write(&file, out).unwrap();
        let n = Command::new("zsh").arg("-n").arg(&file).output().unwrap();
        assert!(n.status.success(), "{}", String::from_utf8_lossy(&n.stderr));
        let run = Command::new("zsh")
            .args(["-f", "-i", "-c"])
            .arg(format!(
                "HOME={h} XDG_CACHE_HOME={h}/cache; source {f}; print -r -- $EDITOR $HISTSIZE; alias ll; type mkcd >/dev/null && echo fn-ok",
                h = dir.display(),
                f = file.display()
            ))
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&run.stdout);
        assert!(
            stdout.contains("nvim 1234"),
            "{stdout} {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert!(stdout.contains("ll='ls -la '\\''x'\\'"), "{stdout}");
        assert!(stdout.contains("fn-ok"), "{stdout}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
