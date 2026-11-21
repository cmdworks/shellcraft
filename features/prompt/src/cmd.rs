//! `shellcraft prompt`: render the prompt string (hot path, reads the config cache only).

use std::path::PathBuf;

use crate::render::{render, render_layout, segments, Input, LAYOUT_ENV};
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
    /// The shell runs as root (themes with a layout can warn about it)
    #[arg(long)]
    pub root: bool,
    /// The current directory is not writable
    #[arg(long)]
    pub readonly: bool,
    /// Number of background jobs
    #[arg(long, default_value_t = 0)]
    pub jobs: u32,
    /// Print the right-hand prompt only (themes with a layout; empty otherwise)
    #[arg(long)]
    pub right: bool,
    /// Print the left prompt, a unit separator (0x1f), then the right prompt
    #[arg(long)]
    pub both: bool,
}

pub fn run(config_path: &std::path::Path, args: PromptArgs) -> Result<()> {
    let PromptArgs {
        shell,
        status,
        duration,
        remote,
        segments_only,
        root,
        readonly,
        jobs,
        right,
        both,
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
        root,
        readonly,
        jobs,
        env: LAYOUT_ENV
            .iter()
            .filter_map(|k| std::env::var(k).ok().map(|v| ((*k).to_string(), v)))
            .collect(),
    };
    let pal = cfg.palette();
    if segments_only {
        print!(
            "{}",
            segments(shell, &cfg.prompt, &cfg.plugin, &pal, &input)
        );
        return Ok(());
    }
    match cfg.layout() {
        Some(l) => {
            let (left, r) = render_layout(shell, &cfg.prompt, &cfg.plugin, &pal, l, &input);
            if both {
                print!("{left}\u{1f}{r}");
            } else if right {
                print!("{r}");
            } else {
                print!("{left}");
            }
        }
        None => {
            // no layout: the right side is empty and the left is the classic prompt
            let left = render(shell, &cfg.prompt, &cfg.plugin, &pal, None, &input);
            if both {
                print!("{left}\u{1f}");
            } else if !right {
                print!("{left}");
            }
        }
    }
    Ok(())
}
