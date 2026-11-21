//! Cross-shell feature tests: each feature should appear (or not) in every shell it supports.

#![allow(clippy::unwrap_used)] // test helpers

use shellcraft_core::Config;
use shellcraft_shell::{emitter, ShellKind};

mod zsh_plugin_tests {
    use super::*;

    #[test]
    fn plugins_use_antidote_after_compdef_stub() {
        let cfg =
            Config::from_toml("[plugins]\nlist = [\"zsh-users/zsh-autosuggestions\", \"a/b\"]\n")
                .unwrap();
        let out = emitter(ShellKind::Zsh).emit(&cfg).unwrap();
        let stub = out.find("compdef() {").unwrap();
        let plugins = out.find("antidote bundle").unwrap();
        assert!(
            stub < plugins,
            "compdef stub must exist before plugins load"
        );
        assert!(out.contains("\nzsh-users/zsh-autosuggestions\na/b\nSHELLCRAFT_PLUGINS\n"));
        let none = emitter(ShellKind::Zsh).emit(&Config::default()).unwrap();
        assert!(!none.contains("antidote"));
    }
}

mod hist_tests {
    use super::*;

    fn script(kind: ShellKind, tools: &str) -> String {
        let cfg = Config::from_toml(&format!("[tools]\n{tools}\n")).unwrap();
        emitter(kind).emit(&cfg).unwrap()
    }

    #[test]
    fn ctrl_r_widget_only_without_atuin() {
        for (kind, widget) in [
            (ShellKind::Zsh, "_sc_hist_widget"),
            (ShellKind::Bash, "_sc_hist_widget"),
            (ShellKind::Fish, "_sc_hist_widget"),
        ] {
            assert!(script(kind, "fzf = true").contains(widget), "{kind:?}");
            let with_atuin = script(kind, "atuin = true");
            assert!(
                !with_atuin.contains(widget),
                "{kind:?} must leave Ctrl+R to atuin"
            );
            assert!(
                with_atuin.contains("hist add"),
                "{kind:?} still records history"
            );
        }
    }
}

mod prompt_tests {
    use super::*;

    fn script(kind: ShellKind, toml: &str) -> String {
        emitter(kind)
            .emit(&Config::from_toml(toml).unwrap())
            .unwrap()
    }

    #[test]
    fn builtin_prompt_replaces_starship() {
        let on = "[prompt]\nbuiltin = true\n[tools]\nstarship = true\n";
        let off = "[prompt]\nbuiltin = false\n[tools]\nstarship = true\n";
        for (kind, marker) in [
            (ShellKind::Zsh, "_sc_prompt_pre"),
            (ShellKind::Bash, "_sc_prompt_hook"),
            (ShellKind::Fish, "function fish_prompt"),
        ] {
            let s = script(kind, on);
            assert!(s.contains(marker), "{kind:?}");
            assert!(
                !s.contains("_sc_cached_init starship"),
                "{kind:?} drops starship"
            );
            let s = script(kind, off);
            assert!(
                !s.contains(marker),
                "{kind:?} default has no built-in prompt"
            );
            assert!(
                s.contains("_sc_cached_init starship"),
                "{kind:?} keeps starship"
            );
        }
    }

    #[test]
    fn bad_symbol_rejected() {
        assert!(Config::from_toml("[prompt]\nsymbol = \"\"\n").is_err());
        assert!(Config::from_toml("[prompt]\nsymbol = \"a\\nb\"\n").is_err());
    }
}

mod theme_tests {
    use super::*;

    #[test]
    fn theme_sets_tool_env_unless_user_overrides() {
        let mut nord = Config::from_toml("[theme]\nname = \"nord\"\n").unwrap();
        // what a theme plugin would contribute
        nord.theme_data.insert(
            "nord".into(),
            shellcraft_core::theme::ThemeColors {
                fzf: "--color=hl:#88c0d0".into(),
                bat: "Nord".into(),
                ..Default::default()
            },
        );
        let s = emitter(ShellKind::Zsh).emit(&nord).unwrap();
        assert!(s.contains("BAT_THEME=Nord") || s.contains("BAT_THEME='Nord'"));
        assert!(s.contains("FZF_DEFAULT_OPTS"));
        let mut own =
            Config::from_toml("[theme]\nname = \"nord\"\n[env]\nBAT_THEME = \"x\"\n").unwrap();
        own.theme_data = nord.theme_data.clone();
        let s = emitter(ShellKind::Zsh).emit(&own).unwrap();
        assert!(!s.contains("Nord") && s.contains("BAT_THEME"));
        let plain = emitter(ShellKind::Zsh).emit(&Config::default()).unwrap();
        assert!(!plain.contains("BAT_THEME"));
    }
}

mod abbr_tests {
    use super::*;

    #[test]
    fn abbreviations_in_zsh_and_fish() {
        let cfg = Config::from_toml("[abbr]\nzzab = \"git checkout 'x y'\"\n").unwrap();
        let z = emitter(ShellKind::Zsh).emit(&cfg).unwrap();
        assert!(z.contains("  'zzab' ") && z.contains("bindkey ' ' _sc_abbr_expand"));
        let f = emitter(ShellKind::Fish).emit(&cfg).unwrap();
        assert!(f.contains("abbr -a -- 'zzab'"));
        assert!(!emitter(ShellKind::Bash)
            .emit(&cfg)
            .unwrap()
            .contains("zzab"));
    }
}

mod picker_tests {
    use super::*;

    #[test]
    fn widgets_use_the_builtin_picker_not_fzf() {
        for kind in [ShellKind::Zsh, ShellKind::Bash, ShellKind::Fish] {
            let s = emitter(kind).emit(&Config::default()).unwrap();
            assert!(s.contains("shellcraft pick"), "{kind:?}");
            for key in ["_sc_hist_widget", "_sc_file_widget", "_sc_cd_widget"] {
                assert!(s.contains(key), "{kind:?} {key}");
            }
            // fzf is only mentioned by the optional fshow helper
            let fzf_lines = s
                .lines()
                .filter(|l| l.contains("fzf") && !l.trim_start().starts_with('#'))
                .count();
            assert!(fzf_lines <= 6, "{kind:?}: {fzf_lines} fzf lines");
        }
    }
}

mod parity_tests {
    use super::*;

    #[test]
    fn nu_and_pwsh_get_the_builtin_features() {
        for (kind, prompt) in [
            (ShellKind::Nushell, "shellcraft prompt --shell nu"),
            (ShellKind::Pwsh, "shellcraft prompt --shell pwsh"),
        ] {
            let on = emitter(kind).emit(&Config::default()).unwrap();
            let mut needles = vec![
                prompt,
                "jump add",
                "hist add",
                "shellcraft pick",
                "shellcraft files",
            ];
            if kind == ShellKind::Nushell {
                needles.push("suggest"); // PowerShell 7 has its own similar-command hint
            }
            for needle in needles {
                assert!(on.contains(needle), "{kind:?} lacks {needle}");
            }
            let off = emitter(kind)
                .emit(&Config::from_toml("[prompt]\nbuiltin = false\n").unwrap())
                .unwrap();
            assert!(!off.contains(prompt), "{kind:?} keeps the user's prompt");
        }
    }
}

mod rc_files {
    use super::*;
    use std::path::PathBuf;

    fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn each_shell_knows_its_rc_file() {
        let env = lookup(&[("HOME", "/h")]);
        let rc = |k| emitter(k).rc_file(&env);
        assert_eq!(rc(ShellKind::Zsh), Some(PathBuf::from("/h/.zshrc")));
        assert_eq!(rc(ShellKind::Bash), Some(PathBuf::from("/h/.bashrc")));
        assert_eq!(
            rc(ShellKind::Fish),
            Some(PathBuf::from("/h/.config/fish/config.fish"))
        );
        assert_eq!(
            rc(ShellKind::Nushell),
            Some(PathBuf::from("/h/.config/nushell/config.nu"))
        );
        assert_eq!(
            rc(ShellKind::Pwsh),
            Some(PathBuf::from(
                "/h/.config/powershell/Microsoft.PowerShell_profile.ps1"
            ))
        );
    }

    #[test]
    fn zdotdir_and_xdg_override_home() {
        let env = lookup(&[("HOME", "/h"), ("ZDOTDIR", "/z"), ("XDG_CONFIG_HOME", "/x")]);
        assert_eq!(
            emitter(ShellKind::Zsh).rc_file(&env),
            Some(PathBuf::from("/z/.zshrc"))
        );
        assert_eq!(
            emitter(ShellKind::Fish).rc_file(&env),
            Some(PathBuf::from("/x/fish/config.fish"))
        );
        let none = lookup(&[]);
        assert_eq!(emitter(ShellKind::Zsh).rc_file(&none), None);
        let empty = lookup(&[("HOME", "")]);
        assert_eq!(
            emitter(ShellKind::Bash).rc_file(&empty),
            None,
            "an empty HOME is no home"
        );
    }
}

mod slots {
    use super::*;
    use shellcraft_core::{InstalledPlugin, SlotPosition, SlotSnippet};

    fn with_slots(slots: Vec<SlotSnippet>) -> String {
        let mut cfg = Config::default();
        cfg.installed.push(InstalledPlugin {
            name: "demo".into(),
            slots,
            ..InstalledPlugin::default()
        });
        emitter(ShellKind::Zsh).emit(&cfg).unwrap()
    }

    fn slot(name: &str, position: SlotPosition, text: &str) -> SlotSnippet {
        SlotSnippet {
            slot: name.into(),
            position,
            text: text.into(),
        }
    }

    #[test]
    fn plugin_code_lands_before_and_after_the_named_slot() {
        let out = with_slots(vec![
            slot("aliases", SlotPosition::Before, "echo BEFORE_ALIASES\n"),
            slot("aliases", SlotPosition::After, "echo AFTER_ALIASES\n"),
        ]);
        let at = |needle: &str| {
            out.find(needle)
                .unwrap_or_else(|| panic!("missing {needle}"))
        };
        assert!(
            at("echo BEFORE_ALIASES") < at("# --- aliases"),
            "before the slot's own text"
        );
        assert!(at("# --- aliases") < at("echo AFTER_ALIASES"), "after it");
        assert!(
            at("echo AFTER_ALIASES") < at("# --- jump"),
            "and before the next slot"
        );
    }

    #[test]
    fn no_attached_code_means_no_change() {
        let plain = emitter(ShellKind::Zsh).emit(&Config::default()).unwrap();
        assert_eq!(with_slots(Vec::new()), plain);
    }

    #[test]
    fn every_shell_lists_slots_that_have_templates() {
        for kind in [
            ShellKind::Zsh,
            ShellKind::Bash,
            ShellKind::Fish,
            ShellKind::Nushell,
            ShellKind::Pwsh,
        ] {
            let e = emitter(kind);
            assert!(e.slots().len() >= 4, "{kind:?}");
            for slot in e.slots() {
                let name = format!("{}/{slot}", kind.name());
                assert!(
                    e.templates().iter().any(|(n, _)| *n == name),
                    "{name} has no template"
                );
            }
        }
    }
}
