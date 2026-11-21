//! nushell and PowerShell: output checks, and syntax checks when the shells are installed.

#![allow(clippy::unwrap_used)] // test helpers

use shellcraft_core::Config;
use shellcraft_shell::{emitter, ShellKind};
use std::process::Command;

fn sample() -> Config {
    Config::from_toml(
        r#"
            [shell]
            history_size = 321
            [prompt]
            builtin = false
            [tools]
            starship = true
            [aliases]
            ll = "ls -la"
            [env]
            EDITOR = "nv'im"
            "#,
    )
    .unwrap()
}

#[test]
fn nu_output() {
    let out = emitter(ShellKind::Nushell).emit(&sample()).unwrap();
    assert!(out.contains("$env.config.history.max_size = 321"));
    assert!(out.contains("$env.EDITOR = r#'nv'im'#"));
    assert!(out.contains("alias ll = ls -la"));
    assert!(out.contains("# tool starship: no init support"));
}

#[test]
fn nu_rejects_keys() {
    let mut cfg = Config::default();
    cfg.keys.insert("ctrl-g".into(), "x".into());
    assert!(emitter(ShellKind::Nushell).emit(&cfg).is_err());
}

#[test]
fn pwsh_output() {
    let mut cfg = sample();
    cfg.keys.insert("ctrl-g".into(), "KillLine".into());
    let out = emitter(ShellKind::Pwsh).emit(&cfg).unwrap();
    assert!(out.contains("Set-PSReadLineOption -MaximumHistoryCount 321"));
    assert!(out.contains("$env:EDITOR = 'nv''im'"));
    assert!(out.contains("function ll { ls -la @args }"));
    assert!(out.contains("Get-Command starship"));
    assert!(out.contains("Set-PSReadLineKeyHandler -Chord 'Ctrl+g' -Function 'KillLine'"));
}

/// Run only where the shells are installed; they were not available when this was written.
#[test]
fn syntax_checks_if_shells_exist() {
    let nu = emitter(ShellKind::Nushell).emit(&sample()).unwrap();
    let dir = std::env::temp_dir().join(format!("shellcraft-exp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("init.nu");
    std::fs::write(&f, nu).unwrap();
    if let Ok(o) = Command::new("nu")
        .args(["--no-config-file", "--commands"])
        .arg(format!("nu-check '{}'", f.display()))
        .output()
    {
        let stdout = String::from_utf8_lossy(&o.stdout);
        // nu-check prints true or false and exits 0 either way.
        assert!(
            stdout.trim() == "true",
            "{stdout} {}",
            String::from_utf8_lossy(&o.stderr)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
