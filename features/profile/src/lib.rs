//! A profile is a git repository (or folder) holding your `shellcraft.toml` and `shellcraft.lock`.
//!
//! `shellcraft pull <repo>` replaces your config with the profile's and installs exactly the
//! plugins pinned in its lock file (same source, same commit, same content hash), so a new
//! machine is one command. `shellcraft push` commits your current config and lock back.

use shellcraft_core::proc::now_secs as now;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Args;
use shellcraft_core::{paths, Config};
use shellcraft_plugin::store::{self, Lock, LockEntry};
use shellcraft_plugins_cli::{confirm, fetch_source, git, install, Host, Spec};

const CONFIG_FILE: &str = "shellcraft.toml";
const LOCK_FILE: &str = "shellcraft.lock";

/// Arguments of `shellcraft pull`.
#[derive(Args)]
pub struct PullArgs {
    /// The profile: a folder, `owner/repo[@rev]` or a git URL
    pub source: String,
    /// Do not ask for confirmation
    #[arg(long)]
    pub yes: bool,
}

/// Arguments of `shellcraft push`.
#[derive(Args)]
pub struct PushArgs {
    /// Commit message
    #[arg(short, long)]
    pub message: Option<String>,
}

fn profile_dir() -> Result<PathBuf> {
    let data =
        paths::data_dir(|k| std::env::var(k).ok()).context("cannot locate data dir: set HOME")?;
    Ok(data.join("profile"))
}

/// A profile is cloned (so `push` has a repository to commit in), including a local git
/// repository or a bare one; only a plain folder is copied.
fn fetch_profile(source: &str, dest: &Path) -> Result<String> {
    let p = Path::new(source);
    let is_git_dir =
        p.join(".git").exists() || (p.join("HEAD").is_file() && p.join("objects").is_dir());
    if p.is_dir() && is_git_dir {
        git(None, &["clone", "--quiet", source, &dest.to_string_lossy()])?;
        return git(Some(dest), &["rev-parse", "HEAD"]);
    }
    fetch_source(source, None, dest)
}

/// Entries that can be reproduced on another machine: not linked for development, not a local copy.
fn reproducible(e: &LockEntry) -> bool {
    !e.dev && e.rev != "local" && e.rev != "link"
}

pub fn pull(args: PullArgs, host: &Host) -> Result<()> {
    let dir = profile_dir()?;
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.parent().unwrap_or(Path::new(".")))?;
    let rev = fetch_profile(&args.source, &dir)?;
    let config_text = fs::read_to_string(dir.join(CONFIG_FILE))
        .with_context(|| format!("the profile has no {CONFIG_FILE}"))?;
    Config::from_toml(&config_text).context("the profile's shellcraft.toml is invalid")?;
    let lock = Lock::load(&dir.join(LOCK_FILE));
    let (wanted, skipped): (Vec<_>, Vec<_>) = lock.plugin.iter().partition(|e| reproducible(e));

    println!("profile {} @ {}", args.source, &rev[..rev.len().min(12)]);
    println!("  config:  replaces {}", host.config_path.display());
    for e in &wanted {
        println!(
            "  plugin:  {} {} from {} @ {}",
            e.name,
            e.version,
            e.source,
            &e.rev[..e.rev.len().min(12)]
        );
        for line in &e.approved {
            println!("             {line}");
        }
    }
    for e in &skipped {
        println!(
            "  skipped: {} ({}), it cannot be reproduced from a repository",
            e.name,
            if e.dev { "linked" } else { "local copy" }
        );
    }
    if !args.yes && !confirm("Replace your config and install these plugins?") {
        bail!("nothing changed");
    }

    if host.config_path.exists() {
        let backup = host
            .config_path
            .with_extension(format!("toml.pre-profile-{}", now()));
        fs::copy(host.config_path, &backup)?;
        println!("backup {}", backup.display());
    }
    if let Some(parent) = host.config_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(host.config_path, config_text)?;

    // Install in passes so a plugin that `requires` another one waits for it.
    let mine = Lock::load(&store::lock_path(host.config_path));
    let mut pending: Vec<&LockEntry> = wanted
        .iter()
        .copied()
        .filter(|e| mine.get(&e.name).map_or(true, |m| m.hash != e.hash))
        .collect();
    let mut last_error = String::new();
    while !pending.is_empty() {
        let before = pending.len();
        let mut still = Vec::new();
        for e in pending {
            let spec = Spec {
                source: e.source.clone(),
                rev: Some(e.rev.clone()),
                expect_hash: Some(e.hash.clone()),
            };
            if let Err(err) = install(&spec, true, host) {
                last_error = format!("{}: {err:#}", e.name);
                still.push(e);
            }
        }
        if still.len() == before {
            bail!(
                "could not install {} plugin(s); last error: {last_error}",
                still.len()
            );
        }
        pending = still;
    }
    println!("profile applied");
    Ok(())
}

pub fn push(args: PushArgs, host: &Host) -> Result<()> {
    let dir = profile_dir()?;
    if !dir.join(".git").exists() {
        bail!("no profile to push to: run `shellcraft pull <repo>` first (the profile must be a git repository)");
    }
    fs::copy(host.config_path, dir.join(CONFIG_FILE))
        .with_context(|| format!("reading {}", host.config_path.display()))?;
    let mut lock = Lock::load(&store::lock_path(host.config_path));
    lock.plugin.retain(reproducible);
    lock.save(&dir.join(LOCK_FILE))?;

    git(Some(&dir), &["add", "-A"])?;
    if git(Some(&dir), &["status", "--porcelain"])?.is_empty() {
        println!("profile is up to date");
        return Ok(());
    }
    let message = args
        .message
        .as_deref()
        .unwrap_or("update shellcraft profile");
    git(Some(&dir), &["commit", "--quiet", "-m", message])?;
    if git(Some(&dir), &["remote"])?.lines().any(|r| r == "origin") {
        git(Some(&dir), &["push", "--quiet", "origin", "HEAD"])?;
        println!("pushed to origin");
    } else {
        println!("committed in {} (no remote to push to)", dir.display());
    }
    Ok(())
}
