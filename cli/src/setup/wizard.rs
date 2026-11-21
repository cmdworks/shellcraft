//! `shellcraft setup`: ask a few questions, write the config, apply it.

use std::fs;
use std::io::{BufRead, Write};
use std::path::Path;

use anyhow::{bail, Context, Result};
use shellcraft_core::{Config, Shell};

const TOOLS: &[&str] = &["fzf", "zoxide", "starship", "mise", "atuin"];

/// What the user chose.
struct Answers {
    shell: Option<Shell>,
    theme: String,
    builtin_prompt: bool,
    tools: Vec<String>,
    wire_rc: bool,
}

/// Config text for the answers.
fn render(a: &Answers) -> String {
    let mut out = String::from("# shellcraft configuration (written by `shellcraft setup`). Run `shellcraft apply` after editing.\n\n[shell]\n");
    if let Some(s) = a.shell {
        out += &format!("default = \"{}\"\n", s.name());
    }
    out += "history_size = 50000\n\n[tools]\n";
    for t in TOOLS {
        out += &format!("{t} = {}\n", a.tools.iter().any(|x| x == t));
    }
    out += &format!(
        "\n[prompt]\nbuiltin = {}\n\n[theme]\nname = \"{}\"\n",
        a.builtin_prompt, a.theme
    );
    out
}

fn ask(
    input: &mut impl BufRead,
    out: &mut impl Write,
    question: &str,
    default: &str,
) -> Result<String> {
    write!(out, "{question} [{default}]: ")?;
    out.flush()?;
    let mut line = String::new();
    input.read_line(&mut line)?;
    let line = line.trim();
    Ok(if line.is_empty() {
        default.to_string()
    } else {
        line.to_string()
    })
}

fn yes(s: &str) -> bool {
    matches!(s.to_lowercase().as_str(), "y" | "yes")
}

pub fn run(
    config_path: &Path,
    input: &mut impl BufRead,
    out: &mut impl Write,
    shell_hint: Option<String>,
) -> Result<()> {
    if config_path.exists() {
        let a = ask(
            input,
            out,
            &format!("{} exists; overwrite?", config_path.display()),
            "n",
        )?;
        if !yes(&a) {
            bail!("kept the existing config");
        }
    }
    let default_shell = shell_hint
        .and_then(|s| Shell::parse(s.rsplit('/').next().unwrap_or("")))
        .map_or("zsh", Shell::name);
    let shell_name = ask(input, out, "shell (zsh, bash, fish)", default_shell)?;
    let shell =
        Shell::parse(&shell_name).with_context(|| format!("unknown shell `{shell_name}`"))?;
    let themes = shellcraft_shell::available_themes(config_path);
    let theme = ask(
        input,
        out,
        &format!("theme ({})", themes.join(", ")),
        "default",
    )?;
    if !themes.contains(&theme) {
        bail!("unknown theme `{theme}` (install more with `shellcraft plugin add themes`)");
    }
    let builtin = yes(&ask(
        input,
        out,
        "use shellcraft's own prompt? (n keeps starship)",
        "y",
    )?);
    let tools_in = ask(
        input,
        out,
        &format!(
            "optional external tools to wire in, comma separated, or none ({})",
            TOOLS.join(", ")
        ),
        "none",
    )?;
    let tools: Vec<String> = tools_in
        .split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty() && t != "none")
        .collect();
    if let Some(bad) = tools.iter().find(|t| !TOOLS.contains(&t.as_str())) {
        bail!("unknown tool `{bad}`");
    }
    let wire_rc = yes(&ask(
        input,
        out,
        "add shellcraft to your shell rc file? (y/n)",
        "y",
    )?);
    let answers = Answers {
        shell: Some(shell),
        theme,
        builtin_prompt: builtin,
        tools,
        wire_rc,
    };

    let text = render(&answers);
    Config::from_toml(&text).context("generated config is invalid")?;
    if let Some(dir) = config_path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(config_path, &text).with_context(|| format!("writing {}", config_path.display()))?;
    writeln!(out, "wrote {}", config_path.display())?;
    crate::setup::apply::run(config_path, Some(shell.name()), false, answers.wire_rc)?;
    writeln!(
        out,
        "next: `shellcraft install` to install the tools you enabled, then restart your shell"
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendered_config_is_valid_and_reflects_answers() {
        let a = Answers {
            shell: Some(Shell::Fish),
            theme: "nord".into(),
            builtin_prompt: true,
            tools: vec!["fzf".into()],
            wire_rc: false,
        };
        let cfg = Config::from_toml(&render(&a)).expect("valid");
        assert_eq!(cfg.shell.default, Some(Shell::Fish));
        assert_eq!(cfg.theme.name, "nord");
        assert!(cfg.prompt.builtin);
        assert_eq!(cfg.tools.get("fzf"), Some(&true));
        assert_eq!(cfg.tools.get("atuin"), Some(&false));
    }

    #[test]
    fn rejects_bad_answers() {
        let mut out = Vec::new();
        let dir = std::env::temp_dir().join(format!("sc-setup-{}", std::process::id()));
        let cfg = dir.join("shellcraft.toml");
        let mut bad_theme = "zsh\nnope\n".as_bytes();
        assert!(run(&cfg, &mut bad_theme, &mut out, None).is_err());
        let mut bad_tool = "zsh\ndefault\nn\nnope\n".as_bytes();
        assert!(run(&cfg, &mut bad_tool, &mut out, None).is_err());
        assert!(!cfg.exists());
    }
}
