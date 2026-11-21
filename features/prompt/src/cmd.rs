//! `shellcraft prompt`: render the prompt string (hot path, reads the config cache only).

use std::path::PathBuf;

use crate::render::{render, segments, Input};
use anyhow::{Context, Result};
use shellcraft_core::{cache, paths, Config, Shell};

/// Arguments of `shellcraft prompt`.
#[derive(clap::Args)]
pub struct PromptArgs {
    /// zsh, bash or fish
    #[arg(long)]
    pub shell: String,
    /// Exit status of the last command
    #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
    pub status: i32,
    /// Duration of the last command in milliseconds
    #[arg(long, default_value_t = 0)]
    pub duration: u64,
    /// `user@host` to show (the shell passes it in SSH sessions)
    #[arg(long)]
    pub remote: Option<String>,
    /// Print only the plugin segments (git, versions, ...), each preceded by a space
    #[arg(long)]
    pub segments_only: bool,
}

pub fn run(config_path: &std::path::Path, args: PromptArgs) -> Result<()> {
    let PromptArgs {
        shell,
        status,
        duration,
        remote,
        segments_only,
    } = args;
    let shell = Shell::parse(&shell).with_context(|| format!("unknown shell `{shell}`"))?;
    let env = |k: &str| std::env::var(k).ok();
    // Cache only: a missing or stale cache falls back to defaults rather than parsing TOML.
    let cfg: Config = paths::cache_dir(env)
        .and_then(|d| cache::load(&d.join("config.bin"), config_path))
        .unwrap_or_default();
    let input = Input {
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
        home: std::env::var_os("HOME").map(PathBuf::from),
        status,
        duration_ms: duration,
        remote,
        cache_dir: paths::cache_dir(env),
    };
    let draw = if segments_only { segments } else { render };
    print!(
        "{}",
        draw(shell, &cfg.prompt, &cfg.plugin, &cfg.palette(), &input)
    );
    Ok(())
}
