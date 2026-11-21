//! Which shell to act on, and where its rc file is.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use shellcraft_core::{Config, Shell};

use crate::emitter;

/// Environment lookup used by the commands.
pub fn env(k: &str) -> Option<String> {
    std::env::var(k).ok()
}

/// Shell to act on: explicit flag, config default, then `$SHELL`.
pub fn pick_shell(flag: Option<&str>, cfg: &Config) -> Result<Shell> {
    if let Some(name) = flag {
        return Shell::parse(name).with_context(|| format!("unknown shell `{name}`"));
    }
    if let Some(s) = cfg.shell.default {
        return Ok(s);
    }
    let from_env = env("SHELL");
    let base = from_env
        .as_deref()
        .and_then(|p| Path::new(p).file_name())
        .and_then(|n| n.to_str());
    base.and_then(Shell::parse)
        .context("cannot tell which shell to use: pass --shell or set [shell] default")
}

/// The rc file of `shell` for the current environment.
pub fn rc_path(shell: Shell) -> Result<PathBuf> {
    emitter(shell).rc_file(&env).context("HOME is not set")
}
