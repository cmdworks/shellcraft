//! `shellcraft complete`: flag and subcommand candidates learned from `--help`.

use std::path::Path;

use crate::Policy;
use anyhow::{Context, Result};
use shellcraft_core::{cache, paths, Config};

/// Arguments of `shellcraft complete`.
#[derive(clap::Args)]
pub struct CompleteArgs {
    /// Forget everything learned so far
    #[arg(long)]
    pub clear: bool,
    /// The words typed so far; the last one is being completed
    #[arg(last = true)]
    pub words: Vec<String>,
}

/// Print `value<TAB>description` lines for the last word of `args.words`.
pub fn run(args: CompleteArgs, config_path: &Path) -> Result<()> {
    let CompleteArgs { clear, words } = args;
    let words = &words;
    let env = |k: &str| std::env::var(k).ok();
    let cache_dir = paths::cache_dir(env).context("cannot locate cache dir: set HOME")?;
    if clear {
        let dir = cache_dir.join("completions");
        if dir.is_dir() {
            std::fs::remove_dir_all(&dir)?;
        }
        println!("cleared {}", dir.display());
        return Ok(());
    }
    let cfg: Config = cache::load(&cache_dir.join("config.bin"), config_path).unwrap_or_default();
    if !cfg.completion.help {
        return Ok(());
    }
    let policy = Policy {
        skip: cfg.completion.skip.clone(),
    };
    let path = std::env::var("PATH").unwrap_or_default();
    for (value, desc) in crate::complete(words, &path, &cache_dir, &policy) {
        println!("{value}\t{desc}");
    }
    Ok(())
}
