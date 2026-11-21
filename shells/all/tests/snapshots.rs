//! Snapshot tests: one full init script per shell for a fixed config.

use shellcraft_core::Config;
use shellcraft_shell::{attach_bundled, emitter, ShellKind};

/// Render like `apply` does (bundled plugins attached), without reading any installed plugins.
fn render(kind: ShellKind, mut cfg: Config) -> String {
    attach_bundled(&mut cfg, kind);
    emitter(kind).emit(&cfg).expect("emit")
}

fn sample(keys: &str) -> Config {
    Config::from_toml(&format!(
        r#"
[shell]
history_size = 5000
[tools]
starship = true
zoxide = true
fzf = true
atuin = true
[aliases]
ll = "ls -lah"
[env]
EDITOR = "nvim"
GREETING = "it's here"
{keys}
"#
    ))
    .expect("sample config")
}

#[test]
fn zsh() {
    let cfg = sample("[keys]\n\"ctrl-g\" = \"push-line\"");
    insta::assert_snapshot!(render(ShellKind::Zsh, cfg));
}

#[test]
fn bash() {
    let cfg = sample("[keys]\n\"ctrl-g\" = \"kill-whole-line\"");
    insta::assert_snapshot!(render(ShellKind::Bash, cfg));
}

#[test]
fn fish() {
    let cfg = sample("[keys]\n\"ctrl-g\" = \"kill-whole-line\"");
    insta::assert_snapshot!(render(ShellKind::Fish, cfg));
}

#[test]
fn nu() {
    insta::assert_snapshot!(emitter(ShellKind::Nushell).emit(&sample("")).expect("emit"));
}

#[test]
fn pwsh() {
    let cfg = sample("[keys]\n\"ctrl-g\" = \"KillLine\"");
    insta::assert_snapshot!(render(ShellKind::Pwsh, cfg));
}
