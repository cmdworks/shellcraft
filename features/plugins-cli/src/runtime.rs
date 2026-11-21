//! Plugin commands (`shellcraft <name>`) and hooks, and the command-name conflict check.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use super::Host;
use anyhow::{bail, Context, Result};
use shellcraft_core::paths;
use shellcraft_plugin::manifest::Manifest;

pub(super) fn cache_dir() -> Result<PathBuf> {
    paths::cache_dir(|k| std::env::var(k).ok()).context("cannot locate cache dir: set HOME")
}

/// `shellcraft <name> args...` for a command of an installed plugin. Exits with its status.
pub fn run_external(args: &[String], host: &Host) -> Result<()> {
    let name = args.first().map(String::as_str).unwrap_or_default();
    let found = shellcraft_plugin::store::load_commands(&cache_dir()?)
        .into_iter()
        .find(|c| c.name == name);
    let Some(cmd) = found else {
        let mut known: Vec<String> = host.builtin_commands.to_vec();
        known.extend(
            shellcraft_plugin::store::load_commands(&cache_dir()?)
                .into_iter()
                .map(|c| c.name),
        );
        let hint = known
            .iter()
            .min_by_key(|k| shellcraft_not_found::distance(name, k))
            .filter(|k| shellcraft_not_found::distance(name, k) <= 2);
        match hint {
            Some(k) => bail!("unknown command `{name}`; did you mean `{k}`?"),
            None => bail!(
                "unknown command `{name}` (see `shellcraft --help` and `shellcraft plugin list`)"
            ),
        }
    };
    let status = std::process::Command::new(&cmd.path)
        .args(&args[1..])
        .status()
        .with_context(|| format!("running {} (plugin {})", cmd.path, cmd.plugin))?;
    std::process::exit(status.code().unwrap_or(1));
}

/// Start the hooks of installed plugins for `event`, detached, with the event in the environment.
/// Arguments of `shellcraft hook` (the shell calls it; hidden from help).
#[derive(clap::Args)]
pub struct HookArgs {
    /// cd or post
    pub event: String,
    #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
    pub exit: i32,
    #[arg(long, default_value_t = 0)]
    pub duration: u64,
    #[arg(long, default_value = "")]
    pub cmd: String,
    #[arg(long, default_value = "")]
    pub old: String,
}

pub fn run_hooks(args: HookArgs) -> Result<()> {
    let HookArgs {
        event,
        exit,
        duration,
        cmd,
        old,
    } = args;
    let (event, cmd, old) = (event.as_str(), cmd.as_str(), old.as_str());
    let Ok(dir) = cache_dir() else { return Ok(()) };
    let cwd = std::env::current_dir().unwrap_or_default();
    for hook in shellcraft_plugin::store::load_hooks(&dir)
        .into_iter()
        .filter(|h| h.event == event)
    {
        let _ = std::process::Command::new(&hook.path)
            .current_dir(&cwd)
            .env("SHELLCRAFT_EVENT", event)
            .env("SHELLCRAFT_CWD", &cwd)
            .env("SHELLCRAFT_OLDPWD", old)
            .env("SHELLCRAFT_EXIT", exit.to_string())
            .env("SHELLCRAFT_DURATION_MS", duration.to_string())
            .env("SHELLCRAFT_COMMAND", cmd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
    Ok(())
}

/// Command names that cannot be taken: built-ins and commands of other installed plugins.
pub(super) fn command_conflicts(m: &Manifest, cache_dir: &Path, builtin: &[String]) -> Vec<String> {
    let others = shellcraft_plugin::store::load_commands(cache_dir);
    m.command
        .keys()
        .filter_map(|n| {
            if builtin.contains(n) {
                Some(format!("`{n}` is a built-in command"))
            } else {
                others
                    .iter()
                    .find(|c| c.name == *n && c.plugin != m.name)
                    .map(|c| format!("`{n}` is already provided by plugin {}", c.plugin))
            }
        })
        .collect()
}
