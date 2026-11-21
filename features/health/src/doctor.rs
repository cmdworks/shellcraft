//! `shellcraft doctor`: report problems with the setup. Exits non-zero if any check fails.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Result};
use shellcraft_core::{paths, Config, Shell};
use shellcraft_shell::rc;

use shellcraft_shell::{env, pick_shell, rc_path};

struct Report {
    failed: u32,
}

impl Report {
    fn ok(&self, msg: &str) {
        println!("  ok    {msg}");
    }
    fn warn(&self, msg: &str) {
        println!("  warn  {msg}");
    }
    fn fail(&mut self, msg: &str) {
        self.failed += 1;
        println!("  FAIL  {msg}");
    }
}

fn on_path(bin: &str) -> Option<PathBuf> {
    let path = env("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(bin))
        .find(|p| p.is_file())
}

/// Arguments of `shellcraft doctor`.
#[derive(clap::Args)]
pub struct DoctorArgs {
    /// zsh, bash or fish (default: [shell] default, then $SHELL)
    #[arg(long)]
    pub shell: Option<String>,
}

pub fn run(config_path: &Path, args: DoctorArgs) -> Result<()> {
    let shell = args.shell.as_deref();
    let mut r = Report { failed: 0 };
    println!("shellcraft {} doctor", env!("CARGO_PKG_VERSION"));

    let cfg = match fs::read_to_string(config_path) {
        Ok(text) => match Config::from_toml(&text) {
            Ok(cfg) => {
                r.ok(&format!("config {} is valid", config_path.display()));
                cfg
            }
            Err(e) => {
                r.fail(&format!("config {}: {e}", config_path.display()));
                bail!("doctor found 1 problem");
            }
        },
        Err(_) => {
            r.fail(&format!(
                "no config at {} (run `shellcraft init`)",
                config_path.display()
            ));
            bail!("doctor found 1 problem");
        }
    };

    for (name, on) in &cfg.tools {
        if !on {
            continue;
        }
        match on_path(name) {
            Some(p) => r.ok(&format!("tool {name}: {}", p.display())),
            None => r.warn(&format!(
                "tool {name} is enabled but not on PATH (skipped at shell start)"
            )),
        }
    }

    let shell = match pick_shell(shell, &cfg) {
        Ok(s) => s,
        Err(e) => {
            r.fail(&format!("{e}"));
            return finish(r);
        }
    };

    let mut cfg = cfg;
    let (expected, _) = shellcraft_shell::generate(&mut cfg, shell, config_path)?;
    let init = paths::cache_dir(env).map(|d| d.join(format!("init.{}", shell.file_ext())));
    match init.as_deref().map(fs::read_to_string) {
        Some(Ok(actual)) if actual == expected => r.ok(&format!(
            "init script is up to date ({})",
            init.as_deref()
                .map_or_else(String::new, |p| p.display().to_string())
        )),
        Some(Ok(_)) => r.fail("init script is stale (run `shellcraft apply`)"),
        _ => r.fail("init script missing (run `shellcraft apply`)"),
    }

    match shell_syntax_check(shell, &expected) {
        Some(Ok(())) => r.ok(&format!(
            "{} accepts the generated script (-n)",
            shell.name()
        )),
        Some(Err(e)) => r.fail(&format!(
            "{} rejects the generated script: {e}",
            shell.name()
        )),
        None => r.warn(&format!("{} not found, syntax check skipped", shell.name())),
    }

    match rc_path(shell) {
        Ok(path) => match fs::read_to_string(&path) {
            Ok(t) if t.contains(rc::MARK_BEGIN) => r.ok(&format!("{} is wired", path.display())),
            _ => r.fail(&format!(
                "{} is not wired (run `shellcraft apply --rc`)",
                path.display()
            )),
        },
        Err(e) => r.fail(&format!("{e}")),
    }
    finish(r)
}

fn finish(r: Report) -> Result<()> {
    if r.failed > 0 {
        bail!("doctor found {} problem(s)", r.failed);
    }
    println!("all good");
    Ok(())
}

/// `None` if the shell binary is not installed.
fn shell_syntax_check(shell: Shell, script: &str) -> Option<Result<(), String>> {
    let tmp = std::env::temp_dir().join(format!(
        "shellcraft-doctor-{}.{}",
        std::process::id(),
        shell.name()
    ));
    fs::write(&tmp, script).ok()?;
    let out = Command::new(shell.name()).arg("-n").arg(&tmp).output();
    let _ = fs::remove_file(&tmp);
    match out {
        Err(_) => None,
        Ok(o) if o.status.success() => Some(Ok(())),
        Ok(o) => Some(Err(String::from_utf8_lossy(&o.stderr).trim().to_string())),
    }
}
